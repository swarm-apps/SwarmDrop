//! 真实进程下的两条承诺：**不该起节点的命令不起节点**，**问不了人的命令不挂起**。
//!
//! 这两条单元测试都覆盖不到。前者的失败形态是「命令能用，只是慢了几秒」，后者是
//! 「在管道里永久挂住且日志无异常」——都要真的把二进制跑起来才看得见。

use std::path::Path;
use std::process::{Command, Output, Stdio};

/// 跑一条命令，stdin 接空（模拟管道 / CI：**不是终端**）。
fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarmdrop"))
        .args(args)
        .arg("--data-dir")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("执行 swarmdrop")
}

/// 节点装配会创建身份文件（`load_or_create_identity`），只读本机记录的路径不会。
///
/// 用它当判据而不是计时：耗时会随机器与网络波动，而这个文件在不在是确定的。
fn started_a_node(dir: &Path) -> bool {
    dir.join("identity.json").exists()
}

/// **只读本机记录的命令一律不得启动节点。**
///
/// 这是本次改动的核心承诺。它答错不报错——只是把「看一眼本机记录」变成一次连引导
/// 节点、做 NAT 探测的几秒等待，而在网络不通的机器上更会直接失败。
#[test]
fn record_commands_never_start_a_node() {
    // **每条只读本机记录的命令都要在这里**。这是「不启动节点」唯一的真护栏——
    // 它从进程外判断（身份文件在不在），而不是让代码断言自己的声明。
    // 曾经还有一条 `command_needs_are_deliberate` 断言每条命令的档位枚举，
    // 但那是自我验证：改了实现让 `invite list` 去起节点，它照样绿。
    for args in [
        ["invite", "list"].as_slice(),
        ["device", "list"].as_slice(),
        ["transfer", "list"].as_slice(),
        ["inbox", "list"].as_slice(),
        // 带参数的两条：解析失败会走另一条返回路径，同样不该起节点。
        ["transfer", "show", "00000000-0000-4000-8000-000000000000"].as_slice(),
        ["inbox", "show", "00000000-0000-4000-8000-000000000000"].as_slice(),
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = run(tmp.path(), args);

        // 成功、或「查无此记录」都可以——空机器上按标识查本就查不到。
        // **不能接受的是「节点未就绪」**（退出码 3）：那说明它试图起节点。
        let code = output.status.code();
        assert_ne!(
            code,
            Some(3),
            "{args:?} 报了「节点不可用」——它不该需要节点: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            !started_a_node(tmp.path()),
            "{args:?} 启动了节点——它只该读本机记录"
        );
    }
}

/// 空机器上的清单命令要给出空结果，而不是报错。
///
/// 新装的机器上执行 `invite list` 是最普通的一次操作，报错会让人以为装坏了。
#[test]
fn listing_on_a_fresh_machine_succeeds() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = run(tmp.path(), &["invite", "list", "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout 应当是合法 JSON");
    assert_eq!(parsed.as_array().map(Vec::len), Some(0));
}

/// **管道里缺参数必须立刻退出，绝不读 stdin。**
///
/// 挂起的表现是命令永远不返回、日志里什么也没有——CI 上只会看到一个超时。
///
/// **每一个可缺省的参数都要在这里**。清单与 `cmd::tests::every_optional_target_makes_
/// the_command_interactive` 一一对应，但看守的是另一件事：那边管日志安不安静，
/// 这边管进程会不会挂住。空机器上有些会先撞到「集合是空的」，同样是用法错误——
/// 两条路径都必须**退出**，这正是本测试的判据。
#[test]
fn missing_argument_in_a_pipe_exits_with_usage() {
    for args in [
        ["invite", "revoke"].as_slice(),
        ["invite", "use"].as_slice(),
        ["device", "forget"].as_slice(),
        ["inbox", "show"].as_slice(),
        ["inbox", "export"].as_slice(),
        ["transfer", "show"].as_slice(),
        ["send"].as_slice(),
        ["send", "--to", "phone"].as_slice(),
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let output = run(tmp.path(), args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?} 缺参数应以用法错误退出: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // **补不出参数就不该已经起了节点。** 只对 `send` 断言是不够的：`invite use`
        // 今天恰好也先问后起，但没有任何东西钉住它——把 `NodeAccess::open` 挪到提问
        // 之前（比如想预热连接）照样全绿，而每一条误调用都会从一毫秒的用法错误
        // 变成几秒的启动 + NAT 探测。
        assert!(
            !started_a_node(tmp.path()),
            "{args:?} 在补参数失败之前就启动了节点"
        );
    }
}

/// **`send` 补参数补不出来时，不得先起一个节点。**
///
/// 起临时节点要连引导节点、做 NAT 探测，以秒计。补参数只读本机记录，把它排在
/// 起节点之前，管道里这条命令就是立刻失败而不是几秒后失败——而顺序写反了**不报错**，
/// 只是慢，所以没有别的东西看得见它。
#[test]
fn send_resolves_its_arguments_before_starting_a_node() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = run(tmp.path(), &["send", "/etc/hosts"]);

    assert_eq!(output.status.code(), Some(2), "缺 --to 应以用法错误退出");
    assert!(
        !started_a_node(tmp.path()),
        "补参数之前就把节点起了——那几秒等待毫无用处"
    );
}

/// 结构化模式下 stdout 只能有结果，诊断一律走 stderr。
#[test]
fn structured_output_keeps_stdout_clean() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = run(tmp.path(), &["device", "list", "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str::<serde_json::Value>(stdout.trim()).expect("stdout 混入了非 JSON 内容");
}

/// 无节点时 `status` 如实报「停止」，**不去起一个临时节点来问自己**。
#[test]
fn status_without_a_node_reports_stopped() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = run(tmp.path(), &["status", "--json"]);

    assert!(output.status.success());
    assert!(
        !started_a_node(tmp.path()),
        "status 启动了节点——那样它报告的 Running 是这次提问自己造成的"
    );
}

/// `pair` / `devices` / `inbox get` 必须不存在，**连别名都没有**。
#[test]
fn removed_commands_are_gone() {
    let tmp = tempfile::tempdir().expect("tempdir");
    for args in [
        ["pair"].as_slice(),
        ["devices"].as_slice(),
        ["inbox", "get", "x"].as_slice(),
    ] {
        let output = run(tmp.path(), args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?} 仍然可用——旧命令必须彻底消失"
        );
    }
}
