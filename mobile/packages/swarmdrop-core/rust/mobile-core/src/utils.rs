//! 跨模块共享的解析辅助。这里只放纯函数;有状态的 helper 放对应业务模块。

use std::path::PathBuf;

use swarmdrop_net::NodeId;

use crate::error::{FfiError, FfiResult};

pub(crate) fn parse_peer_id(value: &str) -> FfiResult<NodeId> {
    value
        .parse()
        .map_err(|error| FfiError::InvalidArgument(format!("invalid peer id: {error}")))
}

/// 把宿主传来的目录解析成文件系统路径。**这是唯一的解析点。**
///
/// expo 的 `Paths.*.uri` 形如 `file:///path/to/dir`，而 Rust 的 [`Path`] 要的是裸路径
/// ——`Path::new("file:///x")` 会得到一个以 `file:` 开头的无效路径。
///
/// 设计上刻意返回 [`PathBuf`] 而不是 `&str`：跨 FFI 进来的 `String` 是**未经解析的
/// 外部输入**，在边界一次性转成可信类型之后，内部就不必再问「这个字符串到底是 URI
/// 还是路径」。此前 `data_dir` 以 `String` 在内部流转，`device_config_path` 与
/// `open_db` 各自剥了一遍前缀——那是隐式契约，谁忘了谁踩坑（`logging` 差点成为第三个）。
///
/// 忘了会怎样：`Path::new("file:///x")` 得到的是一个名为 `file:` 的**相对**目录，
/// 写进去的东西下次启动读不回来。设备名那次的症状是「改了名字，重启又变回去」。
pub(crate) fn parse_host_dir(uri: &str) -> PathBuf {
    PathBuf::from(
        uri.strip_prefix("file://")
            .unwrap_or(uri)
            .trim_end_matches('/'),
    )
}

/// [`parse_host_dir`] 的反向：把内部路径转回宿主认识的 `file://` URI。
///
/// 边界转换成对出现——宿主的世界是 URI（`expo-file-system` 的 `Paths.*`、
/// `Sharing.shareAsync` 都只认它），Rust 内部的世界是 [`Path`]。跨 FFI 出去时转回去，
/// 调用方就不必知道我们内部存的是什么形状。
///
/// [`Path`]: std::path::Path
pub(crate) fn to_host_uri(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 出入成对：解析进来再转回去，应当还原成规范形式。
    #[test]
    fn host_uri_round_trips() {
        let uri = "file:///var/app/logs/swarmdrop.log";
        assert_eq!(to_host_uri(&parse_host_dir(uri)), uri);
    }

    #[test]
    fn strips_file_scheme_and_trailing_slash() {
        assert_eq!(
            parse_host_dir("file:///var/app/docs/"),
            PathBuf::from("/var/app/docs")
        );
        assert_eq!(
            parse_host_dir("file:///var/app/docs"),
            PathBuf::from("/var/app/docs")
        );
    }

    /// 已经是裸路径时原样返回——调用方不必先判断格式，这正是「边界解析」的意义。
    #[test]
    fn leaves_plain_paths_untouched() {
        assert_eq!(
            parse_host_dir("/var/app/docs"),
            PathBuf::from("/var/app/docs")
        );
        assert_eq!(
            parse_host_dir("/var/app/docs/"),
            PathBuf::from("/var/app/docs")
        );
    }
}
