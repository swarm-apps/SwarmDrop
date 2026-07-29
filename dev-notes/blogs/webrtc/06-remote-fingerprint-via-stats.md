# 0.20 把远端证书弄丢了：webrtc#828 与一次 API 边界之争

> 本系列第 6 篇。前置：[01 libp2p 的两种 WebRTC](01-libp2p-webrtc-direct.md) 的
> 「为什么必须再跑一次 Noise」。
>
> 上游 PR：[webrtc-rs/webrtc#828](https://github.com/webrtc-rs/webrtc/pull/828)（评审中）

## 需求：服务端必须拿到客户端的真实指纹

回顾 [第 01 篇](01-libp2p-webrtc-direct.md)：direct 模式建连的最后一步是 Noise 握手，
它的 prologue 绑定**双方**的 DTLS 指纹：

```text
libp2p-webrtc-noise:<客户端指纹><服务端指纹>
```

两端必须算出**逐字节相同**的 prologue。各自的指纹自己知道；客户端从 multiaddr 的 certhash
知道服务端的。剩下**唯一**没有带外来源的值：**服务端需要客户端的指纹**。

它只存在于一个地方——DTLS 握手时客户端出示的那张证书。

0.17 有一行 API 直接给你：

```rust
// webrtc 0.17
let cert_bytes = conn.sctp().transport().get_remote_certificate().await;
let fingerprint = Fingerprint::from_certificate(&cert_bytes);
```

0.20 重构成 sans-io 之后，**这个 API 没了**。公开 API 里连 `dtls_transport` 模块都不存在，
`PeerConnection` 上也没有任何东西能拿到对端证书。

## 唯一剩下的路：统计报告，外加两道坎

数据其实还在——它跑到了 `get_stats` 返回的统计报告里。但取它有两个障碍。

### 坎一：报告不告诉你哪张证书是谁的

报告里每一侧各有一条 `Certificate` 条目，**条目本身不说自己属于哪边**。区分它们的唯一
线索，在另一种条目上：

```mermaid
flowchart LR
    T["Transport 条目<br/>local_certificate_id: 'A'<br/>remote_certificate_id: 'B'"]
    C1["Certificate 条目<br/>id: 'A'<br/>fingerprint: aa:bb:…"]
    C2["Certificate 条目<br/>id: 'B'<br/>fingerprint: cc:dd:…"]
    T -->|local| C1
    T -->|remote| C2
    style C2 fill:#e6f4ea,stroke:#34a853
```

所以是个两步查找：先找 `Transport` 条目读出 `remote_certificate_id`，再拿这个 id 回去
匹配 `Certificate` 条目。

**这个查找写反了不会报错。** 把 `remote_certificate_id` 写成 `local_certificate_id`，
代码照常跑，只是 prologue 会拿本端指纹去和本端比——Noise 随后失败，而错因完全看不出来。
更糟的场景：如果有人拿这个函数做证书固定（certificate pinning），**它会把每一个对端都
判定为合法**。

### 坎二：`get_stats` 的参数类型叫不出名字

第二道坎更基础。`get_stats` 是 `PeerConnection` trait 上的公开方法：

```rust
async fn get_stats(&self, now: Instant, selector: StatsSelector) -> RTCStatsReport;
```

而 `StatsSelector` 和 `RTCStatsReport` 在 `webrtc::peer_connection` 里是 `use` 进来的，
**不是 `pub use`**。

于是出现一个尴尬局面：**一个公开的 trait 方法，它的参数类型和返回类型在 crate 外面
没法命名。** 调用方只能自己去依赖底层的 `rtc` crate 才能把这些类型拼出来——而那意味着
在自己的 `Cargo.toml` 里重复声明一个 `webrtc` 已经 pin 好的版本，并手工保持同步
（`rtc` 还在 release candidate 阶段，版本号一直在动）。

## 第一版 PR：加一个便捷方法

于是 PR 提了两样东西：

1. `PeerConnection::remote_certificate_fingerprint(now)` —— 一个默认方法，把两步查找封起来
2. `pub use` 那三个统计类型，让 `get_stats` 本身可用

## 评审：一个我没能反驳的论点

维护者 rainliu 给了 `CHANGES_REQUESTED`：

> `remote_certificate_fingerprint` is not defined in W3C API. we need to make
> PeerConnection's API more compact and compliance to W3C APIs.
> If application needs it, just use get_stats get it by itself, like this function does.

他是对的。`RTCPeerConnection` 在 W3C 规范里确实没有这个方法。一个便捷函数不值得把
公开 trait 撑大——尤其这个 trait 是外部实现者要对齐的契约，加一个默认方法看似无害，
实则是永久的 API 承诺。

## 关键动作：把一个笼统的意见拆成两个问题

`CHANGES_REQUESTED` 是对整个 PR 的。但 PR 里其实有**两样可分离的东西**，它们面对的
是不同的问题：

| | 是什么问题 | 我的回应 |
|---|---|---|
| `remote_certificate_fingerprint` | **API 面**：该不该给 trait 加非 W3C 方法 | **接受，砍掉** |
| 三行 `pub use` | **可用性**：公开签名里的类型能不能命名 | **保留，单独举证** |

W3C 合规这个论点管得住第一样，管不住第二样——re-export 一个**已经出现在本 crate 公开
签名里**的类型，并没有扩大 API 面，它只是让已有的 API 变得可调用。

举证时最有力的一条，是在查证过程中才发现的：

```rust
// src/peer_connection/mod.rs —— 我改的那两行下面几行
pub use rtc::interceptor::{Interceptor, NoopInterceptor, Registry};
pub use rtc::peer_connection::{ ... };
```

**同一个文件里，紧挨着的位置，已经在这么干了。** 还有 `data_channel`、`rtp_transceiver`、
`media_stream` 几个模块，全都 `pub use` 了 rtc 的类型。

这一下论证的性质就变了：不是「请你接受我的一个新主张」，而是「这个 crate 已有的惯例，
在统计类型这里漏了」。**维护者不需要认同我的品味，只需要认同他自己的惯例。**

回复的结尾留了台阶：如果他仍然坚持，我把三行也删掉，只留文档和测试。这不是客套——
**上游的 API 边界是维护者的决定权，我的工作是把信息摆全，不是把结论摆上去。**

## 一个自我纠错

写 commit message 时我第一版这么论证 re-export 的必要性：

> pre-release 版本互不 semver 兼容，版本不匹配会静默解析出第二份类型。

**这是错的。** Cargo 对 `^0.20.0-rc.4` 的解析范围是 `>=0.20.0-rc.4, <0.21.0`——
rc.5 是满足的，会被统一成同一份。不存在「静默分叉」。

发现之后改成了准确的说法（下游得重复声明一个本 crate 已 pin 的版本并手工同步），
**错误的陈述没有进到上游 PR 里**。

这件事值得单独记一笔：给上游提 PR 时，**论证里的每一个技术断言都会被当成事实**。
一个听起来很专业但其实站不住的理由，比没有理由更糟——它会消耗维护者对你其余论点的信任。
拿不准的机制，宁可用更弱但确定为真的表述。

## 测试怎么设计：让「写反了」无处藏身

前面说过，这个查找写反了不会报错。所以测试必须**专门针对这个失败模式**。

方法是**双向交叉验证**：

```text
offerer 看到的 remote  ==  answerer 报告的 local
answerer 看到的 remote ==  offerer 报告的 local
```

再加一条守卫：

```rust
// 两端的证书必须不同，否则上面两条断言可以靠巧合通过
assert_ne!(offerer_own, answerer_own, "...");
```

如果实现把 `remote` 写成了 `local`，两条交叉断言会同时失败，并把两个指纹并排打出来。
换成一句「返回值非空」的断言，这个 bug 会毫发无伤地通过。

> 同样的思路在 [第 02 篇](02-dtls-fingerprint-dead-switch.md) 出现过：那里用的是
> 正反双向的测试对。**共同点是——先想清楚「这段代码最可能怎么错」，再设计一个专门
> 能捕获它的判据。**

## 结果：反查逻辑归本仓

方法砍掉后，那段两步查找搬回了 SwarmDrop：

```rust
// crates/webrtc-p2p/src/backend/native/direct/upgrade.rs
async fn remote_fingerprint(pc: &dyn PeerConnection) -> Result<Fingerprint, Error> {
    let report = pc.get_stats(Instant::now(), StatsSelector::None).await;

    let remote_id = report.iter().find_map(|entry| match entry {
        RTCStatsReportEntry::Transport(t) if !t.remote_certificate_id.is_empty() =>
            Some(t.remote_certificate_id.clone()),
        _ => None,
    }).ok_or_else(|| Error::Connection("DTLS 握手后统计报告里没有对端证书".into()))?;

    let value = report.iter().find_map(|entry| match entry {
        RTCStatsReportEntry::Certificate(cert) if cert.stats.id == remote_id =>
            Some(cert.fingerprint.clone()),
        _ => None,
    }).ok_or_else(|| Error::Connection("DTLS 握手后仍拿不到对端证书指纹".into()))?;

    crate::protocol::addr::parse_sdp_fingerprint(&value)
        .ok_or_else(|| Error::Connection(format!("对端证书指纹格式无法解析：{value}")))
}
```

十几行，注释里写清了「取错侧会静默失败」以及「上游为什么不收这段」。而 PR 保留的
`pub use` 让这十几行**能被写出来**——这才是它真正的价值。

## 教训

**1. 公开方法的签名里出现的类型，必须可命名。**
否则那个方法在 crate 外面等于不存在。这是 API 设计的基本卫生，与「要不要加便捷方法」
是两个层次的问题。

**2. 笼统的评审意见，先拆再答。**
一个 `CHANGES_REQUESTED` 可能同时覆盖几个可分离的主张。**接受该接受的，为该保留的
单独举证**，比整包接受或整包争辩都好——也让维护者更容易做决定。

**3. 最强的论据是对方自己的惯例。**
「这个 crate 已经在别处这么做了」比任何设计论证都省事。提 PR 前花五分钟 grep 一下
同类模式，往往能把一场辩论变成一次确认。

**4. 论证里的技术断言会被当成事实。**
拿不准的机制（semver 解析规则、编译器行为、协议细节），宁可用更弱但确定为真的表述。
**一个站不住的理由会连累你其余的论点。**

---

**上一篇**：[这条通道是谁开的](05-who-opened-this-channel.md) ·
**下一篇**：[怎么把踩的坑变成上游补丁](07-upstream-methodology.md)

**上游**：[webrtc#828](https://github.com/webrtc-rs/webrtc/pull/828)（评审中）、
[issue #827](https://github.com/webrtc-rs/webrtc/issues/827) ·
**本仓**：`crates/webrtc-p2p/src/backend/native/direct/upgrade.rs`
