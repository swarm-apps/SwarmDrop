//! 文件访问 bridge —— 把 core 的 [`FileAccess`] 拆成「本进程自己做」与
//! 「只能委托 RN 做」两部分。
//!
//! ## 数据流
//!
//! ```text
//! core::TransferManager
//!   ├─ read_source_chunk(source, offset, length) ──► 按 source scheme + 归属分派
//!   │      file:// 且落在应用沙箱容器内 ──► 本进程 POSIX 读（spawn_blocking）
//!   │      其余（content:// / 容器外的 file://） ──► RN expo-fs.read()
//!   │
//!   ├─ create_sink / write_sink_chunk / cleanup_sink ──► StagingArea（纯 Rust POSIX IO）
//!   │      接收暂存恒在应用私有目录，**一次都不跨语言边界**
//!   │
//!   └─ finalize_sink ──► 按目标 scheme 分派
//!          file://    ──► publish_to_local（Rust：建目录 + 逃逸校验 + rename）
//!          content:// ──► RN publish_to_target（只有 ContentResolver 建得了 document）
//! ```
//!
//! **「按 scheme 分派」是这一层的通则，读源只是最后一个补上的。** 在 2026-08-10 之前，
//! `publish` 与 `delete_finalized_file` 都按 `content://` 分派，唯独读源无条件走 JS——
//! 于是每 256 KiB 一次 `invokeBlocking` 硬阻塞单线程 runtime，实测 9.25 ms/块
//! （桌面同操作 0.118 ms），一次 2 GB 发送累计冻结网络事件循环约 39 s。那不只是慢，
//! 是断链的嫌疑诱因。
//!
//! 接收的随机写为什么必须留在本进程，见 [`crate::file_staging`] 的模块文档。
//!
//! ## 关键约束
//!
//! - 所有方法 async；callback 不能在 Rust 持锁时调用（uniffi-bindgen-rn 会死锁）
//! - `MobileFileMetadata` / `MobileFinalizedSink` 用 uniffi Record 实现类型安全

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use swarmdrop_core::host::{
    CoreSaveLocation, FileAccess, FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata,
};
use swarmdrop_core::{AppError, AppResult};
use tracing::{debug, warn};

use crate::error::FfiError;
use crate::file_staging::StagingArea;

/// 接收端保存位置（uniffi 镜像 [`CoreSaveLocation`]）
#[derive(Debug, Clone, uniffi::Enum)]
pub enum MobileSaveLocation {
    /// 文件系统路径（RN 用 expo-file-system 的 uri）
    Path { path: String },
}

impl From<CoreSaveLocation> for MobileSaveLocation {
    fn from(v: CoreSaveLocation) -> Self {
        match v {
            CoreSaveLocation::Path { path } => MobileSaveLocation::Path { path },
        }
    }
}

impl From<MobileSaveLocation> for CoreSaveLocation {
    fn from(v: MobileSaveLocation) -> Self {
        match v {
            MobileSaveLocation::Path { path } => CoreSaveLocation::Path { path },
        }
    }
}

/// 文件元信息（uniffi 镜像 [`HostFileMetadata`]）
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileFileMetadata {
    pub name: String,
    pub relative_path: String,
    pub size: u64,
    pub modified_at: Option<i64>,
    pub checksum: Option<String>,
    /// 接收端保存目录；source_metadata 调用时固定为 None。
    pub save_dir: Option<MobileSaveLocation>,
}

impl From<HostFileMetadata> for MobileFileMetadata {
    fn from(m: HostFileMetadata) -> Self {
        // 穷尽解构作为 drift guard：上游给 HostFileMetadata 加字段时这里会编译失败。
        let HostFileMetadata {
            name,
            relative_path,
            size,
            modified_at,
            checksum,
            save_dir,
        } = m;
        Self {
            name,
            relative_path,
            size,
            modified_at,
            checksum,
            save_dir: save_dir.map(Into::into),
        }
    }
}

impl From<MobileFileMetadata> for HostFileMetadata {
    fn from(m: MobileFileMetadata) -> Self {
        Self {
            name: m.name,
            relative_path: m.relative_path,
            size: m.size,
            modified_at: m.modified_at,
            checksum: m.checksum,
            save_dir: m.save_dir.map(Into::into),
        }
    }
}

/// finalize_sink 的返回（uniffi 镜像 [`FinalizedSink`]）：文件最终 URI + 其父目录 URI。
/// `dir` 供「打开文件夹」定位真实容器目录（file:// 目录 / SAF 目录 document URI）。
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileFinalizedSink {
    pub uri: String,
    pub dir: String,
}

impl From<MobileFinalizedSink> for FinalizedSink {
    fn from(v: MobileFinalizedSink) -> Self {
        Self {
            uri: v.uri,
            dir: v.dir,
        }
    }
}

/// RN 端必须实现的文件 I/O 接口
///
/// `source_id` 和 `sink_id` 都是字符串（host 自定义编码：桌面用 path，
/// RN 用 expo-file-system 的 uri）。core 不解析这些 id，只透传。
#[uniffi::export(with_foreign)]
#[async_trait]
pub trait ForeignFileAccess: Send + Sync {
    /// 读取 source 元信息（文件名/大小）
    async fn source_metadata(&self, source_id: String) -> Result<MobileFileMetadata, FfiError>;

    /// 读取指定 chunk —— core 用于 BLAKE3 hash 计算和 chunk 发送
    // 严格契约（同 crates/host ports.rs 的 FileAccess::read_source_chunk，那里是
    // 权威文档）：必须精确返回 [offset, offset+length)——EOF 截断、越界得空、禁止
    // 超长/取整；bao outboard 构建按顺序、粒度 ≤ 一个 chunk group 调用（2026-08 起
    // chunk group == CHUNK_SIZE，此前是 16KiB —— 但对齐从来不是契约的一部分）。JS 实现在
    // mobile/src/core/foreign-file-access.ts（readBytes 尊重 length，改动时勿破坏）。
    // 注：契约写普通注释而非 ///，是避免动 uniffi docstring 触发绑定重生成。
    async fn read_source_chunk(
        &self,
        source_id: String,
        offset: u64,
        length: u64,
    ) -> Result<Vec<u8>, FfiError>;

    /// 把一个已收齐的暂存文件发布到 SAF (`content://`) 目标目录下。
    ///
    /// **只有 SAF 目标会走到这里。** `file://` 目标由 Rust 侧直接 rename/copy——
    /// 这个 port 存在的唯一理由是 `ContentResolver`：建 document、拿 document URI，
    /// 只有平台侧做得到。接收的随机写则完全不经过 JS（见 `file_staging` 的模块文档）。
    ///
    /// `staging_uri` 是应用私有目录下暂存文件的 **`file://` URI**（percent-encoded）。
    /// **必须带 scheme**：expo 的 `JavaFile` 构造是 `File(URI.create(uri))`，
    /// 无 scheme 的裸路径会抛 `IllegalArgumentException: URI is not absolute`，
    /// 于是每一次 SAF 发布都失败——正是这个改动要修的那个配置。
    ///
    /// 实现方读它、顺序写进目标，**不要**在目标上做定位写。
    ///
    /// 返回文件的最终落盘 URI **及其父目录 URI**——core 会原样落库，收件箱
    /// 「打开/分享/删除」依赖 uri、「打开文件夹」依赖 dir，**不能**用目录 + 相对路径
    /// 拼接代替（SAF document id 有独立编码，重名冲突还会被系统改写成 "foo (1).txt"）。
    ///
    /// 必须**可重入**：目标已存在时覆盖它，而不是生成带序号的副本——进程在拷贝中途
    /// 被杀之后，续传会重新发布一次。失败时应尽力删掉目标位置的半成品。
    async fn publish_to_target(
        &self,
        staging_uri: String,
        metadata: MobileFileMetadata,
    ) -> Result<MobileFinalizedSink, FfiError>;

    /// 删除一个**已最终化**的文件。`uri` 是发布时返回过的那个（`file://` 或 SAF
    /// document URI），也就是落库到 `local_path` 的值。
    ///
    /// **文件已不存在不算错误**，按契约返回 `Ok`（删除幂等）。
    ///
    /// 这条此前不存在：删收件箱文件的编排整段写在 TS 侧（`inbox-store.ts` 的
    /// `deleteLocalFiles`），于是同一段「取 detail → 逐文件删 → 软删记录」三端各一份，
    /// 且这份在 detail 取不到时静默跳过、另两端报错。编排现已收进
    /// `swarmdrop_transfer::inbox::delete_inbox_item`，TS 侧只剩「哪个 URI 怎么删」这一层。
    async fn delete_finalized_file(&self, uri: String) -> Result<(), FfiError>;
}

/// SAF 目标的判别前缀。`content://` 之外的一切都当作本地路径处理。
const SAF_SCHEME: &str = "content://";

/// 本地文件的判别前缀。
///
/// 读源用它做**白名单式**判据，而不是发布侧那样的 `!starts_with(SAF_SCHEME)`：
/// 发送源的 URI 形态比接收目标多（`content://`、`file://`，将来还可能有相册的 `ph://`），
/// 只有明确认得的那一种才允许 Rust 直读，认不出的一律退回 JS。
const LOCAL_SCHEME: &str = "file://";

/// 私有数据区在两个平台上的尾部形态。去掉它，剩下的就是**应用沙箱容器根**。
///
/// 宿主交给 `MobileCore::new` 的 `data_dir` 来自 `getPrivateDataDir()`
/// （`mobile/src/core/paths.ts`）：iOS = `<容器>/Library/Application Support`，
/// Android = `<容器>/files`。
const PRIVATE_DATA_DIR_TAILS: [&str; 2] = ["Library/Application Support", "files"];

/// 「同一个目录的两种写法」的已知别名对，**双向**展开。
///
/// 这不是洁癖：归属判定是纯**词法**的前缀比较（见 [`route_source`]），两侧写法只要
/// 不一致，快路径就整片失效——发送照跑、只是又回到每块一次 `invokeBlocking`，
/// 而日志里看不出任何异常。这是这个性能修复最可能的失效方式，所以别名表必须先于
/// 前缀比较存在。
///
/// - iOS：`/var` 是指向 `/private/var` 的符号链接，而两侧恰好各用一种写法——`data_dir`
///   来自 `FileManager.urls(for:.applicationSupportDirectory)`，给的是 `/var/…`；
///   DocumentPicker / 分享意图落在 `NSTemporaryDirectory()` 下，给的是 `/private/var/…`。
///   没有这张表，iOS 上**一次都不会命中**快路径，而日志里只是「全都走了 JS」。
/// - Android：`/data/data` 是指向 `/data/user/0` 的符号链接。
///
/// 表里没有的形态（例如工作资料下的 `/data/user/10/<pkg>`）不展开，于是那台设备上
/// 快路径不命中——**失败关闭，慢但对**，与认不出容器根时同一个方向。
const ROOT_PREFIX_ALIASES: [(&str, &str); 2] =
    [("/var", "/private/var"), ("/data/user/0", "/data/data")];

/// 分派日志的抽样间隔：每种结论首次出现记一行，之后每这么多次记一行。
/// 判据见 [`MobileFileAccessAdapter::log_source_dispatch`]。
const DISPATCH_LOG_EVERY: usize = 1000;

/// 从私有数据区反推**应用沙箱容器根**——本进程可以直接 `open()` 的那片区域。
///
/// 容器里装着 `cache/`（DocumentPicker / ImagePicker / 分享意图的落点）、Android 的
/// `files/`、iOS 的 `Documents/`（既是收件箱落点，也是「从收件箱转发」的 source）。
/// 一个根就把这些全覆盖了，不必逐个枚举。
///
/// **认不出形态时返回 `None`，于是所有读源退回 JS——与本改动之前的行为逐字节一致。**
/// 失败必须朝这个方向：少认一个根只是慢，多认一个根会让 Rust 去开它开不了的文件
/// （iOS 的安全作用域 URL 直接 EPERM），那是真的坏掉。
///
/// 「剥完还得剩下一个真目录」是这条判据的一部分，不是防御性废话：`data_dir` 若是
/// `files` 或 `/files` 这类退化形态，剥完得到的是 `""` 或 `/`，而
/// `Path::starts_with("")` 对**任何**路径恒真、`starts_with("/")` 对任何绝对路径恒真
/// ——归属判定会整条失效，「认不出就失败关闭」这个前提反过来变成**失败开放**。
/// 生产上 `data_dir` 恒是 expo 给的绝对 URI，够不到这里；但整条推导都架在
/// 「认不出 → 退回 JS」上，唯一的例外必须在源头堵掉。
fn sandbox_container_root(data_dir: &Path) -> Option<PathBuf> {
    PRIVATE_DATA_DIR_TAILS.iter().find_map(|tail| {
        let tail = Path::new(tail);
        // `Path::ends_with` 按整段 component 比较，不会把 `xfiles` 认成 `files`；
        // 命中后沿 `ancestors` 往上退 tail 的段数，剩下的就是容器根。
        if !data_dir.ends_with(tail) {
            return None;
        }
        data_dir
            .ancestors()
            .nth(tail.components().count())
            .map(Path::to_path_buf)
            // 真实的两个根（`/data/user/0/<pkg>`、
            // `/var/mobile/Containers/Data/Application/<UUID>`）都「绝对且有父目录」；
            // 退化出来的 `""` 与 `/` 都不是，见上面那段。
            .filter(|root| root.is_absolute() && root.parent().is_some())
    })
}

/// 把容器根展开成「同一目录的所有已知写法」，供归属判定逐个比对。
///
/// 只从**实际的那个根**出发双向展开，不去猜别的形态：这样别名表覆盖不到的平台布局
/// 只会少一份根（退回 JS），而不会多出一份指向别处的根。
fn container_root_aliases(root: &Path) -> Vec<PathBuf> {
    let mut roots = vec![root.to_path_buf()];
    for (left, right) in ROOT_PREFIX_ALIASES {
        for (from, to) in [(left, right), (right, left)] {
            // `strip_prefix` 同样按整段 component 比较，`/vartmp` 不会被当成 `/var`。
            if let Ok(rest) = root.strip_prefix(from) {
                roots.push(Path::new(to).join(rest));
            }
        }
    }
    roots
}

/// 一条发送源的分派结论。既是分派依据，也是分派日志的**去重键**。
///
/// 用枚举而不是 `Option<PathBuf>`：日志要能说出**为什么**退回 JS，尤其是
/// 「判定为容器外」——别名表漏了一种写法时，那是现场唯一的证据。
#[derive(Debug, PartialEq, Eq)]
enum SourceRoute {
    /// 容器内的 `file://`：本进程直读这条路径。
    Direct(PathBuf),
    /// 交给 JS 读，附带原因。
    Delegate(DelegateReason),
}

/// 退回 JS 的四种原因。**都不是错误**，只是本进程没把握读得了、或判不出归属。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DelegateReason {
    /// 认不出私有数据区形态，快路径整条停用（见 [`sandbox_container_root`]）。
    FastPathDisabled,
    /// 不是 `file://`——`content://` 的读权限在 `ContentResolver` 手里。
    ForeignScheme,
    /// 是 `file://`，但路径含 `..`，词法前缀比较判不出归属。
    Unresolvable,
    /// 是 `file://`，但落在容器（及其别名写法）之外。
    OutsideContainer,
}

/// 分派日志的计数槽数量，必须容得下 [`SourceRoute::log_slot`] 的所有下标。
const DISPATCH_SLOTS: usize = 5;

impl SourceRoute {
    /// 计数槽下标 + 人读标签。
    ///
    /// **没有 `_` 兜底分支**：新增一种分派结论就必须在这里给它一个槽，
    /// 否则编译不过——否则新结论会静默并进别人的计数里，日志就骗人了。
    fn log_slot(&self) -> (usize, &'static str) {
        match self {
            Self::Direct(_) => (0, "本进程直读"),
            Self::Delegate(DelegateReason::FastPathDisabled) => (1, "退回 JS：快路径未启用"),
            Self::Delegate(DelegateReason::ForeignScheme) => (2, "退回 JS：非 file:// 源"),
            Self::Delegate(DelegateReason::Unresolvable) => (3, "退回 JS：路径含 .. 判不出归属"),
            Self::Delegate(DelegateReason::OutsideContainer) => (4, "退回 JS：判定为容器外"),
        }
    }
}

/// 判定一条发送源该由谁来读：本进程直读，还是退回 JS。
///
/// **这不是安全边界，是「开得开吗」的能力判据。** 两条分支跑在同一个进程、用同一套
/// 凭据——退回 JS 的那条最终也是 `expo-file-system` 在本进程里 `open()`。所以判据放宽
/// 的后果是「Rust 能不能打开这个文件」，不是「用户的文件会不会被读到」。记住这一点，
/// 下面三条的分寸才说得通。
///
/// 1. **scheme 必须是 `file://`**——`content://` 的读权限在 `ContentResolver`
///    那边，Rust 裸 `open()` 拿不到。
/// 2. **路径不含 `..`**——[`Path::starts_with`] 按 component 比较但**不规范化**，
///    `<容器>/../../etc/passwd` 会大摇大摆通过前缀检查。这里做的是**保守改道而非拦截**：
///    含 `..` 只说明「判不出归属」，于是交给 JS——JS 照样把它读出来，什么都没被挡住。
///    真正的拦截只在发布侧（[`ensure_within`]），因为那边写错位置会在用户目录里留下
///    东西、且不可逆；这边最坏只是由另一种语言读到了一个本来就读得到的文件。
/// 3. **必须落在应用沙箱容器（或它的别名写法）之内**——iOS 上 `pickDirectoryAsync()`
///    给的是**安全作用域 URL**（expo 替我们 `startAccessingSecurityScopedResource()`
///    过），Rust 裸 `open()` 会 EPERM。归属白名单正是为此存在，
///    **不能放宽成「任何 `file://` 都直读」**。
///
/// **已知限制：判定是纯词法的，不 `canonicalize`。** 容器内一条指向容器外的符号链接
/// 会被判成「容器内」，于是由 Rust 直读。可接受，理由就是开头那条——改变的只有「谁来
/// 读」，JS 分支顺着同一条链接会得到同样的字节。而 `canonicalize` 的代价落在**每一次
/// 读块**上（一次 2 GB 发送 8000 多次），正是这条快路径要省掉的那种 syscall；想靠缓存
/// 摊掉又得再引一张随 source 增长的表，把刚拆掉的东西装回来。发布侧的取舍相反：
/// 那边一次发布只验一次，且写错位置不可逆，所以那边必须 `canonicalize`。
///
/// ⚠️ 别把这条与 [`crate::file_staging`] 里「随机写只施加于本进程完全拥有的 fd」混为
/// 一谈：那条约束**只管写**。读一个外部 fd 不会破坏任何不变量——最坏是读失败并上抛，
/// 而写失败会在用户目录留下半成品、或让已收的数据错位。
///
/// 取 `roots` 而不是 `&self`：它不碰适配器的任何其他状态，独立成函数才能在测试里用
/// 手写的容器根跑判定，不必造一个会去建目录的 [`StagingArea`]。
fn route_source(roots: &[PathBuf], source: &FileSourceId) -> SourceRoute {
    if roots.is_empty() {
        return SourceRoute::Delegate(DelegateReason::FastPathDisabled);
    }
    if !source.0.starts_with(LOCAL_SCHEME) {
        return SourceRoute::Delegate(DelegateReason::ForeignScheme);
    }
    let path = crate::utils::parse_host_dir(&source.0);
    if path.components().any(|c| c == Component::ParentDir) {
        return SourceRoute::Delegate(DelegateReason::Unresolvable);
    }
    if !roots.iter().any(|root| path.starts_with(root)) {
        return SourceRoute::Delegate(DelegateReason::OutsideContainer);
    }
    SourceRoute::Direct(path)
}

/// 取 URI 的 scheme 用于日志。
///
/// **刻意不打完整 URI**：那里面是用户的文件名与目录结构，而这条日志只需要知道形态。
fn source_scheme(source_id: &str) -> &str {
    source_id
        .split_once("://")
        .map_or("(无 scheme)", |(scheme, _)| scheme)
}

/// 把 [`ForeignFileAccess`] 适配为 core 的 [`FileAccess`]。
///
/// **不是纯转发层。** 接收侧的写入（建 sink / 写块 / 清理 / 发布到本地目标）全部由
/// 本进程的 [`StagingArea`] 直接做 POSIX IO，一次都不跨语言边界；只有两件平台独占的事
/// 委托给 JS：往 SAF 目标发布，以及删除 SAF 上已落地的文件。
///
/// 发送侧的读取**按 source 分派**（2026-08-10 起）：落在应用沙箱容器内的 `file://`
/// 由本进程直读，其余仍走 JS——Android 选目录发送时
/// （`Directory.pickDirectoryAsync()` → SAF tree）拿到的是 `content://`，
/// 那种 URI 的读权限在 `ContentResolver` 手里，Rust 拿不到。
pub(crate) struct MobileFileAccessAdapter {
    foreign: Arc<dyn ForeignFileAccess>,
    staging: StagingArea,
    /// 允许本进程直读的发送源归属根，含同一目录的别名写法（见
    /// [`container_root_aliases`]）。**空 = 认不出宿主的目录形态，全部退回 JS。**
    owned_source_roots: Vec<PathBuf>,
    /// 分派日志的计数器，按结论分槽，见 [`Self::log_source_dispatch`]。
    dispatch_counts: [AtomicUsize; DISPATCH_SLOTS],
}

impl MobileFileAccessAdapter {
    pub(crate) fn new(foreign: Arc<dyn ForeignFileAccess>, data_dir: &Path) -> Self {
        let owned_source_roots = sandbox_container_root(data_dir)
            .map(|root| container_root_aliases(&root))
            .unwrap_or_default();
        // 启动期打一行：后面每条分派结论要靠它才解释得通
        //（「全都走了 JS」到底是因为源确实是 SAF，还是因为根本没识别出容器根）。
        if owned_source_roots.is_empty() {
            warn!(
                "认不出私有数据区形态（{}），读源一律走 JS 桥",
                data_dir.display()
            );
        } else {
            let roots = owned_source_roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>()
                .join("、");
            debug!("读源快路径已启用，归属根: {roots}");
        }
        Self {
            foreign,
            staging: StagingArea::new(data_dir),
            owned_source_roots,
            dispatch_counts: std::array::from_fn(|_| AtomicUsize::new(0)),
        }
    }

    /// 分派日志：每种**结论**首次出现记一行，此后每 [`DISPATCH_LOG_EVERY`] 次记一行。
    ///
    /// 这行存在的唯一理由是**下一份真机日志要能回答「快路径到底命中没有」**。此前
    /// 日志里没有任何 source 信息，「移动端发送源是 `file://` 还是 `content://`」全靠
    /// 推断——而这两种情况下，按 scheme 分派的收益一个是几倍、一个是零。
    ///
    /// **不能每块打**：一次 2 GB 发送是 8000 多次读，逐块写日志本身就会变成新的瓶颈。
    ///
    /// **也不能按 source 去重**——那是这条日志的第一版：一张带上限的 source 表，于是
    /// 一次目录发送（几百上千个文件）就把额度用光，此后进程内**再不记录任何分派**，
    /// 恰好在最需要它的下一次发送里失效。按结论分槽计数则天然有界
    /// （[`DISPATCH_SLOTS`] 个计数器，不随 source 增长、无需清空），而它要回答的问题
    /// 本来就是结论级的：快路径命中没有、没命中是卡在哪一条判据上、各占多少块。
    fn log_source_dispatch(&self, route: &SourceRoute, source: &FileSourceId) {
        let (slot, label) = route.log_slot();
        // Relaxed 足够：这些计数只用来抽样，谁先谁后不影响任何判断。
        let count = self.dispatch_counts[slot].fetch_add(1, Ordering::Relaxed) + 1;
        if count != 1 && count % DISPATCH_LOG_EVERY != 0 {
            return;
        }
        debug!(
            count,
            scheme = source_scheme(&source.0),
            "读源分派：{label}"
        );
    }

    /// 发布一条已收齐的暂存文件：按目标 scheme 分派。
    async fn publish(
        &self,
        staging: &Path,
        metadata: HostFileMetadata,
    ) -> AppResult<FinalizedSink> {
        // `StagingArea::open` 已经拒过 `save_dir` 缺失（沒有它算不出暂存位置），
        // 走到这里必然是 Some；这里只是把它取出来分派，不重复报错。
        let Some(CoreSaveLocation::Path { path: save_dir }) = metadata.save_dir.as_ref() else {
            return Err(AppError::Transfer(
                "发布失败：文件元信息缺少保存目录".to_string(),
            ));
        };

        if !save_dir.starts_with(SAF_SCHEME) {
            return publish_to_local(staging, save_dir, &metadata.relative_path).await;
        }

        // 必须交 `file://` URI 而不是裸路径：expo 的 `JavaFile` 走
        // `File(URI.create(uri))`，无 scheme 会抛 `URI is not absolute`。
        let staging_uri = crate::utils::to_host_uri(staging);
        let finalized = self
            .foreign
            .publish_to_target(staging_uri, metadata.into())
            .await
            .map_err(to_app_error)?;
        discard_published_staging(staging).await;
        Ok(finalized.into())
    }
}

/// 删掉一条**已经发布成功**的暂存文件。
///
/// 失败只告警、不上抛：文件已经在目标位置了，把一次成功的接收报成失败是更坏的结果；
/// 残留的暂存由既有的过期回收兜底。
async fn discard_published_staging(staging: &Path) {
    if let Err(e) = tokio::fs::remove_file(staging).await {
        warn!("删除已发布的暂存文件失败: {}, {e}", staging.display());
    }
}

/// 发布到本地（`file://`）目标：建父目录 → 确认没跑出保存目录 → rename。
///
/// **同卷 rename 是原子的、零拷贝。** 默认接收目录（应用私有的 `transfers/`）与
/// 暂存区同分区，所以这是常走的那条路径——改造之后默认配置下的发布几乎不花钱，
/// 真正付拷贝代价的只有 SAF 目标。
///
/// rename 失败一律退回 copy，不去判 `EXDEV`：跨设备只是失败原因之一，而其余原因
/// （目标被占、权限）在 copy 上会以同样的方式失败并报出更贴切的错。为此不值得
/// 引入 `libc` 依赖去读 errno。
async fn publish_to_local(
    staging: &Path,
    save_dir: &str,
    relative_path: &str,
) -> AppResult<FinalizedSink> {
    let root = crate::utils::parse_host_dir(save_dir);
    let target = root.join(relative_path);
    let parent = target
        .parent()
        .ok_or_else(|| AppError::Transfer(format!("发布目标没有父目录: {relative_path}")))?;

    create_dir_within(&root, parent).await?;

    if let Err(rename_err) = tokio::fs::rename(staging, &target).await {
        // **copy 之前必须先删掉可能存在的目标。** `rename` 替换的是符号链接本身，
        // 而 `copy` 会**跟随**它写进被指向的位置——若 `<save_dir>/a.txt` 是一条指向外部的
        // 链接，回退路径就把 `ensure_within` 刚挡住的东西又放了进去。
        // `remove_file` 删的是链接本身、不跟随，于是 copy 落在一个新建文件上。
        // 目标不存在时报 NotFound，忽略即可。
        let _ = tokio::fs::remove_file(&target).await;
        if let Err(copy_err) = tokio::fs::copy(staging, &target).await {
            // 半成品必须删掉（端口契约第 4 条 + spec「publish 失败时清理半成品」）：
            // copy 中途失败会在用户目录留下一个长度不足的文件，看着像收到了，
            // 而暂存还在等重试。SAF 分支在 JS 侧做了同样的事。
            let _ = tokio::fs::remove_file(&target).await;
            return Err(AppError::StorageFailed(format!(
                "发布到 {} 失败：rename({rename_err}) 与 copy({copy_err}) 都没成功",
                target.display()
            )));
        }
        discard_published_staging(staging).await;
    }

    Ok(FinalizedSink {
        uri: crate::utils::to_host_uri(&target),
        dir: crate::utils::to_host_uri(parent),
    })
}

/// 逐层建目录，**每建一层就验一层**。
///
/// 不能先 `create_dir_all(parent)` 再 `ensure_within`：那样一条指向外部的符号链接会被
/// 一路跟随，等验证说「不行」的时候，目录树已经造在保存目录外面了——写入被拦住，
/// 但攻击者指定的目录留在了盘上。逐层验证则在踩到那条链接的下一层之前就停。
///
/// `target` 必须词法上位于 `root` 之下（调用方用 `root.join(relative_path)` 构造，
/// 而 `relative_path` 已过 core 的 `is_safe_relative_path`）。
async fn create_dir_within(root: &Path, target: &Path) -> AppResult<()> {
    let relative = target.strip_prefix(root).map_err(|_| {
        AppError::Transfer(format!(
            "发布目录不在保存目录之下: {} vs {}",
            target.display(),
            root.display()
        ))
    })?;

    let mut current = root.to_path_buf();
    // root 自己也要先在（canonicalize 要求路径存在）。
    tokio::fs::create_dir_all(&current)
        .await
        .map_err(|e| AppError::StorageFailed(format!("创建接收目录失败: {e}")))?;
    for segment in relative.components() {
        current.push(segment);
        tokio::fs::create_dir_all(&current)
            .await
            .map_err(|e| AppError::StorageFailed(format!("创建接收目录失败: {e}")))?;
        ensure_within(root, &current).await?;
    }
    Ok(())
}

/// 断言 `candidate` 解析后仍在 `root` 之内（两侧都 `canonicalize`，因此穿透符号链接）。
///
/// 词法检查看不见文件系统：保存目录下若存在一个指向外部的符号链接
/// （`~/Downloads/SwarmDrop/sub` → `/etc`），一条完全合法的 `sub/x.txt` 照样写到目录外，
/// 而路径字符串里没有任何可疑之处。桌面早有这道防线
/// （`src-tauri/src/host/file_sink/path_ops.rs` 的同名函数），移动端此前**没有**——
/// JS 侧直接 `new File(baseDir, relativePath)` 就写了。发布路径统一进 Rust 之后顺手补齐。
///
/// `canonicalize` 必须在 `create_dir_all` 之后：它要求路径已存在。
async fn ensure_within(root: &Path, candidate: &Path) -> AppResult<()> {
    let real_root = tokio::fs::canonicalize(root)
        .await
        .map_err(|e| AppError::StorageFailed(format!("解析保存目录失败: {e}")))?;
    let real_candidate = tokio::fs::canonicalize(candidate)
        .await
        .map_err(|e| AppError::StorageFailed(format!("解析发布目录失败: {e}")))?;
    if real_candidate.starts_with(&real_root) {
        return Ok(());
    }
    Err(AppError::Transfer(format!(
        "拒绝写入保存目录之外：{} 解析到 {}",
        candidate.display(),
        real_candidate.display()
    )))
}

/// [`read_at_sync`] 的 async 外壳，契约见它。
///
/// **必须 spawn_blocking**，理由同 `StagingArea::write_at`：mobile-core 从不自建 runtime，
/// 所有 async 都挤在 async-compat 那根 `new_current_thread` 上。在它上面同步读盘会把整个
/// 网络事件循环一起卡住——而「每块冻住事件循环」正是读源快路径要消掉的病，
/// 在这里再犯一次等于原地打转。
///
/// 取 `PathBuf` 而非 `&Path`：调用点手上本就是一个待移进闭包的 owned 路径，
/// 借引用只会逼出一次多余的 `to_path_buf`。
async fn read_at(path: PathBuf, offset: u64, length: usize) -> AppResult<Vec<u8>> {
    tokio::task::spawn_blocking(move || read_at_sync(&path, offset, length))
        .await
        .map_err(|e| AppError::StorageFailed(format!("读源任务失败: {e}")))?
}

/// 精确按 `[offset, offset+length)` 读，越过文件尾自动截断（EOF 之后读返回空）。
///
/// 这是 `FileAccess::read_source_chunk` 的契约（权威文档在 `crates/host` 的 `ports.rs`）：
/// 调用方——bao outboard 构建与 sender 的逐块推送——依赖返回字节数与请求一致。**不要**
/// 退回「offset 取整到 chunk、忽略 length」的旧语义：那会让 blake3 的 subtree 断言在
/// prepare 阶段直接 panic（2026-07 桌面那次「小文本正常、图片必炸」的事故）。
///
/// `length == 0` 必须返回空且**不报错**：sender 对空文件会发一次 `(0, 0)` 的读。
///
/// 与桌面 `src-tauri/src/host/file_source/path_ops.rs` 的同名函数逐行同义。两份分开而不是
/// 抽公共 crate：它是各自宿主适配器的私有实现细节，提到 `crates/host` 会让端口层反过来
/// 依赖某一个宿主的读法。
fn read_at_sync(path: &Path, offset: u64, length: usize) -> AppResult<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path)?;
    // seek 越过 EOF 是合法的，后续 read 自然返回 0 → 越界读得空、尾部自动截断。
    // 不预查文件大小：省一次 fstat，也消掉 stat 与 read 之间文件变短的竞态窗口。
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; length];
    let mut filled = 0;
    while filled < length {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

fn to_app_error(e: FfiError) -> AppError {
    e.into()
}

#[async_trait]
impl FileAccess for MobileFileAccessAdapter {
    async fn source_metadata(&self, source: &FileSourceId) -> AppResult<HostFileMetadata> {
        let m = self
            .foreign
            .source_metadata(source.0.clone())
            .await
            .map_err(to_app_error)?;
        Ok(m.into())
    }

    /// 与 [`publish`](Self::publish) 和 [`delete_finalized_file`](Self::delete_finalized_file)
    /// 同一条原则：只有平台独占的能力才委托给 JS。判据见 [`route_source`]。
    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        let route = route_source(&self.owned_source_roots, source);
        self.log_source_dispatch(&route, source);

        match route {
            SourceRoute::Direct(path) => read_at(path, offset, length).await,
            SourceRoute::Delegate(_) => self
                .foreign
                .read_source_chunk(source.0.clone(), offset, length as u64)
                .await
                .map_err(to_app_error),
        }
    }

    async fn create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        self.staging.open(metadata, /* truncate */ true).await
    }

    async fn open_or_create_sink(&self, metadata: HostFileMetadata) -> AppResult<FileSinkId> {
        self.staging.open(metadata, /* truncate */ false).await
    }

    async fn write_sink_chunk(
        &self,
        sink: &FileSinkId,
        offset: u64,
        data: Vec<u8>,
    ) -> AppResult<()> {
        self.staging.write_at(sink, offset, data).await
    }

    async fn finalize_sink(&self, sink: &FileSinkId) -> AppResult<FinalizedSink> {
        let (staging_path, metadata) = self.staging.take(sink)?;
        self.publish(&staging_path, metadata).await
    }

    async fn cleanup_sink(&self, sink: &FileSinkId) -> AppResult<()> {
        self.staging.discard(sink).await;
        Ok(())
    }

    /// 与 [`publish`](Self::publish) 用同一个判据分派：只有 SAF 需要 `ContentResolver`。
    /// `file://` 的那些 URI 正是 `publish_to_local` 自己造的（`to_host_uri`），
    /// 没有理由绕一圈 JS 再删。
    async fn delete_finalized_file(&self, uri: &str) -> AppResult<()> {
        if uri.starts_with(SAF_SCHEME) {
            return self
                .foreign
                .delete_finalized_file(uri.to_string())
                .await
                .map_err(to_app_error);
        }
        match tokio::fs::remove_file(crate::utils::parse_host_dir(uri)).await {
            Ok(()) => Ok(()),
            // 删除幂等——「文件已不存在」不算错误，是端口契约写死的。
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::StorageFailed(format!("删除已落地文件失败: {e}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Mutex;

    /// 只记录调用、不真读盘的 [`ForeignFileAccess`]——用来断言「这条 source 有没有
    /// 被推给 JS」。分派测试关心的就是这一个事实。
    #[derive(Default)]
    struct RecordingForeign {
        reads: Mutex<Vec<String>>,
    }

    impl RecordingForeign {
        fn delegated(&self) -> Vec<String> {
            self.reads.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ForeignFileAccess for RecordingForeign {
        async fn source_metadata(
            &self,
            _source_id: String,
        ) -> Result<MobileFileMetadata, FfiError> {
            unreachable!("分派测试不走元信息")
        }

        async fn read_source_chunk(
            &self,
            source_id: String,
            _offset: u64,
            length: u64,
        ) -> Result<Vec<u8>, FfiError> {
            self.reads.lock().unwrap().push(source_id);
            Ok(vec![0xEE; length as usize])
        }

        async fn publish_to_target(
            &self,
            _staging_uri: String,
            _metadata: MobileFileMetadata,
        ) -> Result<MobileFinalizedSink, FfiError> {
            unreachable!("分派测试不走发布")
        }

        async fn delete_finalized_file(&self, _uri: String) -> Result<(), FfiError> {
            unreachable!("分派测试不走删除")
        }
    }

    /// 造一个「容器根 = `base`、私有数据区 = `base/files`」的适配器，即 Android 的形态。
    fn adapter_in(base: &Path, foreign: Arc<RecordingForeign>) -> MobileFileAccessAdapter {
        MobileFileAccessAdapter::new(foreign, &base.join("files"))
    }

    fn temp_base(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// 两个平台的私有数据区形态都要认得，认不出的必须**失败关闭**。
    ///
    /// 失败关闭是这条推导能成立的全部前提：宿主哪天换了 `getPrivateDataDir()` 的形状，
    /// 结果是「退回 JS，慢但对」，而不是「拿一个错误的根去放行容器外的路径」。
    #[test]
    fn container_root_is_derived_from_both_platform_layouts() {
        assert_eq!(
            sandbox_container_root(Path::new("/data/user/0/com.yexiyue.swarmdrop/files")),
            Some(PathBuf::from("/data/user/0/com.yexiyue.swarmdrop")),
            "Android：<容器>/files"
        );
        assert_eq!(
            sandbox_container_root(Path::new(
                "/var/mobile/Containers/Data/Application/ABC/Library/Application Support"
            )),
            Some(PathBuf::from("/var/mobile/Containers/Data/Application/ABC")),
            "iOS：<容器>/Library/Application Support"
        );
        assert_eq!(
            sandbox_container_root(Path::new(
                "/var/mobile/Containers/Data/Application/ABC/Library"
            )),
            None,
            "只剥了一半的 iOS 形态不算认得"
        );
        assert_eq!(
            sandbox_container_root(Path::new("/tmp/whatever")),
            None,
            "认不出的形态必须失败关闭"
        );

        // 「尾部形态匹配、但剥完什么都不剩」是唯一会失败**开放**的那一类：剥出
        // `""` 或 `/`，而 `Path::starts_with("")` 对任何路径恒真、`starts_with("/")`
        // 对任何绝对路径恒真，归属判据会整条失效。上面四条恰好全绕开了这一类。
        for degenerate in [
            "files",
            "/files",
            "Library/Application Support",
            "/Library/Application Support",
        ] {
            assert_eq!(
                sandbox_container_root(Path::new(degenerate)),
                None,
                "{degenerate}：剥完不是一个真目录，必须当作认不出"
            );
        }
    }

    /// 别名表必须让「同一个目录的两种写法」互认。
    ///
    /// 漏了它，快路径会**整片静默失效**：发送照跑，只是又回到每块一次 `invokeBlocking`
    /// ——症状是「没提速」，而日志里一切正常。这条测试和下面那条是唯一的看守。
    #[test]
    fn container_roots_cover_platform_symlink_aliases() {
        let aliases = |root: &str| container_root_aliases(Path::new(root));
        let ios = "/var/mobile/Containers/Data/Application/ABC";
        let ios_private = "/private/var/mobile/Containers/Data/Application/ABC";

        assert_eq!(
            aliases(ios),
            vec![PathBuf::from(ios), PathBuf::from(ios_private)],
            "iOS：/var 是 /private/var 的符号链接"
        );
        assert_eq!(
            aliases(ios_private),
            vec![PathBuf::from(ios_private), PathBuf::from(ios)],
            "反方向同样要认——宿主给哪一种写法不由我们决定"
        );
        assert_eq!(
            aliases("/data/user/0/com.yexiyue.swarmdrop"),
            vec![
                PathBuf::from("/data/user/0/com.yexiyue.swarmdrop"),
                PathBuf::from("/data/data/com.yexiyue.swarmdrop"),
            ],
            "Android：/data/data 是 /data/user/0 的符号链接"
        );
        assert_eq!(
            aliases("/data/user/10/com.yexiyue.swarmdrop"),
            vec![PathBuf::from("/data/user/10/com.yexiyue.swarmdrop")],
            "表里没有的形态不展开——失败关闭，慢但对"
        );
    }

    /// 分派结论必须逐条对得上：换一种写法仍走直读，退回 JS 的四种原因**各自可辨**。
    ///
    /// 原因可辨不是为了好看——下一轮真机日志判断 D1 有没有生效，靠的就是能不能把
    /// 「源本来就是 SAF」与「判定为容器外」这两种「都走了 JS」区分开。
    ///
    /// 直接跑 [`route_source`] 而不造适配器：判定是纯词法的、不碰盘，也就不必为了
    /// 一次判定去 `create_dir_all` 一个 iOS 容器路径。
    #[test]
    fn route_source_reports_each_conclusion() {
        let container = "/var/mobile/Containers/Data/Application/ABC";
        let roots = container_root_aliases(Path::new(container));
        let route = |uri: &str| route_source(&roots, &FileSourceId(uri.into()));

        // 同一个文件，宿主换一种写法给出来。
        assert_eq!(
            route(&format!("file:///private{container}/tmp/a.bin")),
            SourceRoute::Direct(PathBuf::from(format!("/private{container}/tmp/a.bin"))),
            "/private/var 与 /var 是同一个目录，必须走直读"
        );
        assert_eq!(
            route("file:///var/mobile/Media/DCIM/100APPLE/IMG_0001.JPG"),
            SourceRoute::Delegate(DelegateReason::OutsideContainer),
            "容器外的 file:// 退回 JS，且结论要在日志里认得出来"
        );
        assert_eq!(
            route("content://com.android.providers.downloads/document/42"),
            SourceRoute::Delegate(DelegateReason::ForeignScheme),
            "SAF 的读权限在 ContentResolver 手里"
        );
        assert_eq!(
            route(&format!("file://{container}/../../etc/passwd")),
            SourceRoute::Delegate(DelegateReason::Unresolvable),
            "含 `..` 判不出归属：保守改道给 JS，不是拦截"
        );
        assert_eq!(
            route_source(&[], &FileSourceId(format!("file://{container}/tmp/a.bin"))),
            SourceRoute::Delegate(DelegateReason::FastPathDisabled),
            "没有归属根时一切都退回 JS"
        );
    }

    /// 快路径必须逐字节满足端口契约，且**一次都不推给 JS**。
    ///
    /// 断言集与桌面 `path_ops.rs` 的 `test_read_at_exact_offset_and_length` 同源——
    /// 两份实现同一条契约，就得回答同样的问题。98061 这个大小取自 2026-07 那次
    /// 「offset 取整」事故的真实文件（>16KiB 且非对齐）。
    #[tokio::test]
    async fn owned_source_is_read_directly_with_exact_ranges() {
        let base = temp_base("swarmdrop_read_source_direct");
        let foreign = Arc::new(RecordingForeign::default());
        let adapter = adapter_in(&base, foreign.clone());

        let cache = base.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let file = cache.join("我的 报告.bin");
        let data: Vec<u8> = (0..98061u32).map(|i| (i % 256) as u8).collect();
        std::fs::write(&file, &data).unwrap();
        // 经 URI 往返：真实调用方给的就是 percent-encoded 的 `file://`。
        let source = FileSourceId(crate::utils::to_host_uri(&file));

        assert_eq!(
            adapter
                .read_source_chunk(&source, 16384, 16384)
                .await
                .unwrap(),
            &data[16384..32768],
            "非对齐 offset 必须精确定位"
        );
        assert_eq!(
            adapter
                .read_source_chunk(&source, 98061 - 100, 16384)
                .await
                .unwrap(),
            &data[98061 - 100..],
            "尾部不足 length 必须截断到 EOF"
        );
        assert!(
            adapter
                .read_source_chunk(&source, 98061, 1024)
                .await
                .unwrap()
                .is_empty(),
            "offset 落在 EOF 上要得空而不是报错"
        );
        assert!(
            adapter
                .read_source_chunk(&source, 200_000, 1024)
                .await
                .unwrap()
                .is_empty(),
            "offset 越过 EOF 同样得空"
        );

        // 空文件的 `(0, 0)` 读：sender 对零字节文件真的会发这么一次。
        let empty = cache.join("empty.bin");
        std::fs::File::create(&empty).unwrap();
        let empty_source = FileSourceId(crate::utils::to_host_uri(&empty));
        assert!(
            adapter
                .read_source_chunk(&empty_source, 0, 0)
                .await
                .unwrap()
                .is_empty(),
            "length == 0 必须返回空且不报错"
        );

        assert!(
            foreign.delegated().is_empty(),
            "容器内的 file:// 一次都不该跨语言边界，实际: {:?}",
            foreign.delegated()
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// 三类「不该直读」的 source 必须原样退回 JS——而不是被 Rust 拿去开一个开不了
    /// 的文件，也不是被前缀检查放行。
    #[tokio::test]
    async fn foreign_sources_still_go_through_js() {
        let base = temp_base("swarmdrop_read_source_delegate");
        let foreign = Arc::new(RecordingForeign::default());
        let adapter = adapter_in(&base, foreign.clone());

        // ① SAF：读权限在 ContentResolver 那边。
        let saf = FileSourceId("content://com.android.providers.downloads/document/42".into());
        // ② 容器外的 file://：iOS `pickDirectoryAsync()` 的安全作用域 URL 就长这样，
        //    Rust 裸 open() 会 EPERM。
        let outside = FileSourceId(crate::utils::to_host_uri(&temp_base(
            "swarmdrop_read_source_outside",
        )));
        // ③ 含 `..`：Path::starts_with 不规范化，这条**词法上**在容器内，于是判不出
        //    归属、保守改道给 JS（JS 照样读得到它——这里做的不是拦截）。
        let escaping = FileSourceId(format!(
            "{}/../../etc/passwd",
            crate::utils::to_host_uri(&base.join("cache"))
        ));

        for source in [&saf, &outside, &escaping] {
            assert_eq!(
                adapter.read_source_chunk(source, 0, 8).await.unwrap().len(),
                8,
                "{} 必须由 JS 应答",
                source.0
            );
        }

        assert_eq!(
            foreign.delegated(),
            vec![saf.0.clone(), outside.0.clone(), escaping.0.clone()],
            "三条都要原样推给 JS"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// 认不出私有数据区形态时，连容器内的 `file://` 也退回 JS——即改动前的行为。
    #[tokio::test]
    async fn unrecognised_data_dir_disables_the_fast_path_entirely() {
        let base = temp_base("swarmdrop_read_source_unknown_layout");
        let foreign = Arc::new(RecordingForeign::default());
        // 尾部既不是 `files` 也不是 `Library/Application Support`。
        let adapter = MobileFileAccessAdapter::new(foreign.clone(), &base.join("mystery"));

        let file = base.join("a.bin");
        std::fs::write(&file, b"payload").unwrap();
        let source = FileSourceId(crate::utils::to_host_uri(&file));

        assert_eq!(
            adapter
                .read_source_chunk(&source, 0, 4)
                .await
                .unwrap()
                .len(),
            4
        );
        assert_eq!(foreign.delegated(), vec![source.0]);

        let _ = std::fs::remove_dir_all(&base);
    }

    /// 同卷发布走 rename：暂存不该留下副本，也不该产生一次全量拷贝。
    ///
    /// 这是默认配置下的常走路径——用户没设接收目录时回退到应用私有的 `transfers/`，
    /// 与暂存区同分区。真正付拷贝代价的只有 SAF 目标。
    #[tokio::test]
    async fn publish_to_local_renames_within_the_same_volume() {
        let base = std::env::temp_dir().join("swarmdrop_publish_local_rename");
        let _ = std::fs::remove_dir_all(&base);
        let save_dir = base.join("save");
        std::fs::create_dir_all(&save_dir).unwrap();
        let staging = base.join("deadbeef");
        std::fs::write(&staging, b"payload").unwrap();

        let finalized = publish_to_local(&staging, save_dir.to_str().unwrap(), "docs/a.txt")
            .await
            .expect("publish");

        let target = save_dir.join("docs").join("a.txt");
        assert_eq!(std::fs::read(&target).unwrap(), b"payload");
        assert!(!staging.exists(), "同卷发布是 rename，暂存不该还在");
        assert_eq!(finalized.uri, crate::utils::to_host_uri(&target));
        assert_eq!(
            finalized.dir,
            crate::utils::to_host_uri(&save_dir.join("docs")),
            "dir 必须是真实父目录——收件箱的「打开文件夹」只认它"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// 保存目录下的符号链接可以把一条**完全合法**的相对路径引到目录外。
    ///
    /// 词法检查（core 的 `is_safe_relative_path`）看不见文件系统：`sub/a.txt` 里
    /// 没有 `..`、没有绝对路径，可 `sub` 本身是指向别处的链接。只有实地
    /// `canonicalize` 拦得住。桌面早有这道防线，移动端此前完全没有——
    /// JS 侧 `new File(baseDir, relativePath)` 直接就写了。
    #[cfg(unix)]
    #[tokio::test]
    async fn publish_to_local_rejects_escape_through_symlink() {
        let base = std::env::temp_dir().join("swarmdrop_publish_local_escape");
        let _ = std::fs::remove_dir_all(&base);
        let save_dir = base.join("save");
        let outside = base.join("outside");
        std::fs::create_dir_all(&save_dir).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, save_dir.join("sub")).unwrap();

        let staging = base.join("cafebabe");
        std::fs::write(&staging, b"payload").unwrap();

        let result = publish_to_local(&staging, save_dir.to_str().unwrap(), "sub/a.txt").await;

        assert!(result.is_err(), "经符号链接写到保存目录外必须被拒绝");
        assert!(
            !outside.join("a.txt").exists(),
            "被拒绝之后目录外不能留下任何东西"
        );

        // 多层路径：逐层验证必须在造出 `outside/a/b` 之前就停下。
        // 一次性 create_dir_all 会先跟着链接把整棵子树建好，再由 ensure_within 拒绝写入
        // ——写是拦住了，目录却留在了保存目录外面。
        let nested = publish_to_local(&staging, save_dir.to_str().unwrap(), "sub/a/b/c.txt").await;
        assert!(nested.is_err(), "多层逃逸同样必须被拒绝");
        assert!(
            !outside.join("a").exists(),
            "被拒绝时不能在保存目录外留下任何新建目录"
        );
        assert!(staging.exists(), "拒绝不该顺手把暂存删了——它还要重试");

        let _ = std::fs::remove_dir_all(&base);
    }
}
