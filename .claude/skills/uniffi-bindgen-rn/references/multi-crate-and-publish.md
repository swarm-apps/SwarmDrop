# uniffi-bindgen-rn: 多 Crate 与发布

## 多 Crate 工作区

```
my-project/
├── Cargo.toml              # [workspace]
├── crates/
│   ├── core/               # 核心逻辑 (#[uniffi::export])
│   ├── network/            # 网络模块 (#[uniffi::export])
│   └── api/                # 统一入口 crate (re-export)
```

`ubrn.config.yaml` 指向入口 crate：

```yaml
rust:
  directory: ./rust_modules/my-project
  manifestPath: crates/api/Cargo.toml
```

入口 crate re-export 所有公开 API：

```rust
// crates/api/src/lib.rs
pub use core::*;
pub use network::*;
```

## 发布方式

### 预编译二进制（推荐）

npm 包包含编译好的 `.so` / `.xcframework`：

```json
{
  "files": [
    "android",    // jniLibs/*.so
    "build",      // *.xcframework
    "cpp",        // C++ 绑定
    "src",        // TypeScript
    "ios",
    "*.podspec"
  ]
}
```

验证：`npm pack --dry-run`

> 注意 .gitignore 和 npm files 的交互——.a 和 build/ 被 ignore 时需要在 files 中显式包含。

### 源码发布

让用户自己编译（包小但需要 Rust 工具链）：

```json
{
  "scripts": {
    "postinstall": "yarn ubrn:checkout && yarn ubrn:android --release && yarn ubrn:ios --release"
  }
}
```

### 发布检查清单

- [ ] 全架构 Release 编译通过
- [ ] iOS 真机 + 模拟器测试
- [ ] Android 4 架构 .so 齐全
- [ ] `npm pack --dry-run` 验证
- [ ] TypeScript 类型导出正确
