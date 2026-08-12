//! 证书轮换状态机 —— 本 crate 的架构重心。
//!
//! WebTransport 的传输层几乎是把 `wtransport` 的 async API 翻译成 libp2p 的 poll API，
//! 机械劳动；真正有状态、有时钟、驱动外部可见行为（通告地址）的只有这里。
//!
//! # 时钟从参数进来，不从系统读
//!
//! **这不是风格选择，是验收标准可测性的硬约束。** `current` 的有效期是 14 天：若本模块
//! 内部读 `SystemTime::now()`，「跨过期切换」这条行为就只能靠等 14 天、改系统时钟或干脆
//! 不测来验证，而项目要求护栏测试必须能红。时钟注入把它变成一次
//! `advance(now + 15 天)` 调用。
//!
//! 代价是调用方必须记得推进它 —— 由 `Transport::poll` 每轮顺带完成（见 crate 文档）。
//!
//! # 为什么同时通告两张，以及由此得到的 28 天
//!
//! spec 要求通告地址携带 `current` 与 `next` 两个 certhash。把时间轴画开：
//!
//! ```text
//! 第 0 天    通告 [A, B]  ← 客户端记下这条地址
//! 第 14 天   A 过期 → 通告 [B, C]
//!            客户端持旧地址拨 → 服务端实际用 B → B ∈ 客户端接受集合 {A,B} → 连得上
//! 第 28 天   B 过期 → 通告 [C, D]
//!            客户端持旧地址拨 → 服务端实际用 C → C ∉ {A,B} → 断
//! ```
//!
//! 即**一条通告地址的实际寿命是两个轮换周期（28 天），不是一个**。这条推论直接决定了
//! 上层「多久要刷新一次 bootstrap 清单」的判断，由 `advertised_addr_survives_one_rotation`
//! 钉死。

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, SystemTime};

use libp2p_core::multihash::Multihash;

use super::cert::{Certificate, Error, MAX_VALIDITY};
use crate::addr::Certhash;

/// 保留多少张已退役证书的 certhash。
///
/// spec 建议 Noise 扩展里也带上近期过期的证书哈希，好让持旧地址的客户端仍能通过验证。
/// 取 2（约一个月）：通告地址的寿命本就只有 28 天（见模块文档），再老的 hash 不可能有人
/// 还在用，白占握手载荷。
const RETIRED_KEEP: usize = 2;

/// `next` 相对 `current` 提前生效的量，也是轮换的提前量。
///
/// # 为什么两张证书必须**重叠**而不是首尾相接
///
/// 轮换由 poll 里一个 [`ROTATION_CHECK_INTERVAL`](crate::ROTATION_CHECK_INTERVAL) 的定时器
/// 驱动，所以「该换了」与「真的换了」之间必然有最长一个检查周期的滞后。若判据是「`current`
/// **已经过期**才换」，那段滞后里服务端出示的就是一张过期证书 —— 客户端（浏览器与
/// `wtransport` 的 `ServerHashVerification` 都一样）直接判 `Expired`，**期间所有新连接
/// 一律 TLS 失败**，而错误只说「证书不受信」。每 14 天来一次。
///
/// 解法是让 `next` 提前生效，并在 `current` 过期**之前**就切过去 —— 切换点落在两张都有效
/// 的重叠区里，任何时刻都有一张能用的证书。
///
/// 取 1 小时：必须远大于检查间隔（见 `ROTATION_CHECK_INTERVAL`，当前 10 分钟），
/// 又远小于 14 天（否则白白缩短有效覆盖）。
const ROTATION_OVERLAP: Duration = Duration::from_secs(60 * 60);

/// 一次 [`Rotation::advance`] 的结果。
#[derive(Debug, PartialEq, Eq)]
pub enum Advance {
    /// 未到期，什么都没做。
    Idle,
    /// `current` 已过期：`next` 已提升为 `current`，并已生成新的 `next`。
    ///
    /// 调用方必须做三件事：换掉服务端 TLS 配置、回写持久化、更新通告地址。
    Rotated {
        /// 刚退役的那张证书的 certhash。**仅供日志/诊断**。
        ///
        /// ⚠️ 它**不是** `AddressExpired` 的事实源 —— 那个必须用
        /// `Listener::announced` 记下的**实际发出去的那一份**地址，因为网卡列表可能在两次
        /// 通告之间变过，事后由 certhash 重算会算出一条从未通告过的地址。
        retired: Certhash,
    },
}

/// 一对（当前 + 未来）证书及其轮换规则。
///
/// **纯逻辑**：不持有时钟、不做 IO、不认识 libp2p 的 `Transport`。
pub struct Rotation {
    current: Certificate,
    next: Certificate,
    /// 近期退役的证书，新的在前。
    ///
    /// **存整张而不只是 certhash**：它必须跟着 PEM 一起持久化。只存 hash 的话格式里放不下
    /// （多段 PEM 里没有「裸哈希」这种段），而丢掉它的后果见 [`Self::noise_certhashes`]。
    retired: VecDeque<Certificate>,
    /// [`Self::noise_certhashes`] 的缓存。
    ///
    /// 它**每条入站连接**都要用一次，而内容 14 天才变一次。不缓存的话每次要重新
    /// 组一个 Vec、算 4 次 `Multihash::wrap`、再插进一张 HashSet。
    noise_certhashes: HashSet<Multihash<64>>,
}

impl Rotation {
    /// 首次启动：生成两张首尾相接的证书。
    ///
    /// `next` 比 `current` 的过期时刻**提前 [`ROTATION_OVERLAP`] 生效** —— 切换点因此落在
    /// 两张都有效的重叠区里，不存在「已过期但还没换」的空窗。理由见那个常量的文档。
    pub fn bootstrap(now: SystemTime) -> Result<Self, Error> {
        let current = Certificate::generate(now, MAX_VALIDITY)?;
        let next = Certificate::generate(next_starts_at(&current), MAX_VALIDITY)?;
        Ok(Self::assemble(current, next, VecDeque::new()))
    }

    /// 唯一的组装入口 —— 缓存字段因此只有一处需要维护。
    fn assemble(current: Certificate, next: Certificate, retired: VecDeque<Certificate>) -> Self {
        let mut rotation = Self {
            current,
            next,
            retired,
            noise_certhashes: HashSet::new(),
        };
        rotation.rebuild_noise_certhashes();
        rotation
    }

    /// 重算 [`Self::noise_certhashes`]。三个来源（current / next / retired）任一变化后必须调。
    fn rebuild_noise_certhashes(&mut self) {
        self.noise_certhashes = [self.current.certhash(), self.next.certhash()]
            .into_iter()
            .chain(self.retired.iter().map(Certificate::certhash))
            .map(Certhash::to_multihash)
            .collect();
    }

    /// 从持久化的多段 PEM 还原。
    ///
    /// 格式就是「证书段与私钥段按同一顺序各自罗列」，**没有自定义元数据** —— 有效期本就
    /// 编码在 X.509 里。还原后按 `not_before` 升序确定谁是 `current`：写入顺序万一被外部
    /// 工具打乱，凭有效期仍能恢复出正确的角色。
    ///
    /// 只还原，**不判断是否过期** —— 那是 [`Self::advance`] 的唯一职责。调用方 load 之后
    /// 紧接着 advance 一次即可，两者的分工因此不重叠。
    ///
    /// 返回的 `bool` 是「**是否完整还原**」：为 `false` 时说明内容不足、有东西是现生成的，
    /// 调用方**必须立刻回写**。否则那份不完整的数据会自我延续 —— 每次启动都补生成一张
    /// 不同的 `next`，通告地址的第二个 certhash 每次都变，而上一轮通告出去的地址会在
    /// Noise 阶段失败（判据是「期望集合 ⊆ 收到集合」）。
    pub fn from_pem(pem: &str) -> Result<(Self, bool), Error> {
        use rustls_pki_types::pem::PemObject;
        use rustls_pki_types::{CertificateDer, PrivateKeyDer};

        let certs: Vec<_> = CertificateDer::pem_slice_iter(pem.as_bytes())
            .collect::<Result<_, _>>()
            .map_err(|e| Error::Pem(e.to_string()))?;
        let keys: Vec<_> = PrivateKeyDer::pem_slice_iter(pem.as_bytes())
            .collect::<Result<_, _>>()
            .map_err(|e| Error::Pem(e.to_string()))?;

        if certs.len() != keys.len() {
            return Err(Error::Unpaired {
                certs: certs.len(),
                keys: keys.len(),
            });
        }

        // 配对靠**顺序**：这是 rustls / wtransport 生态里 cert.pem + key.pem 的既有惯例。
        // 排序放在配对之后，两者因此始终一起移动。
        let mut loaded = certs
            .into_iter()
            .zip(keys)
            .map(|(cert, key)| Certificate::from_der(cert.to_vec(), key))
            .collect::<Result<Vec<_>, _>>()?;
        loaded.sort_by_key(Certificate::not_before);

        // 排序后：最后两张是 current / next，更早的都是退役的（新的在前）。
        let newest = loaded.pop().ok_or(Error::EmptyChain)?;
        let (current, next, complete) = match loaded.pop() {
            Some(older) => (older, newest, true),
            // 只存了一张也能恢复：把它当 current、next 现生成。比整体重建强 ——
            // 那张仍然有效的证书对应的通告地址还在对端手里。**但这不算完整还原。**
            None => {
                let generated = Certificate::generate(next_starts_at(&newest), MAX_VALIDITY)?;
                (newest, generated, false)
            }
        };

        loaded.reverse();
        let mut retired: VecDeque<Certificate> = loaded.into();
        retired.truncate(RETIRED_KEEP);

        Ok((Self::assemble(current, next, retired), complete))
    }

    /// 序列化成多段 PEM，供宿主持久化。顺序：退役的在前，`current` / `next` 在后。
    ///
    /// **退役证书必须一起写。** 它们在 Noise 扩展里给持上一轮地址的客户端兜底，只活在
    /// 内存里的话，**一次重启就把「旧地址能撑过一整轮」这条契约打掉**：TLS 仍会通过
    /// （服务端出示的 current 在客户端的接受集合内），Noise 却会失败。
    pub fn to_pem(&self) -> String {
        self.retired
            .iter()
            .rev()
            .chain([&self.current, &self.next])
            .map(Certificate::to_pem)
            .collect()
    }

    /// 推进时钟。
    ///
    /// 幂等：同一个 `now` 连续调用只轮换一次 —— 第二次 `current` 已经有效，走 `Idle`。
    ///
    /// 三条路径：
    ///
    /// | `current` 在 `now` 是否有效 | `next` 在 `now` 是否有效 | 结果 |
    /// |---|---|---|
    /// | 是 | — | `Idle` |
    /// | 否 | 是 | `next` 提升为 `current`，生成新 `next` |
    /// | 否 | 否 | **整体重建** |
    ///
    /// 第三行覆盖两种真实情况：设备关机超过 28 天，以及系统时钟被拨到证书生效之前。
    /// 判据统一成「`current` 在 `now` 有效吗」而不是「过期了吗」，时钟倒退因此自动落进
    /// 重建分支，不必单独判一次。
    pub fn advance(&mut self, now: SystemTime) -> Result<Advance, Error> {
        // 判据有两半：`current` 得是有效的，**且**还没到提前切换点。
        // 少了后半句，切换只会发生在过期之后 —— 那正是那段「新连接全拒」的空窗。
        if self.current.is_valid_at(now) && now < next_starts_at(&self.current) {
            return Ok(Advance::Idle);
        }

        let retired = self.current.certhash();
        let retiring = self.current.clone();

        if self.next.is_valid_at(now) {
            // 新的 next 同样提前生效，下一轮切换才有重叠区可用。
            let regenerated = Certificate::generate(next_starts_at(&self.next), MAX_VALIDITY)?;
            self.current = std::mem::replace(&mut self.next, regenerated);
            tracing::info!(
                retired = ?retired,
                current = ?self.current.certhash(),
                next = ?self.next.certhash(),
                "webtransport 证书轮换：next 已提升为 current"
            );
        } else {
            // 两张都不可用。重建而不是链式推进：关机 28 天以上时链式推进要生成任意多张
            // 中间证书，而它们没有任何一个对端见过。
            let rebuilt = Self::bootstrap(now)?;
            tracing::warn!(
                retired = ?retired,
                current = ?rebuilt.current.certhash(),
                "webtransport 证书全部失效（长时间停机或系统时钟变动），已整体重建；\
                 对端记下的旧地址将失效"
            );
            self.current = rebuilt.current;
            self.next = rebuilt.next;
        }

        self.retired.push_front(retiring);
        self.retired.truncate(RETIRED_KEEP);
        // 三个来源都变了。**漏掉这一行的后果**：Noise 握手继续上报轮换前的集合，
        // 于是持上一轮地址的客户端 TLS 过得去、Noise 却过不去（判据是「期望 ⊆ 收到」）。
        self.rebuild_noise_certhashes();
        Ok(Advance::Rotated { retired })
    }

    /// 通告地址里那串 certhash：`current` 在前，`next` 在后。
    pub fn advertised(&self) -> Vec<Certhash> {
        vec![self.current.certhash(), self.next.certhash()]
    }

    /// Noise 的 `webtransport_certhashes` 扩展要上报的集合。
    ///
    /// 比 [`Self::advertised`] 多出近期退役的那些：客户端可能持有上一轮的通告地址，
    /// 它期望的集合里含已退役的 hash，服务端不带上就会把它拒之门外。
    ///
    /// 构造期算好，每条入站连接直接借用（调用方按需 clone）。
    pub fn noise_certhashes(&self) -> &HashSet<Multihash<64>> {
        &self.noise_certhashes
    }

    /// 当前生效的那张，交给 `wtransport` 起监听 / reload 用。
    pub(crate) fn server_identity(&self) -> wtransport::Identity {
        self.current.identity()
    }

    /// `current` 的过期时刻。诊断用（「还有几天轮换」）。
    pub fn current_expires_at(&self) -> SystemTime {
        self.current.not_after()
    }
}

/// 接替 `cert` 的那张证书应当何时生效。
fn next_starts_at(cert: &Certificate) -> SystemTime {
    cert.not_after() - ROTATION_OVERLAP
}

impl std::fmt::Debug for Rotation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rotation")
            .field("current", &self.current)
            .field("next", &self.next)
            .field("retired", &self.retired)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn t0() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000)
    }

    fn days(n: u64) -> Duration {
        Duration::from_secs(n * 24 * 60 * 60)
    }

    /// 两张证书**重叠**：`next` 比 `current` 的过期时刻提前 [`ROTATION_OVERLAP`] 生效。
    ///
    /// 首尾相接是不行的 —— 切换有最长一个检查周期的滞后，相接的话那段滞后里
    /// 只有一张已过期的证书可用。
    #[test]
    fn bootstrap_generates_two_overlapping_certificates() {
        let r = Rotation::bootstrap(t0()).unwrap();

        assert_eq!(r.current.not_before(), t0());
        assert_eq!(r.current.not_after(), t0() + MAX_VALIDITY);
        assert_eq!(
            r.next.not_before(),
            r.current.not_after() - ROTATION_OVERLAP,
            "next 必须提前生效"
        );
        assert_ne!(r.current.certhash(), r.next.certhash());

        // 重叠区里两张都有效 —— 这正是安全切换点的存在条件。
        let inside = r.current.not_after() - ROTATION_OVERLAP / 2;
        assert!(r.current.is_valid_at(inside));
        assert!(r.next.is_valid_at(inside));
    }

    /// ★ 切换发生在 `current` 过期**之前**，且切过去那张此刻已经生效。
    ///
    /// 这条是 [`ROTATION_OVERLAP`] 存在的全部理由。它红了意味着每 14 天有一段
    /// 「新连接一律 TLS 失败」的窗口，而错误信息只会说「证书不受信」。
    #[test]
    fn rotation_happens_before_expiry_and_lands_on_a_valid_certificate() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        let expiry = r.current.not_after();

        // 切换点之前一点：还不该动。
        assert_eq!(
            r.advance(expiry - ROTATION_OVERLAP - Duration::from_secs(1))
                .unwrap(),
            Advance::Idle
        );

        // 刚过切换点（**仍在 current 的有效期内**）：应当轮换。
        let at = expiry - ROTATION_OVERLAP + Duration::from_secs(1);
        assert!(matches!(r.advance(at).unwrap(), Advance::Rotated { .. }));

        assert!(
            r.current.is_valid_at(at),
            "切过去的证书在切换那一刻必须已经生效，否则客户端会判 NotValidYet"
        );
        assert!(at < expiry, "切换必须发生在旧证书过期之前");
    }

    /// 有效期内推进不得有任何动作 —— 包括不得重新生成证书（那会静默换掉 certhash）。
    #[test]
    fn advance_within_validity_is_idle() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        let before = r.advertised();

        assert_eq!(r.advance(t0() + days(13)).unwrap(), Advance::Idle);
        assert_eq!(r.advertised(), before);
    }

    /// 跨过期：`next` 提升为 `current`，生成新 `next`，并报出退役的 hash。
    #[test]
    fn advance_past_expiry_rotates() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        let [old_current, old_next] = <[Certhash; 2]>::try_from(r.advertised()).unwrap();

        let advanced = r.advance(t0() + days(15)).unwrap();

        assert_eq!(
            advanced,
            Advance::Rotated {
                retired: old_current
            }
        );
        let [current, next] = <[Certhash; 2]>::try_from(r.advertised()).unwrap();
        assert_eq!(current, old_next, "next 必须原样提升，不能重新生成");
        assert_ne!(next, old_next);
        assert_ne!(next, old_current);
    }

    /// 幂等：同一个越期时刻连续推进两次，第二次必须什么都不做。
    /// 少了这条，每次 poll 都会轮换一张新证书。
    #[test]
    fn advance_is_idempotent_at_the_same_instant() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        let now = t0() + days(15);

        assert!(matches!(r.advance(now).unwrap(), Advance::Rotated { .. }));
        let after_first = r.advertised();

        assert_eq!(r.advance(now).unwrap(), Advance::Idle);
        assert_eq!(r.advertised(), after_first);
    }

    /// 两张都过期（关机 28 天以上）→ 整体重建，而不是链式推进出一堆没人见过的中间证书。
    #[test]
    fn advance_rebuilds_when_both_expired() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        let stale = r.advertised();
        let now = t0() + days(60);

        assert!(matches!(r.advance(now).unwrap(), Advance::Rotated { .. }));

        let fresh = r.advertised();
        assert!(fresh.iter().all(|h| !stale.contains(h)));
        assert!(r.current.is_valid_at(now), "重建后 current 必须在 now 有效");
    }

    /// 系统时钟倒退到证书生效之前，同样走重建 —— 判据是「current 在 now 有效吗」，
    /// 不是「过期了吗」，所以这条不需要单独的分支。
    #[test]
    fn advance_rebuilds_when_clock_moves_backwards() {
        let mut r = Rotation::bootstrap(t0()).unwrap();

        let earlier = t0() - days(1);
        assert!(matches!(
            r.advance(earlier).unwrap(),
            Advance::Rotated { .. }
        ));
        assert!(r.current.is_valid_at(earlier));
    }

    /// ★ 通告地址的实际寿命是 **28 天**（两个轮换周期），不是 14。
    ///
    /// 这条推论直接决定上层「多久刷新一次 bootstrap 清单」，改坏它不会有任何编译错误，
    /// 只会在一个月后表现为「浏览器忽然连不上了」。
    #[test]
    fn advertised_addr_survives_one_rotation() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        // 客户端在第 0 天记下的地址所携带的集合。
        let recorded: HashSet<Certhash> = r.advertised().into_iter().collect();

        // 第 15 天：第一次轮换后，服务端实际使用的证书仍在客户端的接受集合内。
        r.advance(t0() + days(15)).unwrap();
        assert!(
            recorded.contains(&r.current.certhash()),
            "旧地址必须能撑过第一次轮换"
        );

        // 第 29 天：第二次轮换后就不在了。
        r.advance(t0() + days(29)).unwrap();
        assert!(
            !recorded.contains(&r.current.certhash()),
            "第二次轮换后旧地址应当失效 —— 若这条也通过，说明寿命判断本身错了"
        );
    }

    /// Noise 扩展要比通告集合更宽：客户端可能持上一轮的地址，
    /// 它期望的集合里含已退役的 hash，服务端不带上就会把它拒之门外。
    #[test]
    fn noise_certhashes_include_recently_retired() {
        let mut r = Rotation::bootstrap(t0()).unwrap();
        let retired = r.advertised()[0];

        r.advance(t0() + days(15)).unwrap();

        let reported = r.noise_certhashes();
        assert!(
            reported.contains(&retired.to_multihash()),
            "退役 hash 必须仍在上报集合内"
        );
        for hash in r.advertised() {
            assert!(reported.contains(&hash.to_multihash()));
        }
    }

    /// 退役队列有上限，不能无界增长 —— 它每次都进 Noise 握手的载荷。
    #[test]
    fn retired_certhashes_are_bounded() {
        let mut r = Rotation::bootstrap(t0()).unwrap();

        for round in 1..=5 {
            r.advance(t0() + days(15 * round)).unwrap();
        }

        assert_eq!(r.retired.len(), RETIRED_KEEP);
        assert_eq!(r.noise_certhashes().len(), 2 + RETIRED_KEEP);
    }

    /// PEM 往返必须保持两张证书与它们的角色 —— 这正是持久化存在的理由：
    /// 重启后 certhash 不变，对端记下的地址继续可用。
    #[test]
    fn pem_roundtrip_preserves_both_certificates_and_roles() {
        let r = Rotation::bootstrap(t0()).unwrap();
        let (restored, complete) = Rotation::from_pem(&r.to_pem()).unwrap();
        assert!(complete, "完整的两张证书必须报告为完整还原");

        assert_eq!(restored.advertised(), r.advertised());
        assert_eq!(restored.current.not_after(), r.current.not_after());
    }

    /// 角色由 `not_before` 判定，不由写入顺序 —— 外部工具重排过 PEM 段也能恢复。
    #[test]
    fn from_pem_recovers_roles_from_validity_not_order() {
        let r = Rotation::bootstrap(t0()).unwrap();
        let expected = r.advertised();

        // 手工把两段调换顺序（证书段与私钥段一起换，模拟"另一种写入顺序"）。
        let swapped = format!("{}{}", r.next.to_pem(), r.current.to_pem());

        assert_eq!(
            Rotation::from_pem(&swapped).unwrap().0.advertised(),
            expected
        );
    }

    /// 只存了一张也要能恢复：把它当 current、现生成 next。
    /// 整体重建会白白丢掉一张仍然有效、且对端已记下的证书。
    #[test]
    fn from_pem_accepts_a_single_certificate() {
        let r = Rotation::bootstrap(t0()).unwrap();
        let only = r.current.certhash();

        let (restored, complete) = Rotation::from_pem(&r.current.to_pem()).unwrap();
        assert!(
            !complete,
            "只有一张时 next 是现生成的 —— 必须报告为不完整，否则调用方不会回写，\
             那份不完整的数据会自我延续"
        );

        assert_eq!(restored.advertised()[0], only);
        assert_eq!(
            restored.next.not_before(),
            restored.current.not_after() - ROTATION_OVERLAP,
            "补生成的 next 同样要提前生效"
        );
    }

    /// 证书与私钥数量对不上必须报错，而不是配对到一半。
    #[test]
    fn from_pem_rejects_unpaired_sections() {
        let r = Rotation::bootstrap(t0()).unwrap();
        // 两张证书 + 一个私钥
        let broken = format!(
            "{}{}",
            r.current.to_pem(),
            r.next
                .to_pem()
                .lines()
                .take_while(|l| !l.contains("PRIVATE"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        assert!(matches!(
            Rotation::from_pem(&broken),
            Err(Error::Unpaired { .. })
        ));
    }

    #[test]
    fn from_pem_rejects_garbage() {
        assert!(Rotation::from_pem("").is_err());
        assert!(Rotation::from_pem("not a pem at all").is_err());
    }
}
