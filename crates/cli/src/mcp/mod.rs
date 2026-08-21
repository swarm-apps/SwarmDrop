//! MCP 宿主：把 SwarmDrop 的能力以 MCP stdio server 暴露给 agent harness。
//!
//! 分层与 [`crate`] 的整体约定一致，另加一条本模块特有的：
//!
//! ```text
//! backend.rs  能力端口（平台中立，将来整体上移到共享 crate）
//! tools.rs    工具的 schema 与分派（只依赖端口）
//! host.rs     端口的 CLI 实现（持节点、走三档取数入口）
//! ```
//!
//! **`tools.rs` 不得引用 `host.rs`**。那条边一旦连上，工具面就绑死在这个宿主上，
//! 而本模块存在的理由正是让它别绑死（见 [`backend`] 的模块文档）。

pub mod backend;
pub mod host;
pub mod tools;

use std::sync::Arc;

use rmcp::ServiceExt;

use crate::exit::{CliError, CliResult};

/// 在标准输入输出上跑 MCP server，直到宿主关闭 stdin 或本进程被终止。
///
/// **不打印任何东西到 stdout**：那条流整个归 MCP 协议（spec: `cli-mcp-host` 的
/// 「stdio 传输与 stdout 纯净」）。日志由 `main` 装到 stderr，本函数不另开输出。
pub async fn serve(backend: Arc<dyn backend::ToolBackend>) -> CliResult<()> {
    let service = tools::ToolHost::new(backend)
        .serve(rmcp::transport::io::stdio())
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("MCP server 启动失败: {err}")))?;

    // `waiting` 在宿主关闭 stdin（或连接出错）时返回。**正常收摊不是失败**——宿主结束
    // 子进程是它的常规动作，报错会让它把一次正常退出记成崩溃并触发重启。
    service
        .waiting()
        .await
        .map_err(|err| CliError::NodeUnavailable(format!("MCP server 意外结束: {err}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    /// **MCP server 不得打开配对窗口**（spec: `cli-mcp-host` 的「配对窗口不因 MCP 而
    /// 打开」）。
    ///
    /// 常驻节点判断「有没有人在等配对」的唯一依据，是有没有客户端在轮询
    /// [`crate::runtime::ipc::Request::PairWaitNext`]。因此这条要求的实现是**不做某件事**
    /// ——而「不做」没有任何代码可以指着说「看，它在这里」，下一个人顺手加一句轮询
    /// （比如为了让 `swarmdrop mcp` 也能接受配对）不会碰到任何阻力。
    ///
    /// 破坏它的后果是**静默**且不可逆的：邀请是一次性凭证，被抢先用掉那次就消耗了，
    /// 真正的设备再来就配不上——而 server 后面根本没有人在看屏幕。
    ///
    /// 只检查代码行，注释里提它是允许的（本模块与 `cmd/mcp.rs` 的文档都会提到它）。
    #[test]
    fn the_mcp_host_never_polls_for_inbound_pairing() {
        // 拼出来而不是写成字面量：本文件也在被扫描的清单里，写成字面量会让这条测试
        // 扫到自己。
        let needle = concat!("PairWait", "Next");

        let sources = [
            ("cmd/mcp.rs", include_str!("../cmd/mcp.rs")),
            ("mcp/mod.rs", include_str!("mod.rs")),
            ("mcp/host.rs", include_str!("host.rs")),
            ("mcp/tools.rs", include_str!("tools.rs")),
            ("mcp/backend.rs", include_str!("backend.rs")),
        ];

        for (name, src) in sources {
            let offending = src
                .lines()
                .map(str::trim)
                .filter(|line| !line.starts_with("//"))
                .find(|line| line.contains(needle));

            assert!(
                offending.is_none(),
                "{name} 轮询了 {needle}，那会让这个 MCP server 变成一扇没人看守的配对窗口：\n  {}\n\
                 见本测试的文档注释。",
                offending.unwrap_or_default()
            );
        }
    }
}
