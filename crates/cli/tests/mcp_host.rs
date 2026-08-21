//! `swarmdrop mcp` 自持节点时，必须把它摆成**常驻形态**。
//!
//! 这条只有真的把进程跑起来才看得见：它握着单实例锁数小时，若不服务本地通道，
//! 同机的每一条命令都会一路重试到「另一个 swarmdrop 进程正在启动，等待超时」，
//! `swarmdrop watch` 的判活永远为假，而那个节点在线、可达、却一个文件都收不下。
//! 三种失败没有一种会报错，也没有一种能在单元测试里显形。
//!
//! ⚠️ **本文件会真的启动一个 P2P 节点**（`tests/without_a_node.rs` 里的用例全都不会）。
//! 判据取「通道文件出现了没有」而不是计时：节点装配的耗时随机器与网络波动，
//! 而文件在不在是确定的。

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// 等节点起来的上限。给得宽是因为它真的要装配一个节点（身份、证书、数据库、监听器），
/// 冷编译后的第一次尤其慢；超时只会让这条用例失败，不会让它假绿。
const READY_TIMEOUT: Duration = Duration::from_secs(60);

fn spawn_mcp(dir: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_swarmdrop"))
        .arg("mcp")
        .arg("--data-dir")
        .arg(dir)
        // stdin 接管道并**持有**它：MCP server 在 stdin 关闭时正常收摊，
        // 接 null 的话它会立刻退出，这条用例就什么都没测到。
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("执行 swarmdrop mcp")
}

/// 本地通道的路径（与 `adapter::paths` 的约定一致）。
#[cfg(unix)]
fn socket_path(dir: &Path) -> std::path::PathBuf {
    dir.join("swarmdrop.sock")
}

/// 等本地通道出现；返回它出现了没有。
#[cfg(unix)]
fn wait_for_channel(dir: &Path) -> bool {
    let socket = socket_path(dir);
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline && !socket.exists() {
        std::thread::sleep(Duration::from_millis(200));
    }
    socket.exists()
}

/// **自持节点的 MCP server 必须服务本地通道。**
///
/// 不服务的表现全是静默的（见本文件的模块文档），所以判据放在进程外：通道文件出现，
/// 且此时 `swarmdrop status` 认得出这个节点在跑。
#[cfg(unix)]
#[test]
fn a_self_hosted_mcp_server_serves_the_local_channel() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut child = spawn_mcp(tmp.path());
    // 握着 stdin 不放：一撒手 server 就收摊了。
    let stdin = child.stdin.take().expect("stdin");

    let appeared = wait_for_channel(tmp.path());

    // 通道在的时候顺手问一句状态——它走的正是别的命令会走的那条路。
    let status = appeared.then(|| {
        Command::new(env!("CARGO_BIN_EXE_swarmdrop"))
            .args(["status", "--json", "--data-dir"])
            .arg(tmp.path())
            .stdin(Stdio::null())
            .output()
            .expect("执行 swarmdrop status")
    });

    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();

    assert!(
        appeared,
        "swarmdrop mcp 自持了节点却没建本地通道——同机的每一条命令都会撞「另一个进程正在启动」"
    );
    let status = status.expect("上面已断言通道在");
    assert!(
        status.status.success(),
        "有通道却问不出状态: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

/// **`SIGTERM` 必须让它退出，而且是成功退出——即使宿主还握着 stdin。**
///
/// 这条钉的是一个只有真进程才显形的挂死：MCP 的 stdio 传输把 stdin 交给了 tokio 的
/// **阻塞**读任务。信号让服务循环返回之后，若只是 `return`，`main` 返回时运行时析构会
/// 等那次读收尾——而宿主还握着 stdin，那次读永远不返回，**进程就此挂死**。
///
/// 挂死比退非零更糟：服务管理器会一直等到自己的超时，agent harness 会留下一个僵尸子进程。
/// 而单实例锁其实已经释放了，所以从外面看是「停不掉、又不占着什么」，无从归因。
///
/// ⚠️ 判据里的 `stdin` 必须**一直不 drop**——撒手之后 server 会走正常的收摊路径，
/// 那条路径本来就好好的，这条用例就什么都没测到。
#[cfg(unix)]
#[test]
fn sigterm_ends_the_process_even_while_the_host_holds_stdin() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut child = spawn_mcp(tmp.path());
    let stdin = child.stdin.take().expect("stdin");

    assert!(
        wait_for_channel(tmp.path()),
        "节点没起来，这条用例没测到该测的东西"
    );

    let killed = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .expect("发送 SIGTERM");
    assert!(killed.success(), "kill 本身失败了");

    // `wait()` 会一直等——挂死的表现就是这条测试超时，而那正是该看见的失败。
    let status = child.wait().expect("等待退出");
    drop(stdin);

    assert_eq!(
        status.code(),
        Some(0),
        "SIGTERM 之后退出码必须是 0——宿主结束子进程是它的常规动作，不是失败"
    );
}
