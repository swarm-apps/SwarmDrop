## ADDED Requirements

### Requirement: 前端为 locale 权威源并同步给后端

前端 SHALL 是应用当前语言的唯一权威源（持久化于偏好存储）。桌面壳 SHALL 在启动创建托盘 / 发通知前取得当前 locale，并 SHALL 在用户切换语言时经命令收到更新。

#### Scenario: 启动读取持久化 locale

- **WHEN** 应用冷启动、托盘尚未创建
- **THEN** 桌面壳从持久化偏好读取当前 locale，作为托盘与通知的初始语言
- **AND** 读取失败时回退到源 locale（`zh`）而非崩溃

#### Scenario: 语言切换推送到后端

- **WHEN** 用户在设置中切换语言
- **THEN** 前端调用 `set_locale` 命令把新 locale 推给后端
- **AND** 后端更新其全局 locale，此后产生的原生字符串使用新语言

### Requirement: 原生 OS 字符串由 Rust 侧目录本地化

托盘菜单文案与系统通知的标题 / 正文 SHALL 由桌面壳侧的翻译目录按当前 locale 产生，覆盖前端支持的全部 locale（当前 `zh` / `zh-TW` / `en`）。

#### Scenario: 通知按当前语言呈现

- **WHEN** 需要弹出一条系统通知（如配对请求、收到文件传输请求）
- **THEN** 通知标题与正文使用后端当前 locale 对应的文案

#### Scenario: locale 缺项回退

- **WHEN** 某条字符串在当前 locale 目录缺失
- **THEN** 回退到源 locale（`zh`）文案，而非展示裸键名

### Requirement: core 通过语义通知类型保持语言中立

core MUST NOT 构造任何语言的通知散文。core SHALL 以携带结构化字段的语义通知类型表达通知意图，由 host 在展示时翻译为当前语言。

#### Scenario: core 只发语义类型

- **WHEN** core 需要触发一条系统通知
- **THEN** 它构造语义通知类型（如 `PairingRequest { hostname }`），不含任何标题 / 正文散文
- **AND** host 侧据此翻译出当前 locale 的标题与正文

#### Scenario: 多 host 复用同一语义类型

- **WHEN** 另一 host（如 SwarmDrop-RN 移动端）复用同一 core
- **THEN** 它接收同一语义通知类型并用自身本地化层翻译，无需改动 core
