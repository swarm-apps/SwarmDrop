//! 数据目录解析。
//!
//! **命令行宿主的目录与图形界面宿主的目录刻意不同**：CLI 使用独立的设备身份
//! （spec: cli-host「独立的设备身份」），共用目录会让两个进程抢同一份 identity 与
//! 同一个数据库。目录分开是那条规格在文件系统上的兑现，不是巧合。
//!
//! 「数据目录下都有什么」全部收在 [`DataDir`] 的方法里。各处自己拼路径的话，
//! 改一个文件名就要改 N 处，而漏掉的那处会静默地指向一个永远不存在的文件。

use std::path::{Path, PathBuf};

use crate::exit::{CliError, CliResult};

/// 身份与已配对设备文件所在目录（两个文件名由端口层的共享实现决定）。
const DATABASE_FILE: &str = "swarmdrop.db";
/// 本地通道：其余命令经它复用正在运行的节点。
const SOCKET_FILE: &str = "swarmdrop.sock";
/// 单实例仲裁锁。
const LOCK_FILE: &str = "swarmdrop.lock";
/// 设备名等用户配置。
const DEVICE_CONFIG_FILE: &str = "device_config.json";

/// 已解析并确保存在的数据目录。
///
/// 构造即保证目录存在——让「目录在不在」只在一个地方回答，后续所有路径使用点
/// 都不必再各自 `create_dir_all`。
#[derive(Debug, Clone)]
pub struct DataDir(PathBuf);

impl DataDir {
    /// 解析数据目录：显式覆盖优先，否则取平台约定位置。
    ///
    /// **取 `data_local_dir` 而非 `data_dir`**：Windows 上后者是 `%APPDATA%`（Roaming），
    /// 会被域漫游配置文件同步到服务器——私钥不该跟着漫游。macOS 与 Linux 上两者解析到
    /// 同一处，所以这不是平台分叉，只是在 Windows 上落到了对的那个。
    pub fn resolve(explicit: Option<PathBuf>) -> CliResult<Self> {
        let dir = match explicit {
            Some(dir) => dir,
            None => directories::ProjectDirs::from("com", "yexiyue", "swarmdrop-cli")
                .ok_or_else(|| {
                    CliError::NodeUnavailable(
                        "无法确定数据目录（HOME 未设置？）；请用 --data-dir 显式指定".into(),
                    )
                })?
                .data_local_dir()
                .to_path_buf(),
        };

        std::fs::create_dir_all(&dir).map_err(|err| {
            CliError::NodeUnavailable(format!("创建数据目录 {} 失败: {err}", dir.display()))
        })?;

        Ok(Self(dir))
    }

    /// 目录本身。身份存储取它即可——两个文件名属于端口层共享实现的磁盘格式约定。
    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn database(&self) -> PathBuf {
        self.0.join(DATABASE_FILE)
    }

    pub fn socket(&self) -> PathBuf {
        self.0.join(SOCKET_FILE)
    }

    pub fn lock(&self) -> PathBuf {
        self.0.join(LOCK_FILE)
    }

    pub fn device_config(&self) -> PathBuf {
        self.0.join(DEVICE_CONFIG_FILE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_override_is_used_and_created() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("nested").join("data");
        assert!(!target.exists());

        let dir = DataDir::resolve(Some(target.clone())).expect("resolve");

        assert_eq!(dir.path(), target);
        assert!(target.is_dir(), "构造应确保目录存在");
    }

    /// 每个已知文件都落在数据目录内，且互不重名。
    ///
    /// 「都在目录内」防的是某个拼接不慎逃到目录外；「互不重名」防的是两份数据
    /// 共用一个文件而互相覆盖。
    #[test]
    fn known_files_live_inside_the_directory_and_are_distinct() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = DataDir::resolve(Some(tmp.path().to_path_buf())).expect("resolve");

        let paths = [
            dir.database(),
            dir.socket(),
            dir.lock(),
            dir.device_config(),
        ];
        for path in &paths {
            assert_eq!(path.parent(), Some(dir.path()), "{path:?} 不在数据目录内");
        }

        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(unique.len(), paths.len(), "存在重名文件");
    }

    /// 默认目录必须与图形界面宿主区分开——两者共用会让 CLI 抢桌面端的身份与数据库。
    ///
    /// 只断言「路径里带得出 cli 这个限定」，不写死具体位置：那由平台约定决定，
    /// 写死等于把 `directories` 的实现细节钉进测试。
    #[test]
    fn default_directory_is_distinct_from_the_gui_host() {
        let Some(project) = directories::ProjectDirs::from("com", "yexiyue", "swarmdrop-cli")
        else {
            return; // 无 HOME 的环境（少数 CI 沙箱），该场景由 resolve 的错误分支覆盖
        };
        let path = project.data_local_dir().to_string_lossy().to_lowercase();
        assert!(
            path.contains("cli"),
            "默认数据目录未与图形界面宿主区分: {path}"
        );
    }
}
