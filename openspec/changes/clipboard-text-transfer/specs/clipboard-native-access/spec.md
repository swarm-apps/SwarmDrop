## ADDED Requirements

### Requirement: 桌面文本发送与复制经显式原生剪贴板操作

桌面端在文本发送页读取剪贴板以及在文本收件箱详情复制正文 SHALL 复用既有原生剪贴板封装，并且每次读取或写入 SHALL 由用户明确触发。应用 SHALL NOT 为文本投递注册持续监听、窗口激活自动读取或接收后自动写入行为。

#### Scenario: 桌面用户在发送页粘贴

- **WHEN** 用户明确点击文本编辑器的粘贴操作
- **THEN** 桌面端 SHALL 经原生剪贴板封装读取纯文本并填入编辑器
- **AND** 桌面端 SHALL NOT 直接调用 `navigator.clipboard`

#### Scenario: 桌面用户复制收到的文本

- **WHEN** 用户明确选择复制一条收到的文本
- **THEN** 桌面端 SHALL 经原生剪贴板封装写入完整正文并给出反馈
- **AND** 打开条目、接收通知或切换窗口 SHALL NOT 触发写入
