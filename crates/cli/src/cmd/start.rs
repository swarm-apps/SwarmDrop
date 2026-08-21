//! `start`：启动常驻节点。

use std::sync::Arc;

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::boot::boot;
use crate::runtime::daemon::Daemon;
use crate::runtime::single::{Acquisition, acquire};

/// 后台启动时，等待节点就绪的上限。
///
/// 超时不代表失败——子进程可能只是起得慢。所以超时只影响「要不要打印就绪信息」，
/// 不影响退出码：把一次慢启动报成失败，会诱导用户重复执行，反而撞上单实例拒绝。
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// 后台启动时，留给「有没有新版本」那次查询的上限。
///
/// 它**必须有上限且很短**：父进程做完这件事就退出，用户正在终端前等 `start -d` 返回。
/// 检查更新是搭便车的，永远排在启动之后——超时就当没查过（时间戳已经记下，
/// 下一次启动不会立刻重试）。
const HINT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

pub async fn run(data_dir: &DataDir, json: bool, detach: bool, auto_accept: bool) -> CliResult<()> {
    if detach {
        return spawn_detached(data_dir, json, auto_accept).await;
    }

    let Acquisition::Owner(lock) = acquire(data_dir).await? else {
        return Err(CliError::NodeUnavailable(
            "节点已在运行；如需停止请执行 swarmdrop stop".into(),
        ));
    };

    let node = Arc::new(boot(data_dir, json).await?);
    // 常驻形态的装配（本地通道 + 被动接收 + 配对确认台）收在 `runtime::daemon`——
    // `swarmdrop mcp` 自持节点时走的是同一条，理由见那个模块的文档。
    let daemon = Daemon::start(data_dir, node.clone(), auto_accept)?;

    crate::render::status::render_started(&node.node_id, json);
    if !json {
        println!("接收落点  {}", daemon.save_dir().display());
        // 把配对的门开在哪儿说清楚。不说的话，用户在别处扫码被拒时无从判断
        // 是网络不通还是被本机挡了。
        if auto_accept {
            println!("配对策略  自动接受邀请配对（--auto-accept）");
        } else {
            println!("配对策略  需要确认——在本机执行 swarmdrop invite create 期间才接受配对");
        }
    }

    // 顺带看一眼有没有新版本。**前台必须 spawn 而不能 await**：紧接着就是服务循环，
    // 在这里等一次网络往返等于让节点晚几百毫秒开始接受连接。进程长期存活，任务跑得完。
    if crate::runtime::update::should_check(data_dir, json) {
        let data_dir = data_dir.clone();
        tokio::spawn(async move { crate::cmd::update::hint_if_outdated(&data_dir).await });
    }

    daemon.serve().await?;

    node.manager.shutdown().await;
    drop(lock); // 显式：持有权必须在节点关停之后释放
    Ok(())
}

/// 以后台方式启动：重新拉起自己（不带后台标志），父进程在节点就绪后返回。
///
/// **不做 daemon 化的那套仪式**（fork/setsid/重定向）：那些在 Windows 上完全不同，
/// 而真正需要脱离终端的场景（开机自启）本就该交给服务管理器。这里只解决交互式用户的
/// 便利：拉起一个子进程，自己退出。
async fn spawn_detached(data_dir: &DataDir, json: bool, auto_accept: bool) -> CliResult<()> {
    if crate::runtime::ipc::is_alive(&data_dir.socket()).await {
        return Err(CliError::NodeUnavailable(
            "节点已在运行；如需停止请执行 swarmdrop stop".into(),
        ));
    }

    let exe = std::env::current_exe()
        .map_err(|err| CliError::NodeUnavailable(format!("无法定位自身可执行文件: {err}")))?;

    let mut cmd = std::process::Command::new(exe);
    cmd.arg("start")
        .arg("--data-dir")
        .arg(data_dir.path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        // **子进程不查更新，由父进程查。** 它的 stderr 是 null，那行提示没有人看得到；
        // 更要紧的是它会**抢先记下时间戳**，把父进程那次挤进节流窗口——于是提示既没显示、
        // 又要等满一个间隔才有下次机会。复用现成的开关而不新增一个隐藏参数。
        .env(crate::runtime::update::NO_CHECK_ENV, "1");
    if json {
        cmd.arg("--json");
    }
    if auto_accept {
        cmd.arg("--auto-accept");
    }

    cmd.spawn()
        .map_err(|err| CliError::NodeUnavailable(format!("后台启动失败: {err}")))?;

    // 轮询通道，等它把服务端建起来。
    let deadline = std::time::Instant::now() + READY_TIMEOUT;
    while std::time::Instant::now() < deadline {
        if crate::runtime::ipc::is_alive(&data_dir.socket()).await {
            crate::render::status::render_detached(true, json);
            // **这里只能 await**：父进程马上退出，spawn 出去的任务会随它一起消失。
            // 与前台那处的写法不同，两边各有各的理由，别统一。
            if crate::runtime::update::should_check(data_dir, json) {
                let _ = tokio::time::timeout(
                    HINT_TIMEOUT,
                    crate::cmd::update::hint_if_outdated(data_dir),
                )
                .await;
            }
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    crate::render::status::render_detached(false, json);
    Ok(())
}
