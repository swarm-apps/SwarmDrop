//! 桌面端的文件来源扫描。
//!
//! **这里只剩「给用户挑文件」这件事**——把一棵目录树摊平成前端可渲染、可回传的列表。
//! 它跨 Tauri IPC，所以类型带 `specta::Type`。
//!
//! `FileAccess` 契约的实现（精确读取、暂存写入、发布）**不在这里**，在
//! [`swarmdrop_host_fs::LocalFileAccess`]：那份实现桌面与命令行宿主逐行同构，且它的
//! 契约里记录过两次真实事故（读取按块取整导致验签 panic、续传误截断产出内容有洞的
//! 文件），只应存在一份。目录扫描留下的理由正相反——它不属于那个契约，返回形状是
//! 各宿主界面自己的事。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use swarmdrop_core::AppResult;
use swarmdrop_core::host::FileSourceId;

/// 文件来源：标准文件系统路径。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FileSource {
    Path { path: PathBuf },
}

/// 目录遍历后的扁平化文件条目。
///
/// 同时用于 `scan_sources` 命令的返回与 `prepare_send` 命令的输入，
/// 因此同时派生 Serialize + Deserialize。
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EnumeratedFile {
    /// 文件名。
    pub name: String,
    /// 相对路径（Unix 风格 `/` 分隔符）。
    pub relative_path: String,
    /// 文件来源。
    pub source: FileSource,
    /// 文件大小。
    pub size: u64,
}

impl FileSource {
    /// 这个来源是文件还是目录、多大、叫什么。
    ///
    /// 实现复用 [`swarmdrop_host_fs`] 的那一份——`scan_sources` 只是要 `is_dir` 来决定
    /// 递归与否，没有理由为此再写一次 stat。
    pub async fn stat(&self) -> AppResult<swarmdrop_host_fs::local_fs::source_ops::LocalFileStat> {
        match self {
            Self::Path { path } => swarmdrop_host_fs::local_fs::source_ops::metadata(path).await,
        }
    }

    /// 递归展开这个来源（仅目录有意义）。
    pub async fn enumerate(&self, parent_relative_path: &str) -> AppResult<Vec<EnumeratedFile>> {
        match self {
            Self::Path { path } => enumerate_dir(path, parent_relative_path).await,
        }
    }
}

/// 来源 → 传给核心的标识。
///
/// 桌面把路径原样作为标识；[`LocalFileAccess`](swarmdrop_host_fs::LocalFileAccess)
/// 那侧按「先试 JSON、否则当路径」解回来。
pub fn source_id(source: &FileSource) -> FileSourceId {
    match source {
        FileSource::Path { path } => FileSourceId(path.to_string_lossy().into_owned()),
    }
}

/// 递归遍历目录，返回所有文件的扁平化列表。
pub async fn enumerate_dir(
    path: &std::path::Path,
    parent_relative_path: &str,
) -> AppResult<Vec<EnumeratedFile>> {
    let path = path.to_path_buf();
    let parent = parent_relative_path.to_owned();
    tokio::task::spawn_blocking(move || enumerate_dir_sync(&path, &parent)).await?
}

fn enumerate_dir_sync(
    path: &std::path::Path,
    parent_relative_path: &str,
) -> AppResult<Vec<EnumeratedFile>> {
    use path_slash::PathExt as _;
    use walkdir::WalkDir;

    let mut files = Vec::new();

    for entry in WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_dir() {
            continue;
        }

        let entry_path = entry.path();
        let name = entry_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        let sub_path =
            pathdiff::diff_paths(entry_path, path).unwrap_or_else(|| entry_path.to_path_buf());
        let relative_path = if parent_relative_path.is_empty() {
            sub_path.to_slash_lossy().into_owned()
        } else {
            format!("{}/{}", parent_relative_path, sub_path.to_slash_lossy())
        };

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);

        files.push(EnumeratedFile {
            name,
            relative_path,
            source: FileSource::Path {
                path: entry_path.to_path_buf(),
            },
            size,
        });
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_enumerate_dir() {
        let dir = std::env::temp_dir().join("swarmdrop_test_enum");
        let sub = dir.join("subdir");
        let _ = std::fs::create_dir_all(&sub);
        std::fs::write(dir.join("a.txt"), "aaa").unwrap();
        std::fs::write(sub.join("b.txt"), "bbb").unwrap();

        let files = enumerate_dir(&dir, "root").await.unwrap();
        assert_eq!(files.len(), 2);

        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));

        // 检查相对路径包含前缀
        for f in &files {
            assert!(f.relative_path.starts_with("root/"));
        }

        let _ = std::fs::remove_dir_all(&dir);
    }
}
