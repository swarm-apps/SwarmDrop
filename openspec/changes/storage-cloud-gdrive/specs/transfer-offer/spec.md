# Spec Delta

## MODIFIED Requirements

### Requirement: 前端接收页展示文件树

接收页 SHALL 使用 `FileTree` 组件展示 Offer 中的文件列表，并允许接收方在本地目录与已连接的云账户之间选择本次接收目的地。

#### Scenario: 从 TransferFileInfo 构建树数据

- **WHEN** 接收页需要展示文件列表
- **THEN** MUST 调用 `buildTreeDataFromOffer(files: TransferFileInfo[])` 将 flat list 转为 `TreeData { dataLoader, rootChildren }`，使用 `relativePath` 重建目录层级

#### Scenario: 复用 FileTree 组件

- **WHEN** 渲染文件树
- **THEN** MUST 使用 `<FileTree mode="select">` 组件，传入 `dataLoader`、`rootChildren`、`totalCount`、`totalSize`，不传 `onRemoveFile`（接收方不能删除文件）

#### Scenario: 保留保存路径选择功能

- **WHEN** 用户在接收页查看文件树并确认接收
- **THEN** 目的地选择器 MUST 允许选择本地目录；存在状态正常的已连接云账户时，MUST 同时允许选择该账户，并将所选目的地传给后端
