## Context

SwarmDrop 的文件传输接收方需要记录保存位置。桌面端使用文件系统绝对路径（如 `/home/user/Downloads/SwarmDrop`），Android 端使用 MediaStore 公共目录（`Download/SwarmDrop`）。当前 `save_path` 字段是 `Option<String>`，在 Android 端存储的是显示字符串 `"Download"`，无法用于打开文件夹操作。`fileUris`/`saveDirUri` 作为运行时数据随 `TransferCompleteEvent` 传递，但 `completeSession` 直接移除 session 导致这些数据丢失，数据库也未持久化。

## Goals / Non-Goals

**Goals:**

- 从类型层面区分桌面端和 Android 端的保存位置，消除歧义
- 数据库持久化保存位置的完整信息，使历史记录也能正确打开文件
- 前端根据 `SaveLocation` 类型分支处理打开逻辑，不再依赖独立的 `fileUris`/`saveDirUri` 运行时字段
- 平滑迁移旧数据

**Non-Goals:**

- 不改变 Android 端的实际文件存储方式（仍使用 MediaStore 写入 `Download/SwarmDrop`）
- 不改变桌面端的文件选择和保存流程
- 不在数据库中持久化 Android `content://` URI（这些 URI 可能随系统重启失效，应动态 resolve）

## Decisions

### 1. SaveLocation 枚举设计

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SaveLocation {
    /// 桌面端：文件系统绝对路径
    #[serde(rename_all = "camelCase")]
    Path { path: String },
    /// Android 端：公共目录子目录
    #[serde(rename_all = "camelCase")]
    AndroidPublicDir { subdir: String },
}
```

**理由**：使用 `serde(tag = "type")` 产生 `{"type":"path","path":"..."}` 或 `{"type":"androidPublicDir","subdir":"SwarmDrop"}`，前端天然映射为 TypeScript discriminated union。

**替代方案**：使用两个独立列 `save_type` + `save_path`。被否决，因为增加了列数且两列之间存在隐式关联。

### 2. 数据库存储方式

`save_path` 列类型不变（仍为 `TEXT NULL`），但内容从裸字符串改为 JSON 字符串。

**理由**：
- 无需修改列类型，SQLite 的 TEXT 列存 JSON 完全没问题
- SeaORM 2.0 支持通过 `DeriveValueType` + serde 自动序列化/反序列化 JSON 列
- Migration 只需 UPDATE 现有数据的内容格式

### 3. Migration 策略

新增 migration `m20260310_000001_save_location_enum`：
- 将桌面端旧数据 `save_path = "/some/path"` 转换为 `{"type":"path","path":"/some/path"}`
- 将 Android 端旧数据 `save_path = "Download"` 转换为 `{"type":"androidPublicDir","subdir":"SwarmDrop"}`
- NULL 值保持不变

**判断依据**：旧数据中 Android 端 `save_path` 固定为 `"Download"`，桌面端为绝对路径（以 `/` 或盘符开头）。Migration 按此规则分类转换。

### 4. TransferCompleteEvent 简化

移除 `file_uris` 和 `save_dir_uri` 字段，改为只携带 `save_location: Option<SaveLocation>`。

**理由**：Android URI 不适合持久化（重启后可能失效），前端打开文件时应动态 resolve。`SaveLocation::AndroidPublicDir { subdir }` 提供了足够的信息让前端在运行时通过 `AndroidFs.resolveInitialLocation()` 获取有效 URI。

### 5. 前端 acceptReceive 参数调整

`acceptReceive` 命令的 `save_path: String` 参数改为 `save_location: SaveLocation`。前端在接受传输时，桌面端传 `{ type: "path", path: "..." }`，Android 端传 `{ type: "androidPublicDir", subdir: "SwarmDrop" }`。

**理由**：将平台感知前移到用户选择阶段，Rust 端不再需要 `build_file_sink` 中的 `#[cfg]` 平台判断——直接根据枚举 variant 构造 `FileSink`。

### 6. 前端打开文件逻辑

`openTransferResult` 根据 `SaveLocation.type` 分支：
- `path`：使用 `revealItemInDir` 或 `openPath`（现有桌面端逻辑）
- `androidPublicDir`：动态 resolve URI 后调用 `showViewDirDialog` / `showViewFileDialog`

## Risks / Trade-offs

- **旧数据迁移判断**：用 `"Download"` 硬编码判断 Android 旧数据，如果用户曾自定义过（当前不可能，Android 端固定写 `SwarmDrop`），可能误判 → 风险极低，当前代码 Android 端 `save_dir_display()` 硬编码返回 `"Download"`
- **Android URI 不持久化**：每次打开都需 resolve → 开销可忽略，且避免了 URI 过期问题
- **Breaking change**：前端类型从 `savePath?: string` 变为 `saveLocation?: SaveLocation`，需同步更新所有使用处 → 一次性修改，全部在本项目内
