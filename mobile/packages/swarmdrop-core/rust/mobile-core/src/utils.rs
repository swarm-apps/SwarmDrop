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
///
/// **必须 percent-decode。** expo 交出来的 URI 是编码过的：一个名叫 `我的 下载` 的
/// 目录会是 `file:///.../%E6%88%91%E7%9A%84%20%E4%B8%8B%E8%BD%BD`。不解码就会得到一个
/// 字面含 `%E6%88%91` 的路径，`create_dir_all` 会**新建**那个怪名字的目录，
/// 用户选的那个反而一个文件都收不到。
pub(crate) fn parse_host_dir(uri: &str) -> PathBuf {
    let raw = uri
        .strip_prefix("file://")
        .unwrap_or(uri)
        .trim_end_matches('/');
    PathBuf::from(
        percent_encoding::percent_decode_str(raw)
            .decode_utf8_lossy()
            .as_ref(),
    )
}

/// `file://` 路径段里必须被编码的字符集。
///
/// **不含 `/`**——它是分隔符，编码掉就把整条路径压成了一个段。`%` 必须在里面，
/// 否则一个真叫 `50% off.pdf` 的文件会编出无法二次解析的串。
const PATH_ENCODE_SET: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'%');

/// [`parse_host_dir`] 的反向：把内部路径转回宿主认识的 `file://` URI。
///
/// 边界转换成对出现——宿主的世界是 URI（`expo-file-system` 的 `Paths.*`、
/// `Sharing.shareAsync` 都只认它），Rust 内部的世界是 [`Path`]。跨 FFI 出去时转回去，
/// 调用方就不必知道我们内部存的是什么形状。
///
/// **必须 percent-encode**，理由与 [`parse_host_dir`] 对称：这个串会落进
/// `transfer_files.local_path`，随后被三端当 URI 用——iOS `Sharing.shareAsync` 走
/// `URL(string:)`（遇到未编码的空格返回 nil）、Android `new File(uri)` 走
/// `URI.create`（遇到非法字符抛 `IllegalArgumentException`）、RN `<Image source={{uri}}>`
/// 同理。此前这里直接拼原始路径，而旧的 JS 实现返回的是 expo 编码过的 `file.uri`，
/// 于是「接收一个名字里带空格或中文的文件」之后，打开 / 分享 / 预览会全线失效。
///
/// [`Path`]: std::path::Path
pub(crate) fn to_host_uri(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    let encoded = percent_encoding::utf8_percent_encode(&raw, PATH_ENCODE_SET);
    format!("file://{encoded}")
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

    /// 含空格 / 非 ASCII 的名字必须能原样往返——这是「打开 / 分享 / 删除」的前提。
    #[test]
    fn percent_encoding_round_trips_for_spaces_and_cjk() {
        let path = PathBuf::from("/var/app/transfers/我的 报告 (1).pdf");
        let uri = to_host_uri(&path);
        assert!(
            uri.contains("%20") && !uri.contains(' '),
            "空格必须被编码，实际: {uri}"
        );
        assert_eq!(parse_host_dir(&uri), path, "编码后必须能解回原路径");
    }

    /// 名字里真的含 `%` 时不能二次解析成别的字符。
    #[test]
    fn literal_percent_survives_round_trip() {
        let path = PathBuf::from("/var/app/transfers/50% off.pdf");
        assert_eq!(parse_host_dir(&to_host_uri(&path)), path);
    }

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
