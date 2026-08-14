//! Host 能力抽象（端口层）。
//!
//! Core / transfer 只依赖这些 trait，桌面端、React Native 和测试环境分别提供实现。
//! 事件聚合（`CoreEvent` / `EventBus`）与测试用 `MemoryHost` 留在 `swarmdrop-core`
//! ——它们引用 network / transfer 域的 DTO，下沉到本 crate 会成环。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::device::{DeviceName, PairedDeviceInfo};
use crate::error::AppResult;

/// 设备身份密钥材料。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentityBytes {
    pub keypair: Vec<u8>,
}

impl std::fmt::Debug for DeviceIdentityBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 绝不打印密钥材料
        f.debug_struct("DeviceIdentityBytes")
            .field("keypair", &"<redacted>")
            .finish()
    }
}

/// 宿主提供的设备身份存储。
///
/// **只管密钥材料**（设备 Ed25519 身份 + WebRTC Direct 证书）。已配对设备列表不在这里，
/// 见 [`PairedDeviceStore`]。
///
/// **名字描述的是角色，不是实现——别照它推断桌面走系统钥匙串。** 移动端确实是系统
/// keychain（iOS Keychain / Android EncryptedSharedPreferences）；桌面端自 2026-08-11
/// 起是 `app_local_data_dir` 下权限 0600 的明文文件（`src-tauri/src/host/identity_store.rs`，
/// 根因与安全边界见那里的模块文档）。
///
/// **改名的触发条件：当没有任何一端还是 keychain 时。** 现在改是把一个对移动端准确的名字
/// 换成一个更笼统的，而代价是 uniffi 那侧 `ForeignKeychainProvider` 的跨 FFI 契约与 4 个
/// 入库的生成文件（cpp / ts）——不值。
#[async_trait]
pub trait KeychainProvider: Send + Sync {
    async fn load_identity(&self) -> AppResult<Option<DeviceIdentityBytes>>;
    async fn save_identity(&self, identity: DeviceIdentityBytes) -> AppResult<()>;
    async fn delete_identity(&self) -> AppResult<()>;

    /// WebRTC Direct 证书（完整 PEM，含私钥）。
    ///
    /// 它与设备 Ed25519 身份分开保存：前者固定分享地址中的 certhash，后者才是
    /// Noise 握手使用的长期身份。
    async fn load_webrtc_certificate_pem(&self) -> AppResult<Option<String>>;
    async fn save_webrtc_certificate_pem(&self, pem: String) -> AppResult<()>;
    async fn delete_webrtc_certificate_pem(&self) -> AppResult<()>;
}

/// 已配对设备列表的持久化端口（整份快照读写）。
///
/// **为什么它不属于 [`KeychainProvider`]。** 两者存的东西性质相反：密钥材料是不出进程的
/// 秘密（宿主实现只负责把它交给平台的安全存储或受限文件，任何人能读到都算泄露），而已配对设备列表是
/// 可导出、会被整份覆写、将来还可能供用户备份的**业务数据**。合成一个 trait 的代价由
/// 没有密钥存储的那一端付：浏览器为了存一份设备列表得实现六个永远不该被调用的密钥方法，
/// 而 `load_identity()` 返回 `Ok(None)` 这种「实现了但不能用」的方法是最容易被误用的
/// 形态——调用方编译通过、运行期静默无效。拆开之后 Web 端只实现这两个方法，也就没有
/// 理由再在自己那侧长一套平行实现。
///
/// **端口刻意只有 load / save 两个方法。** upsert（保留既有信任策略）、改策略、移除
/// 这些都是**业务规则**而非存储能力，统一实现在 `swarmdrop_core::paired_devices`，
/// 对 `&dyn PairedDeviceStore` 操作。端口实现**不得自带业务判断**——规则一旦下放到端口，
/// 三端就会各写一遍，而那正是「Web 的 upsert 整条替换、把用户设过的信任级别与收件策略
/// 静默重置」这个 bug 的成因。
///
/// 代价明说：整份快照覆写存在 read-modify-write 竞态。现状即如此（三端全是整份覆写），
/// 且调用点都在用户操作路径上（串行），故不加固；将来若真出现并发写，正确的修法是给
/// core 的写操作加一把锁，而不是把算法推回端口。
#[async_trait]
pub trait PairedDeviceStore: Send + Sync {
    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>>;

    /// 整份覆写。**收借用而非所有权**：三端实现拿到它只做一次
    /// `serde_json::to_string(&devices)` 或 `.to_vec()`，而调用方（core 的
    /// `paired_devices` 各写操作）既要交给端口、又要把同一份列表返给上层，
    /// 收所有权就逼得每次写都整份 `clone()` 一遍。
    async fn save_paired_devices(&self, devices: &[PairedDeviceInfo]) -> AppResult<()>;
}

/// 用户设备名的持久化端口（桌面 / 移动 = `device_config.json`，Web = IndexedDB）。
///
/// 端口只认已归一化的 [`DeviceName`]——未归一化的 `String` 在类型层面就传不进来，
/// 而归一化的唯一入口是 [`DeviceName::parse`]。
///
/// 读取动作由 core 的组合根（`start_node`）承担，不由各 host 自己读完再把值塞进
/// `OsInfo`——那正是这个端口要取代的旧形态：「三端各写各的」原封不动，本机 `OsInfo`
/// 依然没有唯一装配点。
///
/// **load 不返回错误、save 返回错误，这个不对称是刻意的：**
/// - [`load_device_name`](Self::load_device_name) 在节点启动路径上。一个被手改坏的
///   JSON、一次 IndexedDB 打不开，若能让 `start_node` 返回 `Err`，代价是「节点起不来」；
///   而正确行为显然是「用 hostname 兜底继续跑」。把降级写进 trait 契约，比指望每个调用点
///   自己写 `.unwrap_or_default()` 更难被后来者写反。
/// - [`save_device_name`](Self::save_device_name) 只在用户点保存时发生。静默失败等于
///   「改了名字、重启又变回去」且没有任何信号，必须冒泡到 UI。
#[async_trait]
pub trait DeviceConfig: Send + Sync {
    /// 读取持久化的设备名。**不返回错误**：无值 / 读失败 / 内容非法一律降级为 `None`
    /// （调用方回退到 [`OsInfo::hostname`](crate::device::OsInfo::hostname)）。
    async fn load_device_name(&self) -> Option<DeviceName>;

    /// 写入设备名；`None` 表示清空，回退到 hostname。
    async fn save_device_name(&self, name: Option<DeviceName>) -> AppResult<()>;
}

/// 文件 source 标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileSourceId(pub String);

/// 文件 sink 标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct FileSinkId(pub String);

/// `finalize_sink` 的返回：文件最终落盘位置 + 其父目录 —— 都是 host 侧唯一诚实的
/// 事实源（保存目录 + 相对路径拼接推导不出:SAF document URI 有独立编码,重名冲突
/// 还会被改写成 "foo (1).txt"）。`dir` 供「打开文件夹」定位真实容器目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedSink {
    /// 文件最终 URI（桌面绝对路径 / 移动 file:// 或 SAF document URI）。
    pub uri: String,
    /// 文件父目录 URI（桌面父目录绝对路径 / 移动 file:// 目录或 SAF 目录 document URI）。
    pub dir: String,
}

/// 接收端保存位置（host-agnostic）。
///
/// core 内部统一用此类型，避免把 `entity::SaveLocation`（SeaORM 实体细节）
/// 暴露到公共 API 上。DB 边界用 [`From`] 与 `entity::SaveLocation` 双向转换。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum CoreSaveLocation {
    /// 宿主自己解释的保存位置串，**core 视其为不透明**。
    ///
    /// 桌面是文件系统绝对路径；移动端是 expo-file-system 的 URI，**可能是 `file://`，
    /// 也可能是 Android SAF 的 `content://` tree**（用户在设置里选了系统公共目录时）；
    /// Web 是 OPFS 的相对路径。名字叫 `Path` 是历史，别据此假设它一定是文件系统路径——
    /// 移动端的发布路径正是靠嗅探 `content://` 前缀来分派的。
    Path { path: String },
}

impl From<CoreSaveLocation> for entity::SaveLocation {
    fn from(v: CoreSaveLocation) -> Self {
        match v {
            CoreSaveLocation::Path { path } => entity::SaveLocation::Path { path },
        }
    }
}

impl From<entity::SaveLocation> for CoreSaveLocation {
    fn from(v: entity::SaveLocation) -> Self {
        match v {
            entity::SaveLocation::Path { path } => CoreSaveLocation::Path { path },
        }
    }
}

/// 文件元信息。
///
/// `save_dir` 由 core 在 `accept_and_start_receive` 时填入用户选择的保存位置，
/// host adapter 据此决定真实写入路径——避免 host 端自己保存"当前会话目录"。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct HostFileMetadata {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_at: Option<i64>,
    pub checksum: Option<String>,
    /// 接收端保存位置；source_metadata（发送端）固定为 None。
    #[serde(default)]
    pub save_dir: Option<CoreSaveLocation>,
}

/// 宿主文件访问能力。
///
/// # 接收侧是「暂存 → 发布」两个阶段
///
/// `create_sink`/`open_or_create_sink` 开的是**暂存**，不是最终文件；
/// `finalize_sink` 才把它发布到用户选定的位置并回报它最终在哪。
/// 实现方必须满足下面四条——它们不是建议，core 的恢复逻辑直接依赖：
///
/// 1. **暂存位置的文件描述符必须由本进程完全拥有。** 接收期间的随机写
///    （定位偏移 + 写）只能施加于这样的 fd。由外部文档提供方授予的描述符
///    （Android SAF 经 `ContentResolver` 拿到的那种）**不满足**：它可能在本进程
///    无从感知的情况下失效，使已打开的文件通道在自身仍报告为「打开」的状态下、
///    于下一次定位操作失败。外部位置只能在发布阶段被**顺序**写一次。
/// 2. **暂存位置必须是 [`HostFileMetadata`] 的确定性函数**（`save_dir` + `relative_path`）。
///    续传只拿得到元信息——里面**没有会话标识**——却必须重新接上同一份暂存。
/// 3. **`finalize_sink` 只发布、不校验。** 完整性由 bao 逐块验签在落盘前保证
///    （`root == 整文件 blake3` 是它的不变量），再读一遍整个文件是纯冗余。
/// 4. **发布失败意味着「数据完好、只是搬不过去」**（空间不足 / 权限被撤 / fd 失效），
///    不意味着数据损坏。实现方 SHALL 保留暂存，使调用方能原地重试而不必让对端
///    重传任何字节；core 据此**不会**重置该文件的分块进度。
///
/// 完整推导见 `openspec/changes/receive-staging-publish/`。
///
/// **Web 实现目前不满足第 1、4 条**（`crates/web/src/file_access.rs`）：它直接把
/// OPFS 的最终路径当暂存写，`finalize_sink` 只是 `close()`。OPFS 的句柄归本进程所有，
/// 所以第 1 条的**风险**在那里不存在；但「发布失败后可原地重试」在那边是未定义的。
/// 这是已知缺口，不是可以照抄的先例。
#[async_trait]
pub trait FileAccess: Send + Sync {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata>;
    /// 精确读取源文件 `[offset, offset+length)` 区间的字节。
    ///
    /// **严格契约**（宿主实现必须逐条满足，违约会破坏 bao 逐块验签——
    /// 2026-07 桌面宿主把 offset 取整到 256KiB chunk，>16KiB 文件 prepare 直接
    /// panic 进 blake3）：
    /// - 返回字节数 == `min(length, 文件大小 - offset)`：不取整、不多读、不少读；
    /// - `offset` 越过 EOF → 返回空 `Vec`（不报错）；尾部不足 `length` → 截断到 EOF；
    /// - 禁止返回超过 `length` 的数据（内核视为违约、响错拒收）。
    ///
    /// 调用方包括 bao outboard 构建（顺序读、粒度 ≤ 一个 chunk group）与 sender 的
    /// 逐块推送。**不要假设 offset/length 与任何 chunk 尺寸对齐**——2026-08 之前那两条
    /// 路的粒度差 16 倍（16KiB vs 256KiB），现在恰好相同，但对齐从来不是契约的一部分。
    /// 参考实现（含契约单测）：桌面 `src-tauri/src/host/file_source/path_ops.rs::read_at_sync`。
    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>>;

    /// 开一条**新**暂存：同名残留一律清空。用于首次传输。
    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId>;

    /// 接上一条**已有**暂存，没有才新建；已有内容必须**保留**。用于续传。
    ///
    /// **没有默认实现是刻意的**——曾经的默认是转调 `create_sink`，那会在续传时把暂存
    /// 截断，随后只补拉缺失的块，产出一个长度正确、内容有洞的文件。而 core 这一侧再也
    /// 拦不住它：发布不做整文件校验了（第 3 条），完整性判定只看分块位图。
    /// 漏实现必须在编译期红，同 [`delete_finalized_file`](Self::delete_finalized_file)。
    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId>;

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()>;

    /// **发布**这条暂存到用户选定的位置，返回文件的**最终落盘位置及其父目录**
    /// （桌面端为 `.part` 重命名后的绝对路径 + 其 dirname，移动端为 expo-file-system 的
    /// `file://` / SAF document URI + 对应目录 URI）。
    ///
    /// **不做完整性校验**（trait 文档第 3 条）。**失败表示搬不过去、而非数据损坏**
    /// （第 4 条）：实现方 SHALL 保留暂存，并尽力删除目标位置上的不完整产物。
    ///
    /// 返回值是 host 对「文件实际在哪」唯一诚实的事实源——保存目录 + 相对路径的
    /// 字符串拼接推导不出它（SAF URI 有独立的 document 段编码，重名冲突还会被
    /// host 改写成 "foo (1).txt"），core 必须原样落库供收件箱 / 「打开文件夹」消费。
    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink>;

    /// 丢弃一条**未最终化**的 sink，并**真正删掉已经落盘的那部分产物**。
    ///
    /// 「删掉部分产物」是契约的一部分，不是可选优化——此前这句话只活在各端实现的注释里，
    /// 默认实现又是 no-op，于是「要不要真删」全靠各端自己揣摩。继承 no-op 的表现是
    /// 「功能看起来正常，只是盘上慢慢堆残件」，没有任何测试会红。
    ///
    /// Web 比桌面更需要它：那边**没有 `.part` 中间态**，写的就是最终路径，残件是个
    /// 文件名正确、内容截断的东西；桌面留下的至少还叫 `xxx.part`，一眼能看出没写完。
    ///
    /// 保留默认实现是为了不逼所有实现方同时改动，但**新实现一律要覆盖它**。
    async fn cleanup_sink(&self, _sink: &FileSinkId) -> AppResult<()> {
        Ok(())
    }

    /// 删除一个**已最终化**的文件。参数是 [`finalize_sink`](Self::finalize_sink) 返回过的
    /// `uri`，也就是落库到 `local_path` 的那个值。
    ///
    /// 与 [`cleanup_sink`](Self::cleanup_sink) 的分工按**生命周期阶段**划分，不是重复：
    /// 那个丢弃的是取消/失败留下的半成品（输入是 sink id，文件还没进任何账本），
    /// 这个删的是已经落盘、已经进了收件箱、用户在界面上看得见的东西。
    ///
    /// **文件已不存在不算错误**，返回 `Ok`——删除天然幂等，而「删两次」在重试路径上很常见。
    ///
    /// 各端对 `uri` 的解释不同，这正是它该由实现方处理的理由：桌面是文件系统绝对路径、
    /// 移动是 `file://` 或 SAF document URI、Web 是 `opfs:/` 前缀的 OPFS 键。上层编排
    /// 只管「把 local_path 递过来」，不需要知道哪一端用哪个字段。
    ///
    /// **没有默认实现是刻意的**：漏实现要在编译期红，而不是变成一条静默泄漏。
    async fn delete_finalized_file(&self, uri: &str) -> AppResult<()>;
}

/// 语义通知：core 只表达「发生了什么」，不含任何语言的标题 / 正文散文。由 host 在展示
/// 时按当前 locale 翻译（桌面端走 rust-i18n；移动端目前传 `None` 不弹通知，未来可自行
/// 本地化）。与错误 `kind` 同构——core 保持语言中立，翻译发生在呈现边缘。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    /// 收到配对请求。`hostname` = 请求方设备名。
    PairingRequest { hostname: String },
    /// 收到文件传输请求（需用户确认）。`device_name` = 发送方设备名。
    IncomingTransfer { device_name: String },
    /// 收到文本投递。正文不得进入该语义通知，防止锁屏或系统历史泄露。
    IncomingText { device_name: String },
}

/// 宿主通知能力。
///
/// 入参是语义 [`Notification`]，host 侧负责翻译成当前语言的标题 / 正文再展示。
/// `notify_if_unfocused` 用于桌面端：仅当窗口未聚焦时才推送通知。
/// 默认实现 fallback 到 `notify`，移动端无窗口聚焦概念时无需 override。
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, notification: Notification) -> AppResult<()>;

    async fn notify_if_unfocused(&self, notification: Notification) -> AppResult<()> {
        self.notify(notification).await
    }
}

/// 更新安装请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallRequest {
    pub url: String,
    pub is_force: bool,
}

/// 宿主更新/安装能力。
#[async_trait]
pub trait UpdateInstaller: Send + Sync {
    async fn install_update(&self, request: UpdateInstallRequest) -> AppResult<()>;
}
