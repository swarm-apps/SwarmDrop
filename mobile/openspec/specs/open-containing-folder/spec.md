# open-containing-folder Specification

## Purpose
「打开文件夹」定位到收到内容真实所在的容器目录(而非配置的存储根),以 host `finalize_sink` 返回的文件父目录 URI 为事实源,SAF 与 file:// 皆正确;覆盖传输详情页与收件箱详情页。

## Requirements
### Requirement: 打开文件夹定位到真实容器目录

「打开文件夹」动作 SHALL 打开收到内容真实所在的容器目录,而非配置的存储根目录。容器目录以 host 侧 `finalize_sink` 返回的**文件父目录 URI** 为唯一事实源,MUST NOT 由「保存目录 + 相对路径」字符串拼接推导得出(SAF `content://` document URI 与重名改写下拼接必然失真)。此要求同时适用于传输详情页与收件箱详情页。

#### Scenario: 收到文件夹(文件落在子目录)后打开文件夹

- **WHEN** 一次已完成接收的内容含目录结构(文件真实落盘于 `<存储根>/<子目录>/…`),用户在传输详情页或收件箱详情页点「打开文件夹」
- **THEN** 系统文件管理器 SHALL 定位到文件真实所在的 `<存储根>/<子目录>`,而非 `<存储根>`

#### Scenario: 收到平铺单文件后打开文件夹

- **WHEN** 一次已完成接收只含落在存储根下的平铺文件(父目录即存储根),用户点「打开文件夹」
- **THEN** 系统 SHALL 打开该父目录(此情形恰为存储根),行为与旧版一致

#### Scenario: 内容跨多个顶层目录

- **WHEN** 一次已完成接收的文件分布在多个不同的父目录下(不存在唯一共同容器目录)
- **THEN** 系统 SHALL 回退到打开配置的存储根目录

### Requirement: 容器目录事实源贯穿并落库

`FileAccess::finalize_sink` SHALL 返回文件的最终 URI 及其**父目录 URI**;core SHALL 将该父目录 URI 落库到 `transfer_files.local_dir`。容器目录 `content_root_of(files, save_path)` SHALL **在 core 内解析成可直接打开的目录**:所有文件 `local_dir` 唯一一致 → 该目录;否则(跨多个不同父目录 / 缺 `local_dir` 的历史)→ 回退存储根 `save_path`。传输投影 `content_root` 与收件箱 `root_path` 均取此解析结果,消费方直读、不再各自兜底,MUST NOT 从相对路径拼接推导。

#### Scenario: 接收完成时记录父目录

- **WHEN** 接收方成功 finalize 一个文件
- **THEN** core SHALL 把 host 返回的父目录 URI 写入该文件的 `local_dir` 列

#### Scenario: 唯一容器 → 定位到该容器

- **WHEN** 一次已完成接收的所有文件 `local_dir` 相同(单文件 / 平铺同一文件夹)
- **THEN** 传输投影 `content_root` 与收件箱 `root_path` SHALL 均为该目录

#### Scenario: 跨多目录 / 缺 local_dir → core 回退存储根

- **WHEN** 一次已完成接收的文件分布在多个不同父目录(无唯一容器),或文件 `local_dir` 为 NULL(旧版本落库)
- **THEN** `content_root_of` SHALL 在 core 内回退到会话保存目录,传输投影 `content_root` 与收件箱 `root_path` 均为存储根;不报错、不做相对路径推导

