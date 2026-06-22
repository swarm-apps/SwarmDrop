# 踩坑合集

按出现频率排序。每条都标了 **症状 / 原因 / 修法**。

## 🔥 高频

### 1. `EventRegistry not found in Tauri state - Did you forget to call Builder::mount_events?`

**症状**：第一次 `Foo::emit(&app)` 立即 panic。

**原因**：注册了 events 但 setup hook 里漏了 `mount_events(app)`。

**修法**：

```rust
.setup(move |app| {
    specta.mount_events(app);    // ← 加这行
    Ok(())
})
```

### 2. `BigInt forbidden`

**症状**：编译时报 `error: Attempted to export type "Foo" containing a BigInt-style type`。

**原因**：默认禁止导出 `u64` / `i64` / `usize` / `isize` / `i128` / `u128` / `f128` 等 BigInt 类型。

**修法**（任选其一）：

```rust
// 方案 1：全局强制转 number（自己负责精度）
SpectaBuilder::new().dangerously_cast_bigints_to_number()

// 方案 2：换更小整数
pub struct Foo { pub size: u32 }   // 不是 u64

// 方案 3：按字段标记可丢精度
pub struct Foo {
    #[specta(type = specta_typescript::Number)]
    pub size: u64,
}

// 方案 4：序列化为 String
pub struct Foo {
    #[specta(type = String)]
    #[serde(with = "...")]
    pub size: u64,
}
```

详见 [types.md](types.md#bigint-处理-关键)。

### 3. `Detected multiple types with the same name: "Foo"`

**症状**：编译时报多个 type 同名冲突。

**原因**：specta v2 type schema **全局只认 type name 不认 module path**。`#[serde(rename)]` 只影响字段不影响 type name。

**修法**：

- **最简单**：真的把 Rust struct 改名（例 `RuntimeFoo` vs `DbFoo`）。
- 备选：用 `Layout::Namespaces` 或 `Layout::ModulePrefixedName`，让 TS 输出按 module 分组。但 `FlatFile` 之外的 layout 各有 trade-off。

### 4. `the trait Event is not implemented for ...`

**症状**：编译报错，明明已经写了 `#[derive(Event)]`。

**原因 + 修法**：

- 检查 Cargo.toml：`tauri-specta` 的 `features` 必须有 `derive`：
  ```toml
  tauri-specta = { version = "=2.0.0-rc.25", features = ["typescript", "derive"] }
  ```
- 检查 import：如果 `use tauri_specta::Event;` 把 trait import 到当前作用域，会**遮蔽**同名 derive macro。derive 写完整路径：
  ```rust
  #[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]  // ← 完整路径
  pub struct Foo;
  ```
- 或者 trait import 改成 `use tauri_specta::Event as _;`（只 import 方法，不引入名字）。

### 5. `the trait Type is not implemented for ...`

**症状**：某个字段类型编译失败。

**原因**：字段类型没 derive `specta::Type`。`Type` 有传染性——整条链路都要 derive。

**修法**：

```rust
#[derive(Type)] pub struct Outer { inner: Inner }    // Outer derive 了
#[derive(Type)] pub struct Inner { ... }              // ← 这个也要 derive
```

如果是**第三方类型**没法 derive：

- 开对应 specta feature（`uuid` / `chrono` / `serde_json` / `url` / `bytes`）
- 或在边界 toString，参数类型用 `String`，函数内 `parse()`
- 或 `#[specta(type = String)]` 让该字段在 schema 里假装是 String

详见 [types.md](types.md#跨-ipc-边界传第三方类型)。

## 中频

### 6. `Attempted to export "xxx" but was unable to due to name "xxx" containing an invalid character`

**症状**：导出报错，提示 type name 含非法字符（如 `-`）。

**原因**：在 wrapper struct 上加了 `#[serde(rename = "kebab-name")]` 之类，specta 把它当 type-level rename。但 TS 标识符不允许带 `-`。

**修法**：**移除 wrapper struct 上的 `serde(rename)`**。tauri-specta 已经自动把 PascalCase struct 名转 kebab-case 作为 wire 事件名，**不需要**手工指定：

```rust
// ❌ 错
#[derive(Type, Event)]
#[serde(rename = "user-created")]    // ← 不要写
pub struct UserCreated(...);

// ✅ 对
#[derive(Type, Event)]
pub struct UserCreated(...);          // ← struct 名 PascalCase 就够
// 自动生成 wire 名 "user-created"
```

如果非要自定义 wire 名，用 derive macro 自己的属性：

```rust
#[derive(Type, tauri_specta::Event)]
#[tauri_specta(event_name = "my-custom-name")]
pub struct UserCreated(...);
```

### 7. `tauri::generate_handler!` 残留与 `specta.invoke_handler()` 冲突

**症状**：命令注册不一致，前端调 `commands.foo()` 失败或行为怪异。

**原因**：迁移过程中保留了旧的 `tauri::generate_handler![...]`，而 builder 又调了 `specta.invoke_handler()`。两个 handler 不能同时存在。

**修法**：把旧的 `tauri::generate_handler!` 整行删掉：

```rust
// ❌ 旧
.invoke_handler(tauri::generate_handler![foo, bar])

// ✅ 新（删除旧行，换成）
.invoke_handler(specta.invoke_handler())
```

具体命令列表搬到 `collect_commands![foo, bar]` 里。

### 8. `__cmd__foo` 隐藏符号找不到

**症状**：`collect_commands![commands::foo]` 编译报错找不到 `__cmd__foo`。

**原因**：`#[tauri::command]` 生成的隐藏符号需要通过**模块路径**访问，但模块用了非 glob `pub use`：

```rust
// ❌
pub use foo::handle;     // 只导出 handle 函数本身，__cmd__handle 访问不到
```

**修法**：glob re-export：

```rust
// ✅
pub use foo::*;          // 把所有公开符号都暴露出来，包括 __cmd__*
```

### 9. async 命令编译报错：`reference to ...`

**症状**：

```rust
async fn bad(name: &str) -> String { ... }
//                ^^^^ Tauri 拒绝
```

**原因**：Tauri 自身的限制（不是 specta）——async 命令不能借用参数。

**修法**：用 owned 类型：

```rust
async fn good(name: String) -> String { ... }
```

### 10. Channel 类型在生成的 TS 里报错

**症状**：`Channel<UploadProgress>` 在 TS 看不到自动 import。

**原因**：`Channel` 需要从 `@tauri-apps/api/core` import；生成的 `bindings.ts` 会自己加 import，但**前端要先安装** `@tauri-apps/api`。

**修法**：

```bash
pnpm add @tauri-apps/api@^2
```

确保版本和 `tauri-specta` 期待的 Tauri v2 兼容。

## 低频但坑

### 11. phase-aware 输出 `Foo_Serialize` / `Foo_Deserialize`，前端不想看到

**症状**：bindings.ts 里同一个类型出现两个版本。

**原因**：specta v2 默认按 serde 阶段拆分。如果 struct 有 `#[serde(skip_serializing)]` / `rename(serialize, deserialize)` / `default` 这类不对称属性，会生成两个别名。

**修法**：

- 如果**有意**让前后端形状不同，保持默认即可——`Foo = Foo_Serialize | Foo_Deserialize` 反映了真实差异。
- 如果只是没注意到对称性，**统一掉 serde 属性**让两端一致。
- 如果整个项目都对称，**全局关掉**：
  ```rust
  SpectaBuilder::new().disable_serde_phases()
  ```

### 12. specta `=2.0.0-rc.25` 升级后 tauri-specta 编译炸

**症状**：升级 `specta` 小版本（rc.25 → rc.26）后 `tauri-specta` 报奇怪的 type schema 错。

**原因**：specta v2 还在 rc，rc 版本之间 schema 二进制不兼容。

**修法**：

- **锁版本**：用 `=2.0.0-rc.25` 而不是 `^2.0.0-rc.25`
- 升级 specta 时必须**同步升级** tauri-specta，且确认两者的 rc 序号一致
- 不要混用不同 rc 序号的 specta / specta-typescript / tauri-specta

### 13. 前端 `commands.xxx()` 调用没补全，但 Rust 端有这个命令

**症状**：Rust 端写了 `#[tauri::command]` `#[specta::specta]` 命令，前端 IDE 看不到。

**原因**：

- bindings.ts 没刷新（没跑 export 或没启动 dev）
- 命令没加进 `collect_commands![]`
- bindings.ts 路径写错（`export("../src/bindings.ts")` 路径不对）

**修法**：

```bash
# 强制 export
cargo test --test specta_export -- --nocapture

# 检查 bindings.ts 文件是否真的更新
ls -l src/bindings.ts

# 检查 collect_commands! 是否包含
grep "fn_name" src-tauri/src/setup.rs
```

### 14. tauri-specta export 把生成时间写进 header，导致 CI git diff 永远不干净

**症状**：每次 export 都修改 bindings.ts 的时间戳，CI 红灯。

**原因**：自己在 `header()` 里写了 `Generated at {now}`。

**修法**：**header 必须确定性**——只写静态字符串：

```rust
// ❌
.header(format!("// Generated at {now}\n"))

// ✅
.header("// AUTO-GENERATED by tauri-specta. DO NOT EDIT.\n")
```

如果一定要带版本号，用 build 时 `env!("CARGO_PKG_VERSION")` 这种编译期确定的值，不要用运行时 `now`。

### 15. SeaORM Entity 直接 derive specta::Type 报错

**症状**：给 SeaORM 的 `Model` struct 加 `specta::Type` 编译失败。

**原因**：SeaORM `Model` 内部有非 specta-friendly 的关联类型（`PrimaryKey`、`Relation` 等）。

**修法**：**不要直接** derive Entity Model 的 Type。在 IPC 边界用独立的 DTO struct：

```rust
// Entity（DB 层）
#[derive(DeriveEntityModel)]
pub struct Model { ... }

// IPC 层 DTO（specta-friendly）
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: i32,
    pub name: String,
    // 只挑要给前端看的字段
}

impl From<Model> for UserDto {
    fn from(m: Model) -> Self { Self { id: m.id, name: m.name } }
}
```

### 16. `#[tauri::command]` 内部 `serde_json::Value` 字段 → BigInt panic

**症状**：返回类型含 `serde_json::Value` 且开了 `dangerously_cast_bigints_to_number`，运行时还是出问题。

**原因**：`serde_json::Value::Number` 内部可能装 i64。`dangerously_cast_bigints_to_number` 是**编译期标志**，runtime 实际数据如果超过 2^53 仍会精度丢失。

**修法**：

- 上游确保数据范围安全
- 或在前端把数值字段单独包成字符串：
  ```rust
  pub struct Item {
      #[serde(rename = "big_id_str")]
      pub big_id: String,    // 把 i64 转 String 再发
  }
  ```

## 调试小技巧

### 看 bindings.ts 没更新到哪里

跑 export 时加 `nocapture`：

```bash
cargo test --test specta_export -- --nocapture
```

会显示 export 调用的所有 warning（包括类型不能导出的具体路径）。

### 手动看某个 type 的 schema

```rust
#[test]
fn debug_type() {
    let mut types = specta::Types::default();
    types.register::<MyStruct>();
    dbg!(types);
}
```

### 看 specta 收集到的命令列表

```rust
let builder = specta_builder();
// builder.cfg.commands 含所有收集到的 Function
```

可以加个临时 `eprintln!` 打印命令名。

## 相关
- [setup.md](setup.md) — 装配阶段坑
- [types.md](types.md) — 类型 derive 坑
- [events.md](events.md) — 事件相关坑
- [export.md](export.md) — 导出阶段坑
