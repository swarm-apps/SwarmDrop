//! 文件访问 bridge —— 把 core 的 [`FileAccess`] 拆成「本进程自己做」与
//! 「只能委托 RN 做」两部分。
//!
//! ## 数据流
//!
//! ```text
//! core::TransferManager
//!   ├─ read_source_chunk(source, offset, length) ──► RN expo-fs.read()
//!   │      发送源的 URI 不受我们控制：Android 选目录发送走 SAF tree，是 content://
//!   │
//!   ├─ create_sink / write_sink_chunk / cleanup_sink ──► StagingArea（纯 Rust POSIX IO）
//!   │      接收暂存恒在应用私有目录，**一次都不跨语言边界**
//!   │
//!   └─ finalize_sink ──► 按目标 scheme 分派
//!          file://    ──► publish_to_local（Rust：建目录 + 逃逸校验 + rename）
//!          content:// ──► RN publish_to_target（只有 ContentResolver 建得了 document）
//! ```
//!
//! 接收的随机写为什么必须留在本进程，见 [`crate::file_staging`] 的模块文档。
//!
//! ## 关键约束
//!
//! - 所有方法 async；callback 不能在 Rust 持锁时调用（uniffi-bindgen-rn 会死锁）
//! - `MobileFileMetadata` / `MobileFinalizedSink` 用 uniffi Record 实现类型安全

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use swarmdrop_core::host::{
    CoreSaveLocation, FileAccess, FileSinkId, FileSourceId, FinalizedSink, HostFileMetadata,
};
use swarmdrop_core::{AppError, AppResult};
use tracing::warn;

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
    // 超长/取整；bao outboard 构建会按 16KiB 粒度、非对齐 offset 调用。JS 实现在
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

/// 把 [`ForeignFileAccess`] 适配为 core 的 [`FileAccess`]。
///
/// **不是纯转发层。** 接收侧的写入（建 sink / 写块 / 清理 / 发布到本地目标）全部由
/// 本进程的 [`StagingArea`] 直接做 POSIX IO，一次都不跨语言边界；只有两件平台独占的事
/// 委托给 JS：往 SAF 目标发布，以及删除 SAF 上已落地的文件。
///
/// 发送侧的读取仍走 JS——源 URI 不受我们控制，Android 上选目录发送时
/// （`Directory.pickDirectoryAsync()` → SAF tree）拿到的就是 `content://`。
pub(crate) struct MobileFileAccessAdapter {
    foreign: Arc<dyn ForeignFileAccess>,
    staging: StagingArea,
}

impl MobileFileAccessAdapter {
    pub(crate) fn new(foreign: Arc<dyn ForeignFileAccess>, data_dir: &Path) -> Self {
        Self {
            foreign,
            staging: StagingArea::new(data_dir),
        }
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

    async fn read_source_chunk(
        &self,
        source: &FileSourceId,
        offset: u64,
        length: usize,
    ) -> AppResult<Vec<u8>> {
        self.foreign
            .read_source_chunk(source.0.clone(), offset, length as u64)
            .await
            .map_err(to_app_error)
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
