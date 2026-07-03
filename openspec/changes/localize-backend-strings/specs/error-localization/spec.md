## ADDED Requirements

### Requirement: 用户可读错误文案由前端按稳定 kind 生成

系统 SHALL 保证一切经 IPC 返回前端的错误携带一个稳定、语言无关的 `kind` 判别码；用户可读文案 SHALL 由前端依据 `kind`（及可用的结构化参数）经 Lingui 生成，而非直接展示后端返回的 `message` 字段。后端 `message` SHALL 仅作为开发者/日志用的技术细节。

#### Scenario: 前端按 kind 渲染当前语言文案

- **WHEN** 后端命令返回错误 `{ kind: "NodeNotStarted", message: "Node not started" }`
- **THEN** 前端根据 `kind` 查 Lingui 目录，渲染当前 locale 下的用户可读文案（如中文「节点未启动」、English "Node not started"）
- **AND** 不把后端 `message` 原文直接展示给用户

#### Scenario: 未知 kind 回退

- **WHEN** 错误的 `kind` 未在前端映射表中登记
- **THEN** 前端回退到通用「出错了，请重试」文案
- **AND** 原始 `message` 仅保留在可展开详情或日志中

### Requirement: 内部/技术类错误归为通用提示

前端 SHALL 把内部或技术类错误（`Io` / `Serialization` / `Database` / `TaskJoin` / `P2p` / `Tauri`）归为统一的通用提示，而非逐条翻译；其技术细节 SHALL 仅在可展开详情或日志中呈现。

#### Scenario: 技术错误只给通用提示

- **WHEN** 命令因数据库或 IO 故障返回 `kind: "Database"` / `"Io"`
- **THEN** 用户看到通用「出错了，请重试」提示
- **AND** 具体技术信息不作为主文案直接抛给用户

### Requirement: 后端不返回预翻译的用户散文

后端错误 MUST NOT 内嵌某一语言的用户可读散文作为其权威文案；语义完全由 `kind` 承载。既有越界的中文错误变体（配对码过期 / 无效）SHALL 改为仅靠 `kind` 表达。

#### Scenario: 配对码错误不再自带中文

- **WHEN** 配对码过期或无效
- **THEN** 后端返回 `kind: "ExpiredCode"` / `"InvalidCode"`，其 `message` 为语言无关的技术描述
- **AND** 用户看到的过期/无效提示由前端按当前 locale 渲染
