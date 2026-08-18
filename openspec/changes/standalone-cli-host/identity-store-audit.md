# `identity_store.rs` 归属审计（任务 1.1 产物）

对 `src-tauri/src/host/identity_store.rs`（580 行）逐项判定归属。
**结论：除路径解析外全部平台中立**，可整体下沉。

## 下沉到 `crates/host`（平台中立）

| 项 | 说明 |
|---|---|
| `IdentityFile` | 磁盘格式（`keypair` + `webrtcCertificatePem`），两字段均 `#[serde(default)]`；**刻意不 derive `Debug`**（会打印私钥字节，绕过 `DeviceIdentityBytes` 的 redacting Debug） |
| `IDENTITY_FILE` / `PAIRED_DEVICES_FILE` | 文件名常量——属于磁盘格式约定，改名会读不到存量数据 |
| `ReadJsonError` | `Io` 与 `Parse` 两类必须分开，调用方按数据可恢复性各自决定降不降级 |
| `read_json` | 不存在→`None`、可解析→`Some`、否则按两类报错；**绝不降级为默认值** |
| `Durability` | `Rename` / `Fsync` 两级，差别只在崩溃丢不丢最后一次写 |
| `write_json_atomic` + `write_text_atomic_blocking` | 同目录临时文件 → 可选 `sync_all` → 原子 `persist`；四条安全属性（随机名、`O_EXCL`、unix 0600、提前返回清残留）交给 `tempfile` |
| `identity_write_lock()` | 进程级 static async 锁，罩住 identity 的整段读-改-写 |
| `update_identity` / `clear_identity_field` | RMW 收口；后者在文件不存在时直接返回，不凭空造字段 |
| `KeychainProvider` 全部 6 个方法 | — |
| `PairedDeviceStore` 两个方法 | 含两条非对称降级策略（见下） |
| 8 个测试用例 | 全部是行为断言，无桌面依赖 |

### 必须随实现一起迁移的三条判据

1. **identity 读取失败不降级**——降级会让 core 走「生成新身份并覆盖原文件」，一次坏块静默换掉身份。
2. **paired-devices 的非对称降级**——解析失败降级为空列表（内容确已坏，重试无用，不该锁死应用）；
   **I/O 失败必须上抛**（内容可能仍完好，降级后下一次 save 会用空快照原子覆盖掉好文件，
   把可恢复故障变成永久数据丢失）。
3. **设备列表不 fsync、密钥文件 fsync**——前者高频重写且 macOS 的 `F_FULLFSYNC` 要 1–20ms，
   「不损坏」靠 rename 原子性而非 fsync；后者丢了不可恢复。

## 留在桌面侧（平台特有）

| 项 | 说明 |
|---|---|
| `DesktopIdentityStore::new(app: &AppHandle)` | **唯一的 Tauri 依赖**，只用于算 `app_local_data_dir`（Windows 上刻意用本机目录而非漫游目录） |
| 模块文档中「为什么不用 keychain」一节 | ad-hoc 签名 / macOS DR 匹配的历史归因，属于桌面语境 |
| `identity_path()` 的对外暴露 | 方法本身中立，但消费方（诊断 UI）在桌面侧 |

## 下沉时必须做的两处调整

1. **`read_json` 的 `tokio::fs::read_to_string` → 同步 `std::fs`**。`crates/host` 要过 wasm
   双 target 门禁，而 tokio 的 `fs` feature 在 wasm 上不存在（`device_config_file.rs` 已按此
   约定处理）。载荷只有几百字节，读取在微秒级。
   **写路径无需改动**——它已经是「同步 IO 包在一次 `spawn_blocking` 里」，只需要 tokio 的
   `rt` feature，而 `crates/host` 已有。
2. **`crates/host` 需新增 `tempfile` 依赖**（workspace 根已有 `tempfile = "3"`）。

## 命名与构造（对齐 `device_config_file.rs`）

- 模块 `identity_store_file.rs`，类型 `JsonFileIdentityStore`
- 构造取**目录**而非两个文件路径：文件名是这份实现的磁盘格式约定，不应让每个宿主各自重复
  （`JsonFileDeviceConfig` 取文件路径，是因为它只有一个文件，无重复风险）
