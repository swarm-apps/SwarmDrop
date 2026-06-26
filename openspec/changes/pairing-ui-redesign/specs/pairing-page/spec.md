## ADDED Requirements

### Requirement: 移动端配对全屏页面

移动端配对 UI SHALL 为全屏页面（覆盖设备列表和底部导航栏），包含 Header、Tab 切换栏和内容区域。

#### Scenario: 进入配对页面
- **WHEN** 用户在移动端点击 Header 的 ＋ 按钮
- **THEN** 显示全屏配对页面，默认选中"生成配对码" Tab，底部导航栏隐藏

#### Scenario: 配对页面 Header
- **WHEN** 配对全屏页面显示时
- **THEN** Header 左侧显示 ← 箭头 + "添加设备"（18px/600），右侧显示 X 关闭按钮

#### Scenario: 点击返回或关闭
- **WHEN** 用户点击 ← 或 X 按钮
- **THEN** 关闭配对页面，返回设备列表，底部导航栏恢复显示

### Requirement: 移动端 Tab 切换

移动端配对页面 SHALL 包含"生成配对码"和"输入配对码"两个 Tab，可自由切换。

#### Scenario: Tab 切换外观
- **WHEN** 配对页面显示时
- **THEN** Tab 栏为圆角灰色背景（bg-muted），活跃 Tab 为白色背景 + 阴影 + 前景色文字，非活跃 Tab 为灰色文字

#### Scenario: 切换到生成配对码 Tab
- **WHEN** 用户点击"生成配对码" Tab
- **THEN** 内容区显示生成码视图，调用 `generateCode()` 生成新码

#### Scenario: 切换到输入配对码 Tab
- **WHEN** 用户点击"输入配对码" Tab
- **THEN** 内容区显示输入码视图，重置之前的 store 状态

### Requirement: 移动端生成码视图

生成配对码 Tab 内容 SHALL 包含图标、说明文字、6 位配对码展示、倒计时和重新生成按钮。

#### Scenario: 生成码布局
- **WHEN** "生成配对码" Tab 激活
- **THEN** 居中显示：Link 图标（蓝色，64px 圆形背景 blue/8%）→ 说明文字"在另一台设备上输入此配对码"（14px muted）→ 6 位数字（3-分隔符-3，每位 48x60 圆角方块 bg-muted，数字 28px/700）→ 倒计时（Clock 图标 + "配对码将在 m:ss 后过期"）→ 蓝色全宽按钮"重新生成"（RefreshCw 图标）

#### Scenario: 倒计时结束
- **WHEN** 配对码过期
- **THEN** 倒计时文字变为"配对码已过期"，重新生成按钮保持可用

### Requirement: 移动端输入码视图

输入配对码 Tab 内容 SHALL 包含图标、说明文字、OTP 输入框和查找设备按钮。

#### Scenario: 输入码布局
- **WHEN** "输入配对码" Tab 激活
- **THEN** 居中显示：Keyboard 图标（蓝色，64px 圆形背景 blue/8%）→ 说明文字"输入另一台设备上的配对码"（14px muted）→ 6 位 OTP 输入框（3-分隔符-3，每位 44x56 圆角方块，已填充位蓝色边框，空位灰色边框）→ 蓝色全宽按钮"查找设备"（Search 图标）

#### Scenario: 输入完成自动查找
- **WHEN** 用户输入完 6 位数字
- **THEN** 自动触发 `searchDevice(code)` 查找设备

### Requirement: 桌面端生成码 Dialog

桌面端生成配对码 SHALL 使用 Dialog 弹窗，布局对齐设计稿。

#### Scenario: Dialog 布局
- **WHEN** 桌面端用户点击"生成配对码"
- **THEN** 弹出 Dialog（400px 宽），包含：Link 图标（蓝色，64px 圆形背景 blue-50）→ 标题"添加新设备"（20px/600）→ 描述"在另一台设备上输入以下配对码"（14px muted）→ 6 位数字（3-分隔符-3，每位 48x56 圆角方块 bg-muted，数字 24px/600）→ 过期提示 → 底部两按钮：左"取消"（outline），右"复制配对码"（蓝色，Copy 图标）

#### Scenario: 复制后反馈
- **WHEN** 用户点击"复制配对码"
- **THEN** 配对码复制到剪贴板，按钮文字变为"已复制"（Check 图标），2 秒后恢复

### Requirement: 桌面端输入码全屏页面

桌面端输入配对码 SHALL 为全屏内容页面（替代当前的 Dialog），显示在 sidebar 旁的主内容区。

#### Scenario: 输入码页面布局
- **WHEN** 桌面端用户点击"输入配对码"
- **THEN** 主内容区切换为输入码页面：Toolbar 显示 `← 连接已有设备`（带返回箭头）；内容区居中显示：Link 图标（蓝色，64px 圆形背景 blue-50）→ 标题"连接已有设备"（20px/600）→ 描述"输入另一台设备上显示的配对码"（14px muted）→ 6 位 OTP 输入框（3-分隔符-3，每位 48x56）→ 底部两按钮：左"取消"（outline），右"确认"（蓝色）

#### Scenario: 点击返回
- **WHEN** 用户点击 ← 返回按钮 或 "取消"
- **THEN** 返回设备列表页面

### Requirement: 入口行为调整

移动端和桌面端的配对入口行为 SHALL 根据平台差异化处理。

#### Scenario: 移动端 ＋ 按钮
- **WHEN** 用户在移动端点击 Header 的 ＋ 按钮
- **THEN** 直接进入全屏配对页面（不显示下拉菜单）

#### Scenario: 桌面端添加设备菜单
- **WHEN** 用户在桌面端点击"连接设备"按钮
- **THEN** 显示下拉菜单："生成配对码"打开 Dialog，"输入配对码"进入全屏页面
