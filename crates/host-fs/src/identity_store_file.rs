//! [`KeychainProvider`] 与 [`PairedDeviceStore`] 的 JSON 文件实现 —— 原生宿主共用的那一份。
//!
//! ## 为什么端口层会做 IO
//!
//! 与 [`crate::device_config_file`] 同一条判据：桌面与命令行宿主会写出**逐行同构**的两份
//! 实现——同一套磁盘格式、同一套原子写、同一套降级策略、连护栏测试都同名同义，而两份之间
//! 唯一的真差异是**目录从哪来**（桌面从 `app_local_data_dir` 解析，CLI 从自己的数据目录
//! 解析），那是构造点的事，不属于端口实现本身。
//!
//! 于是把「这两个文件怎么读写」下沉成端口的平台中立默认实现，每个原生宿主只留「目录怎么
//! 算出来」。
//!
//! 整模块 `cfg(not(target_family = "wasm"))`：浏览器没有文件系统，Web 的端口实现是
//! `crates/web` 里的 IndexedDB 版本。
//!
//! ## 安全形态
//!
//! 私钥以**明文**存储，unix 上权限 0600，等同于无 passphrase 的 `~/.ssh/id_ed25519`。
//! 防护边界是「其他用户读不到」，**不是**「同用户下的其他进程读不到」。
//!
//! ## 三条不可协商的判据
//!
//! 1. **原子写**（[`write_json_atomic`]）——直接 `fs::write` 覆盖时，写到一半断电就是一个
//!    截断的 JSON。私钥文件损坏不可恢复（身份丢失 = 所有配对失效）。
//! 2. **身份读取失败不降级**（[`read_json`]）——降级为默认值会让上层走「生成新身份并覆盖
//!    原文件」的路径。一次磁盘坏块就静默换掉身份，用户只看到「设备列表空了」。
//! 3. **设备列表的降级是非对称的**（[`JsonFileIdentityStore::load_paired_devices`]）——解析
//!    失败降级、I/O 失败上抛。两者合并处理时，任一方向都有一种灾难。
//!
//! 三条各有一条护栏测试看守，改实现必须同时改测试。

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use swarmdrop_host::device::PairedDeviceInfo;
use swarmdrop_host::error::{AppError, AppResult};
use swarmdrop_host::ports::{DeviceIdentityBytes, KeychainProvider, PairedDeviceStore};

/// 密钥材料。近乎只读——首次生成后几乎不再写。
const IDENTITY_FILE: &str = "identity.json";
/// 已配对设备列表。业务数据，配对 / 解除配对 / 对端改名都会重写。
const PAIRED_DEVICES_FILE: &str = "paired-devices.json";

/// `identity.json` 的内容。
///
/// **只装密钥材料**：已配对设备列表在另一个文件里。两者分开不是洁癖——设备列表会随
/// identify 观察到的对端改名而重写，与密钥同文件意味着每次改名都擦写一遍承载私钥的文件，
/// 每次都是一个新的损坏窗口。
///
/// ⚠️ **改字段要同步 `crates/core/examples/seed_demo_profile.rs`**：demo fixture 的
/// `identity.json` 是照这个结构手抄的第二份（那个 example 够不到本模块的私有类型）。
/// 两个字段都带 `#[serde(default)]`，所以漏同步是**静默**的——fixture 少一个字段只会
/// 得到默认值、看起来完全正常，直到录 demo 时才发现。
///
/// **刻意不 derive `Debug`**：它会把私钥字节原样打印出来，而端口层专门给
/// [`DeviceIdentityBytes`] 手写了一个 redacting 的 `Debug`（`keypair: "<redacted>"`）。
/// 在同一条数据路径上留一个 derive 等于给那道防线开一个旁路，且 derive 不触发
/// unused 警告——只能靠不写。
#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentityFile {
    /// protobuf-encoded Ed25519 keypair。`None` = 无身份，触发上层生成新身份。
    ///
    /// 用 `Option` 而不是「空 Vec 当哨兵」：同一个 struct 里两种「缺席」编码并存，
    /// 会让每个读点都得记住「空 Vec 也算没有」。
    #[serde(default)]
    keypair: Option<Vec<u8>>,
    /// WebRTC Direct 证书完整 PEM（含私钥）。整体持久化，仅重建密钥会改变 certhash，
    /// 让已经发出的邀请地址失效。
    #[serde(default)]
    webrtc_certificate_pem: Option<String>,
}

/// [`KeychainProvider`] + [`PairedDeviceStore`] 的 JSON 文件实现：一个目录 = 一份设备身份。
///
/// 两个端口由同一个结构体实现，但各读写各自的文件——端口分开的理由（密钥是秘密、
/// 设备列表是业务数据）在存储布局上同样成立。
///
/// 构造取**目录**而非两个文件路径：文件名属于这份实现的磁盘格式约定（改名会读不到存量
/// 数据），不该让每个宿主各自重复一遍。这与 [`crate::device_config_file::JsonFileDeviceConfig`]
/// 取文件路径的做法不同，因为那里只有一个文件、没有重复的余地。
#[derive(Debug, Clone)]
pub struct JsonFileIdentityStore {
    identity_path: PathBuf,
    paired_devices_path: PathBuf,
}

impl JsonFileIdentityStore {
    /// `dir` 是存放两个文件的目录；不存在时由写入侧 `create_dir_all`。
    ///
    /// 宿主应传**本机数据目录**而非漫游目录：Windows 上 `%APPDATA%` 会被域漫游配置文件
    /// 同步到服务器，私钥不该跟着漫游。
    pub fn new(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            identity_path: dir.join(IDENTITY_FILE),
            paired_devices_path: dir.join(PAIRED_DEVICES_FILE),
        }
    }

    /// 密钥文件的绝对路径。诊断界面要把它展示给用户——身份是明文文件，
    /// 「存在哪」是备份 / 迁移 / 收紧权限的前提。
    pub fn identity_path(&self) -> &Path {
        &self.identity_path
    }

    fn read_identity(&self) -> AppResult<IdentityFile> {
        Ok(read_json(&self.identity_path)
            .map_err(ReadJsonError::into_app)?
            .unwrap_or_default())
    }

    /// 读一份 → 改一个字段 → 原子写回，**整段持锁**。
    ///
    /// 四个 save/delete 方法共用它，而不是各写一遍这三步：那个不变量复制四份之后，
    /// 给 [`IdentityFile`] 加第三个字段就会有第五、第六份，其中任意一份漏掉
    /// `read_identity()` 就是静默丢字段。
    ///
    /// **锁必须罩住「读」到「写」的整段**，只锁写那一下等于没锁：`save_identity` 与
    /// `save_webrtc_certificate_pem` 并发时（界面的 `start` 撞上 MCP 的
    /// `ensure_node_running`，两者都会进 `load_or_create_webrtc_certificate`），两边各自
    /// 读到同一份快照，后写的那份把先写的字段覆盖掉。丢 `webrtcCertificatePem` 意味着
    /// certhash 变、已发出的邀请地址全部失效；丢 `keypair` 更糟——下次启动会静默生成新
    /// 身份，所有配对作废。
    async fn update_identity(
        &self,
        mutate: impl FnOnce(&mut IdentityFile) + Send,
    ) -> AppResult<()> {
        let _guard = identity_write_lock().lock().await;
        let mut file = self.read_identity()?;
        mutate(&mut file);
        write_json_atomic(&self.identity_path, &file, Durability::Fsync).await
    }

    /// 删一个字段：与 [`Self::update_identity`] 的区别是**文件不存在时直接返回**，
    /// 而不是凭空造出一个 `{"keypair": null, "webrtcCertificatePem": null}`。
    ///
    /// 被替换的 keychain 后端把 `NoEntry` 当成删除成功，这里保持同一语义：删除一个不存在
    /// 的东西是 no-op，不该产生副作用，更不该为此付一次 fsync。
    async fn clear_identity_field(
        &self,
        mutate: impl FnOnce(&mut IdentityFile) + Send,
    ) -> AppResult<()> {
        let _guard = identity_write_lock().lock().await;
        let Some(mut file) =
            read_json::<IdentityFile>(&self.identity_path).map_err(ReadJsonError::into_app)?
        else {
            return Ok(());
        };
        mutate(&mut file);
        write_json_atomic(&self.identity_path, &file, Durability::Fsync).await
    }
}

/// `identity.json` 的读-改-写串行化。
///
/// **进程级 static 而不是实例字段**：组装点的两个端口工厂各自 `new` 一个
/// [`JsonFileIdentityStore`]，实例级锁根本罩不到彼此。跨进程不在此列——那由各宿主自己的
/// 单实例机制把关（桌面是 single-instance 插件，命令行是数据目录上的文件锁）。
///
/// 用 async 锁（`tokio::sync::Mutex`）是因为临界区里有 `.await`（原子写），
/// `std::sync::MutexGuard` 不能跨 await 存活。
///
/// 只保护 `identity.json`：`paired-devices.json` 这侧没有读-改-写（`save_paired_devices`
/// 是整份覆写），它的读-改-写在上层。
fn identity_write_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[async_trait]
impl KeychainProvider for JsonFileIdentityStore {
    async fn load_identity(&self) -> AppResult<Option<DeviceIdentityBytes>> {
        Ok(self
            .read_identity()?
            .keypair
            .map(|keypair| DeviceIdentityBytes { keypair }))
    }

    async fn save_identity(&self, identity: DeviceIdentityBytes) -> AppResult<()> {
        self.update_identity(|file| file.keypair = Some(identity.keypair))
            .await
    }

    async fn delete_identity(&self) -> AppResult<()> {
        self.clear_identity_field(|file| file.keypair = None).await
    }

    async fn load_webrtc_certificate_pem(&self) -> AppResult<Option<String>> {
        Ok(self.read_identity()?.webrtc_certificate_pem)
    }

    async fn save_webrtc_certificate_pem(&self, pem: String) -> AppResult<()> {
        self.update_identity(|file| file.webrtc_certificate_pem = Some(pem))
            .await
    }

    async fn delete_webrtc_certificate_pem(&self) -> AppResult<()> {
        self.clear_identity_field(|file| file.webrtc_certificate_pem = None)
            .await
    }
}

#[async_trait]
impl PairedDeviceStore for JsonFileIdentityStore {
    /// **解析失败**降级为空列表，**I/O 失败**上抛。两者不能合并处理。
    ///
    /// - 解析失败降级：内容确定坏了，重试无用。而上层的 `initialize_identity` 里那句
    ///   `load_paired_devices().await?` 会把 `Err` 变成「`deviceId` 为 null → 节点永远起
    ///   不来」，且诊断里给的是**身份文件**路径、不是这份，用户连该删哪个文件都不知道。
    ///   一条坏记录不该锁死整个应用——大不了重新配对。
    /// - I/O 失败**必须**上抛：文件内容**可能仍然是完好的**（Windows 上 Defender / 索引器
    ///   的 `ERROR_SHARING_VIOLATION`、任何平台的瞬时 `EIO`）。若这里也降级成空列表，
    ///   下一次 `save_paired_devices` 就会拿着这份空快照**原子覆盖掉那个完好的文件**——
    ///   一次可恢复的瞬时故障就此变成永久数据丢失，全程零报错。
    ///
    /// 与 `identity.json` 的对照：那份两类都上抛（见 [`read_json`]），因为它丢了不可恢复。
    async fn load_paired_devices(&self) -> AppResult<Vec<PairedDeviceInfo>> {
        match read_json::<Vec<PairedDeviceInfo>>(&self.paired_devices_path) {
            Ok(devices) => Ok(devices.unwrap_or_default()),
            Err(ReadJsonError::Parse(message)) => {
                tracing::warn!("[identity-store] {message}，本次以空列表继续（配对关系需重建）");
                Ok(Vec::new())
            }
            Err(err @ ReadJsonError::Io(_)) => Err(err.into_app()),
        }
    }

    async fn save_paired_devices(&self, devices: &[PairedDeviceInfo]) -> AppResult<()> {
        // 设备列表**不 fsync**：它每次配对、解除配对、改策略、以及 identify 观测到对端
        // 改名时都会重写，而 macOS 上 `sync_all` 走 `F_FULLFSYNC`（典型 1–20ms 且阻塞一个
        // blocking 线程）。「不损坏」这条不变量靠的是 rename 的原子性，不是 fsync；这里
        // 崩溃最坏丢掉最后一次改名观测，下次 identify 自会补上。私钥不同——它丢了不可恢复。
        write_json_atomic(&self.paired_devices_path, devices, Durability::Rename).await
    }
}

/// 读失败的两种，**调用方必须分开处理**。
///
/// 「文件内容坏了」与「这次没读成」是不同的事：前者重试多少次都一样，后者下次可能就好了。
/// 把两者合成一个 `Err` 会逼调用方二选一，而两个选择各有一种灾难——见
/// [`JsonFileIdentityStore::load_paired_devices`] 的文档。
#[derive(Debug)]
enum ReadJsonError {
    /// 读不出来（权限、被占用、坏块…）。文件内容**可能仍然是好的**。
    Io(String),
    /// 读出来了但解析不了。文件内容确定是坏的。
    Parse(String),
}

impl ReadJsonError {
    fn into_app(self) -> AppError {
        match self {
            Self::Io(message) | Self::Parse(message) => AppError::Identity(message),
        }
    }
}

/// 读一个 JSON 文件：不存在 → `Ok(None)`，存在且可解析 → `Ok(Some)`，否则按上面两类报错。
///
/// **绝不降级为默认值**——那会让 `load_identity()` 返回 `Ok(None)`，上层于是走「生成新身份」
/// 的路径**并覆盖原文件**。一次磁盘坏块、一次被外部工具改坏，就把身份不可恢复地换掉了，
/// 而用户看到的只是「设备列表空了」，没有任何错误提示。降不降级由调用方按数据的可恢复性
/// 决定。
///
/// 用**同步 `std::fs`** 而非 `tokio::fs`：本 crate 必须过 wasm 双 target 门禁，而 tokio 的
/// `fs` feature 在 wasm 上根本不存在（与 [`crate::device_config_file`] 同一条约定）。
/// 载荷只有几百字节到几 KB，读取在微秒级，阻塞 async 上下文的代价可以忽略。
/// **写路径是另一回事**——它可能 fsync（macOS 上 1–20ms），所以走 `spawn_blocking`。
fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, ReadJsonError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(ReadJsonError::Io(format!(
                "读取 {} 失败: {err}",
                path.display()
            )));
        }
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|err| ReadJsonError::Parse(format!("解析 {} 失败: {err}", path.display())))
}

/// 写入的持久化强度。
///
/// 两级的差别只在崩溃时**丢不丢最后一次写**——「文件不会损坏」两级都保证，那由 rename
/// 的原子性给。
#[derive(Clone, Copy)]
enum Durability {
    /// 只靠 rename 的原子替换。崩溃可能丢掉最后一次写，但文件不会是半截的。
    Rename,
    /// rename 之前先 `sync_all`。给「丢了不可恢复」的数据用。
    Fsync,
}

/// 原子写：同目录临时文件 → （可选落盘）→ 原子替换。
///
/// 同一文件系统内的替换在 POSIX 与 Windows 上都是原子的，因此目标文件在任何时刻要么是
/// 上一个完整版本、要么是新的完整版本，不存在截断态。
///
/// **不 fsync 父目录**：那只影响「新内容在崩溃后是否保住」，不影响「文件是否损坏」——
/// 替换未持久化时留下的是旧文件的完整内容，仍满足上面那条不变量。
///
/// 四条安全属性交给 [`tempfile`] 而不是自己搓：随机文件名、`O_EXCL` 创建、unix `0o600`、
/// 以及**任何提前返回都在 drop 时清掉残留**。自己写这四条要 55 行，其中一半篇幅是在向
/// 读者证明标准库的 `open(2)` 语义——比如「`mode` 只在真正创建文件时生效，所以复用一个
/// 残留 tmp 会让私钥继承它的权限」。那份证明这个生态已经做过一次了。
///
/// 整个函数在**一次** `spawn_blocking` 里用同步 IO 完成：逐步用 `tokio::fs` 的话这里就是
/// 5 次 blocking pool 往返，而载荷只有几 KB。
async fn write_json_atomic<T: Serialize + ?Sized>(
    path: &Path,
    value: &T,
    durability: Durability,
) -> AppResult<()> {
    let text = serde_json::to_string_pretty(value).map_err(AppError::Serialization)?;
    let path = path.to_path_buf();

    tokio::task::spawn_blocking(move || write_text_atomic_blocking(&path, &text, durability))
        .await?
}

fn write_text_atomic_blocking(path: &Path, text: &str, durability: Durability) -> AppResult<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .ok_or_else(|| AppError::Identity(format!("{} 没有父目录", path.display())))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AppError::Identity(format!("创建 {} 失败: {e}", parent.display())))?;

    // 与目标**同目录**：跨文件系统的替换会失败（EXDEV）。
    // `.suffix(".tmp")` 是给测试里那条「不留残留」的断言用的——tempfile 默认把 `.tmp`
    // 放在**前缀**。
    let mut tmp = tempfile::Builder::new()
        .suffix(".tmp")
        .tempfile_in(parent)
        .map_err(|e| AppError::Identity(format!("创建临时文件失败: {e}")))?;

    tmp.write_all(text.as_bytes())
        .map_err(|e| AppError::Identity(format!("写入 {} 失败: {e}", path.display())))?;
    if matches!(durability, Durability::Fsync) {
        tmp.as_file()
            .sync_all()
            .map_err(|e| AppError::Identity(format!("落盘 {} 失败: {e}", path.display())))?;
    }

    tmp.persist(path)
        .map_err(|e| AppError::Identity(format!("替换 {} 失败: {e}", path.display())))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例一个独立的临时目录，**返回的 `TempDir` 是 RAII guard**——用例结束即删。
    ///
    /// 调用点必须把它绑到一个活到用例结束的名字上（`let (_dir, store) = …`），否则目录
    /// 会在这一行之后立刻被删掉。
    fn temp_store(tag: &str) -> (tempfile::TempDir, JsonFileIdentityStore) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("swarmdrop_identity_{tag}_"))
            .tempdir()
            .expect("tempdir");
        let store = JsonFileIdentityStore::new(dir.path());
        (dir, store)
    }

    /// 目录里所有 `.tmp` 残留（文件名），用于「写完不留临时文件」这条断言。
    fn temp_files_in(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn identity_round_trips() {
        let (_dir, store) = temp_store("roundtrip");

        assert!(store.load_identity().await.unwrap().is_none());

        store
            .save_identity(DeviceIdentityBytes {
                keypair: vec![1, 2, 3],
            })
            .await
            .unwrap();

        assert_eq!(
            store.load_identity().await.unwrap().map(|i| i.keypair),
            Some(vec![1, 2, 3])
        );
    }

    /// 证书与 keypair 同文件，但互不覆盖。
    #[tokio::test]
    async fn certificate_and_keypair_coexist() {
        let (_dir, store) = temp_store("coexist");

        store
            .save_identity(DeviceIdentityBytes { keypair: vec![7] })
            .await
            .unwrap();
        store
            .save_webrtc_certificate_pem("PEM".to_string())
            .await
            .unwrap();

        assert_eq!(
            store.load_identity().await.unwrap().map(|i| i.keypair),
            Some(vec![7])
        );
        assert_eq!(
            store
                .load_webrtc_certificate_pem()
                .await
                .unwrap()
                .as_deref(),
            Some("PEM")
        );
    }

    /// **护栏**：损坏的身份文件必须报错，而不是被当成「没有身份」。
    ///
    /// 降级为 `Ok(None)` 会让上层生成新身份并覆盖原文件——一次可恢复的读取故障
    /// 就此变成不可恢复的身份丢失，且全程无提示。
    #[tokio::test]
    async fn corrupt_identity_file_errors_instead_of_resetting() {
        let (dir, store) = temp_store("corrupt");
        let path = dir.path().join(IDENTITY_FILE);
        std::fs::write(&path, b"{ not json").unwrap();

        assert!(store.load_identity().await.is_err());
        // 原文件必须原样保留，供人工抢救。
        assert_eq!(std::fs::read(&path).unwrap(), b"{ not json");
    }

    /// **护栏**：写设备列表不得触碰密钥文件。
    ///
    /// 两者同文件时，每次对端改名都会擦写一遍承载私钥的文件。
    #[tokio::test]
    async fn saving_paired_devices_does_not_touch_identity_file() {
        let (dir, store) = temp_store("isolation");
        store
            .save_identity(DeviceIdentityBytes { keypair: vec![9] })
            .await
            .unwrap();
        let before = std::fs::read(dir.path().join(IDENTITY_FILE)).unwrap();

        store.save_paired_devices(&[]).await.unwrap();

        let after = std::fs::read(dir.path().join(IDENTITY_FILE)).unwrap();
        assert_eq!(before, after);
    }

    /// **护栏**：设备列表存得进、读得回。
    ///
    /// 这份文件的 on-disk 形状是**顶层数组**，而它有两个手写的跨语言消费者
    /// （`e2e/.../lan-transfer.demo.ts` 与 `seed_demo_profile.rs`）。没有往返测试的话，
    /// 序列化形状变了只会在录 demo 时才发现。
    #[tokio::test]
    async fn paired_devices_round_trip() {
        use swarmdrop_host::device::OsInfo;
        use swarmdrop_net_base::SecretKey;

        let (_dir, store) = temp_store("paired-roundtrip");
        assert!(store.load_paired_devices().await.unwrap().is_empty());

        let device = PairedDeviceInfo::new(
            SecretKey::generate().node_id(),
            OsInfo {
                name: Some("书房 Mac".into()),
                hostname: "study-mac".into(),
                os: "macos".into(),
                platform: "macos".into(),
                arch: "aarch64".into(),
                capabilities: Vec::new(),
            },
            1_720_000_000,
        );
        store
            .save_paired_devices(std::slice::from_ref(&device))
            .await
            .unwrap();

        let loaded = store.load_paired_devices().await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].peer_id, device.peer_id);
        assert_eq!(loaded[0].os_info.name.as_deref(), Some("书房 Mac"));
    }

    /// **护栏**：设备列表读坏时降级为空，而不是让整个应用起不来。
    ///
    /// 与 `corrupt_identity_file_errors_instead_of_resetting` 刚好相反，两条要一起看：
    /// 身份坏了必须报错（丢了不可恢复），设备列表坏了必须放行（大不了重新配对）。
    /// 上层 `initialize_identity` 里那句 `load_paired_devices().await?` 会把这里的任何
    /// `Err` 变成「`deviceId` 为 null → 节点永远起不来」。
    #[tokio::test]
    async fn corrupt_paired_devices_degrades_to_empty() {
        let (dir, store) = temp_store("paired-corrupt");
        std::fs::write(dir.path().join(PAIRED_DEVICES_FILE), b"{ not an array").unwrap();

        assert!(store.load_paired_devices().await.unwrap().is_empty());
    }

    /// **护栏**：删一个不存在的东西是 no-op，不该凭空造出文件。
    ///
    /// 被替换的 keychain 后端把 `NoEntry` 当删除成功；`update_identity` 那条路径会写出
    /// 一个 `{"keypair": null, …}` 并附带一次 fsync。
    #[tokio::test]
    async fn deleting_from_absent_file_creates_nothing() {
        let (dir, store) = temp_store("delete-absent");

        store.delete_identity().await.unwrap();
        store.delete_webrtc_certificate_pem().await.unwrap();

        assert!(!dir.path().join(IDENTITY_FILE).exists());
    }

    /// **护栏**：连写多次，始终不留临时文件，且 unix 上权限恒为 0600。
    ///
    /// 写多次而不是一次：单次写只覆盖「新建」路径，而权限最容易出问题的是**覆盖已有
    /// 文件**那条——`mode(0o600)` 只在 `open(2)` 真正创建文件时生效，一个被复用的残留
    /// tmp 会把自己的权限带给最终文件（`persist` 保留源 inode 的 mode），产出一个全局
    /// 可读的、装着明文私钥的 `identity.json`。现在这四条属性由 `tempfile` 保证
    /// （随机名 + `O_EXCL` + 0600 + drop 时清残留），这条测试是它们的回归锚点。
    ///
    /// `#[cfg(unix)]` 只包权限那一半——「无残留」这条在 Windows 上同样要跑。
    #[tokio::test]
    async fn atomic_write_leaves_no_temp_and_restricts_permissions() {
        let (dir, store) = temp_store("atomic");

        for i in 0..3u8 {
            store
                .save_identity(DeviceIdentityBytes { keypair: vec![i] })
                .await
                .unwrap();
            store
                .save_webrtc_certificate_pem(format!("PEM-{i}"))
                .await
                .unwrap();
        }

        let path = dir.path().join(IDENTITY_FILE);
        assert!(path.exists());
        // 扫整个目录而不是猜某个固定名字：tmp 名是 tempfile 随机生成的，写死一个名字的
        // 断言会永远为真——一条测不到任何东西的护栏。
        assert_eq!(temp_files_in(dir.path()), Vec::<String>::new());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
