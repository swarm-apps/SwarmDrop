# core-wasm-ready 设计决策

## 决策记录（2026-07-19，与用户逐项确认）

### D1：sea-orm 摘出方式 = 独立 crate（否决 feature 门控）

`crates/storage-sql` 装下 `SqlSessionStore + ops + inbox`。理由：core = 业务 + 端口，
实现归实现——与 `crates/{net,transfer,host}` 的拆分风格一致，依赖图彻底干净（core 零
sea-orm，而非「有但被 cfg 藏着」）。feature 门控（entity 的 `sqlite` feature 先例）改动
更小，但 Sql 实现住在 core 里层次上欠，被否决。

**依赖方向（已核实，storage-sql 不需要依赖 core）**：

```
                    ┌──────────────┐
                    │    entity    │  (sea-orm 已 feature 解绑, wasm 可编)
                    └──────┬───────┘
          ┌────────────────┼───────────────┐
          ▼                ▼               ▼
   ┌────────────┐   ┌─────────────┐  ┌───────────────┐
   │  swarmdrop │   │  swarmdrop  │  │  storage-sql  │ ← 新
   │  -transfer │◀──│    -core    │  │ (SqlSession-  │
   │ (Session-  │   │ (零 sea-orm) │  │  Store 实现)   │
   │  Store 端口)│   └──────┬──────┘  └───┬───────────┘
   └─────▲──────┘          │             │ 依赖 transfer(trait)
         │                 │             │ + entity + host + sea-orm
         └─────────────────┼─────────────┘
                           ▼
              src-tauri / mobile-core
              （组装点：core 业务 + storage-sql 注入 Arc<dyn SessionStore>）
```

`ops.rs` 里的 `crate::transfer::*` 引用实际指向 `swarmdrop-transfer` 的 re-export
（core 无本地 transfer 模块），搬移时改直接引用即可；`TransferProjection`/
`CreateSessionInput`/`TransferState` 等类型均已住在 transfer，无需回依 core。

### D2：tokio → n0-future 并入本 change

DoD 定为「core 进 check-wasm 双 target 常绿」——只摘 sea-orm 不迁 tokio 则 wasm 仍编不过，
change 价值不闭环。迁移口径与 transfer/net 一致（iroh-migration.md 的既定方案）：

- 换：`tokio::spawn` → `n0_future::task::spawn`、`tokio::time::*` → `n0_future::time::*`、
  `Instant` → `n0_future::time::Instant`
- 不换：`tokio::sync::*`（mpsc/oneshot/watch/Mutex）与 `select!` ——纯用户态原语，wasm 可用
- ⚠️ n0-future 非 wasm 侧无条件开 tokio `test-util` feature，经 unification 传染全构建
  （libp2p-wasm.md 已记录）——迁移后核对 `cargo tree -e features` 无意外膨胀

已知同类坑（迁移时顺带自查）：wasm 的 `Instant` 原点是页面加载，`Instant - Duration`
开局下溢 panic（门 5）——core 的 4 个文件里若有同模式一并 `checked_sub` 化。

### D3：Web 消费 core 另立 change

本 change 到「core wasm 可编 + 桌面/移动回归绿」为止。Web 接入（删 `share_code.rs` 重复、
`WebPeerDirectory` 换 core 配对、IndexedDB SessionStore、web crate 瘦身为「host 实现 +
WebNode 薄壳」）依赖 React UI 工程的配对/持久化产品决策，现在做会返工。

### D4：不做的事（讨论中已否决的方向，防止重提）

- **「JS 实现 host + 删 web crate」**：JS→Rust trait 的适配胶水必须是 Rust 代码且依赖
  wasm-bindgen——放 core 污染平台中立、放独立 crate 即 web crate 重生；热路径
  （write_sink_chunk 每 chunk）多一层完整跨界序列化。web crate 是三端对称中 Web 那一格
  （桌面 src-tauri / 移动 mobile-core / Web crates/web），不删。
- **「core 直接 wasm-pack 导出」**：wasm-pack 需要 `#[wasm_bindgen]` 注解层（JsValue
  转换/错误映射/Promise 化），塞进 core 则 Web 暴露层进业务核心，三端不对称。暴露层
  留在 web crate（WebNode）。

## 参考

- `dev-notes/knowledge/storage-abstraction.md`——切割线/SendWrapper/耦合面（trait 层已落地段）
- `dev-notes/knowledge/iroh-migration.md`——n0-future 迁移细则
- `dev-notes/knowledge/libp2p-wasm.md`——五道运行时门、test-util 传染警告
