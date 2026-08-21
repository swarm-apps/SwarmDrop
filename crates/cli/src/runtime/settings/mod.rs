//! 命令行宿主私有的持久化配置。
//!
//! **本模块只管「盘上那份怎么读写」**，不含任何三层来源的判断（那在 [`scalar`]）、也不含
//! 引导清单的合并规则（那在 [`super::bootstrap_nodes`]）。
//!
//! ## 装的是哪两项，为什么不是三项
//!
//! | 项 | 落在哪 | 为什么 |
//! |---|---|---|
//! | 接收落点 | 这里 | 宿主部署配置，core 不认识它 |
//! | 引导节点的增删 | 这里 | 各端各一份清单，core 只收最终地址 |
//! | 设备名 | **`device_config.json`** | 它是 `DeviceConfig` 端口的磁盘格式，四端共用一个实现 |
//!
//! 设备名不搬进来：那个端口是 core 用的（邀请串与配对请求都携带设备名），把落点与引导
//! 清单加进它，移动端与桌面端就要实现两个它们用不上的方法——它们的这两项分别在
//! `receive-location` 与 `preferences-store` 里，语义还不同（移动端的落点是 SAF tree URI）。
//!
//! 反过来，这两项**合在一个文件**而不是各一个：三项以下、同一读写时机、同一生命周期，
//! 拆文件只是把原子写做两遍。
//!
//! ## 两条读写规则
//!
//! 1. **原子写**（同目录临时文件 → rename）。半截的配置文件会让节点下次启动直接起不来，
//!    而用户看到的现象是「我只是改了个接收目录」。
//! 2. **解析失败返回 `Err`，绝不回落默认值**。这条与 `JsonFileIdentityStore` 同一体例：
//!    静默回落意味着用户自己加的那条中继在一次坏块之后**无声地消失**，而他只会发现
//!    「跨网突然连不上了」，没有任何线索指向这个文件。
//!
//! ⚠️ 「文件不存在」**不是**解析失败：那是全新机器的正常形态，读出全部未设置。

pub mod scalar;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::exit::{CliError, CliResult};

/// 盘上 `settings.json` 的全量内容。
///
/// 字段全部可缺省：新增一项时，旧文件仍然读得动（读出「未设置」）。反方向也成立——
/// 未知字段被忽略而不是报错，降级回旧版本不会把配置文件变成一个读不动的东西。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSettings {
    /// 接收落点。`None` = 未设置，回落到 `<下载目录>/SwarmDrop`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive_dir: Option<String>,
    /// 用户对内置引导清单的增删。
    #[serde(default, skip_serializing_if = "BootstrapOverlay::is_empty")]
    pub bootstrap: BootstrapOverlay,
}

/// 用户对**内置**引导清单的叠加。
///
/// ⚠️ **持久化的是这两个集合，不是合并后的最终清单。** 存最终清单会在版本更新更换内置
/// 地址时把老用户永久压在旧地址上：他的清单里躺着一份旧地址快照，新地址永远到不了他
/// 手上，故障形态是「升级后突然连不上」且无法自查。判据与 Web 端同一条
/// （spec: `bootstrap-node-settings`）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapOverlay {
    /// 用户自己添加的地址。
    #[serde(default)]
    pub custom: Vec<String>,
    /// 用户撤销掉的内置地址。**只装内置项**——撤销自定义项是从 `custom` 里拿走，
    /// 不是往这里加（见 [`super::bootstrap_nodes`] 的增删不对称）。
    #[serde(default)]
    pub removed: Vec<String>,
}

impl BootstrapOverlay {
    fn is_empty(&self) -> bool {
        self.custom.is_empty() && self.removed.is_empty()
    }
}

/// 一份配置文件的读写入口。
///
/// 构造是零成本的（只存路径）：多数命令根本不碰它。
#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// `path` 是配置文件本身，不是它的目录。
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// 读出全量配置。文件不存在 = 全部未设置，**不是错误**。
    ///
    /// 解析失败是错误且**不降级**，理由见模块文档。
    pub fn read(&self) -> CliResult<StoredSettings> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|err| {
                CliError::NodeUnavailable(format!(
                    "配置文件 {} 解析失败: {err}\n\
                     它没有被自动重置——里面可能有你自己加的引导节点。\
                     修好它，或删掉它以恢复默认配置。",
                    self.path.display()
                ))
            }),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(StoredSettings::default()),
            Err(err) => Err(CliError::NodeUnavailable(format!(
                "读取配置文件 {} 失败: {err}",
                self.path.display()
            ))),
        }
    }

    /// 读 → 改 → 原子写回，返回改完之后的全量配置。
    ///
    /// **每一次写入都走它**，不要在别处 `read` 完再 `write`：那会把「先读后写」这个窗口
    /// 复制到每个调用点，而本程序的写者不止一个进程（有常驻节点时是它写，没有时是当前
    /// 这条命令写）。
    pub fn update(
        &self,
        edit: impl FnOnce(&mut StoredSettings) -> CliResult<()>,
    ) -> CliResult<StoredSettings> {
        let mut settings = self.read()?;
        edit(&mut settings)?;
        write_atomic(&self.path, &settings)?;
        Ok(settings)
    }
}

/// 原子写：同目录临时文件 → 原子替换。
///
/// 同一文件系统内的替换在 POSIX 与 Windows 上都是原子的，因此目标文件在任何时刻要么是
/// 上一个完整版本、要么是新的完整版本，不存在截断态。
///
/// **不 fsync**：崩溃可能丢掉最后一次写（用户重设一次即可），但文件不会损坏——后者才是
/// 会让节点起不来的那种失败。同一取舍见 `JsonFileIdentityStore` 的 `Durability::Rename`。
///
/// 临时文件交给 [`tempfile`]（随机名 + `O_EXCL` + 提前返回时自动清理），不自己搓。
fn write_atomic(path: &Path, settings: &StoredSettings) -> CliResult<()> {
    use std::io::Write;

    let text = serde_json::to_string_pretty(settings)
        .map_err(|err| CliError::NodeUnavailable(format!("序列化配置失败: {err}")))?;

    let parent = path
        .parent()
        .ok_or_else(|| CliError::NodeUnavailable(format!("{} 没有父目录", path.display())))?;

    // 与目标**同目录**：跨文件系统的替换会失败（EXDEV）。
    let mut tmp = tempfile::Builder::new()
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|err| CliError::NodeUnavailable(format!("创建临时文件失败: {err}")))?;
    tmp.write_all(text.as_bytes())
        .map_err(|err| CliError::NodeUnavailable(format!("写入配置失败: {err}")))?;
    tmp.persist(path)
        .map_err(|err| CliError::NodeUnavailable(format!("替换 {} 失败: {err}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &tempfile::TempDir) -> SettingsStore {
        SettingsStore::new(dir.path().join("settings.json"))
    }

    /// 全新机器：文件不存在 = 全部未设置，不是错误。
    #[test]
    fn a_missing_file_means_nothing_is_set() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(store(&dir).read().expect("读"), StoredSettings::default());
    }

    #[test]
    fn writing_then_reading_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store(&dir);

        store
            .update(|settings| {
                settings.receive_dir = Some("/tmp/drop".into());
                settings.bootstrap.custom.push("/ip4/1.2.3.4/tcp/1".into());
                Ok(())
            })
            .expect("写");

        let read = store.read().expect("读");
        assert_eq!(read.receive_dir.as_deref(), Some("/tmp/drop"));
        assert_eq!(read.bootstrap.custom, ["/ip4/1.2.3.4/tcp/1"]);
        assert!(read.bootstrap.removed.is_empty());
    }

    /// **解析失败必须是错误，不得回落默认。**
    ///
    /// 回落是静默的：用户自己加的中继在一次坏块之后无声消失，而他只会发现「跨网连不上」，
    /// 没有任何线索指向这个文件。
    #[test]
    fn a_corrupt_file_is_an_error_not_a_silent_reset() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{ this is not json").expect("写坏文件");

        let err = SettingsStore::new(path).read().expect_err("必须报错");
        assert!(
            err.to_string().contains("解析失败"),
            "错误没有指向解析: {err}"
        );
    }

    /// 未知字段被忽略而不是报错——降级回旧版本不该把配置文件变成读不动的东西。
    #[test]
    fn unknown_fields_are_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"receiveDir":"/tmp/x","futureThing":42}"#).expect("写");

        let read = SettingsStore::new(path).read().expect("读");
        assert_eq!(read.receive_dir.as_deref(), Some("/tmp/x"));
    }

    /// 写完不留临时文件——残留会在数据目录里越攒越多，且它们与真配置同名前缀。
    #[test]
    fn writing_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        store(&dir)
            .update(|settings| {
                settings.receive_dir = Some("/tmp/drop".into());
                Ok(())
            })
            .expect("写");

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("列目录")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "留下了临时文件: {leftovers:?}");
    }

    /// **两个数据目录互不影响。** 配置随 `--data-dir` 隔离是它落在数据目录里的全部理由。
    #[test]
    fn two_data_dirs_keep_their_own_settings() {
        let one = tempfile::tempdir().expect("tempdir");
        let two = tempfile::tempdir().expect("tempdir");

        store(&one)
            .update(|settings| {
                settings.receive_dir = Some("/tmp/one".into());
                Ok(())
            })
            .expect("写");

        assert_eq!(store(&two).read().expect("读"), StoredSettings::default());
        assert_eq!(
            store(&one).read().expect("读").receive_dir.as_deref(),
            Some("/tmp/one")
        );
    }

    /// 编辑闭包报错时**不落盘**：校验失败不该留下半个改动。
    #[test]
    fn a_rejected_edit_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = store(&dir);

        let result = store.update(|settings| {
            settings.receive_dir = Some("/tmp/nope".into());
            Err(CliError::Usage("拒绝".into()))
        });
        assert!(result.is_err());
        assert!(!dir.path().join("settings.json").exists(), "不该写出文件");
    }
}
