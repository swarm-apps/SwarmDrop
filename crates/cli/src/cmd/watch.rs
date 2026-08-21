//! `watch`：订阅本机发生的事件，推给调用它的程序。
//!
//! ## 它与 `transfer watch` 是两件事
//!
//! 判据一句话：**面板服务于「此刻我想看一眼」，订阅服务于「有变化就告诉我」**。
//! 完整对照表在 [`crate::runtime::watch`]。
//!
//! ⚠️ 两条流的语义因此正好相反，别把其中一条的写法搬到另一条上：面板每秒重绘**全量
//! 快照**、画在 stderr（它是过程信息）；本命令只在**有变化时**推一条增量、写在 stdout
//! （它就是命令的结果）。
//!
//! ## 它不启动节点，也不因为没有节点而失败
//!
//! 按「这条命令会不会导致一个数据包离开本机」那句问句——不会，它只观察。没有常驻节点时
//! 它照样建立订阅：先给一条直读本机记录拼出来的基线，然后等节点出现；节点关停后继续等。
//!
//! 这让调用方**不必关心自己与节点的启动顺序**。少了这一条，宿主（agent harness 的插件）
//! 就得自己写一套重试与竞态处理，而那正是这条命令该替它做的事。
//!
//! 取数、重连与发号都在 [`crate::runtime::watch::client`]——本模块只把参数交进去，
//! 再把出来的每一行交给渲染。

use crate::adapter::paths::DataDir;
use crate::exit::CliResult;
use crate::render::stream;
use crate::runtime::watch::client::Subscription;

pub async fn run(data_dir: &DataDir, json: bool, inbox_limit: u32) -> CliResult<()> {
    Subscription::new(data_dir, inbox_limit)
        .run(|line| stream::write(&line, json))
        .await
}
