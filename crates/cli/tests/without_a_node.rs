//! 真实进程下的三条承诺：**不该起节点的命令不起节点**，**问不了人的命令不挂起**，
//! **长驻命令收到终止信号时干净地成功退出**。
//!
//! 三条单元测试都覆盖不到。第一条的失败形态是「命令能用，只是慢了几秒」，第二条是
//! 「在管道里永久挂住且日志无异常」，第三条要有一个真的能收信号的进程——
//! 都要把二进制跑起来才看得见。

use std::io::BufRead;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};

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
        ["config", "list"].as_slice(),
        ["config", "get", "device-name"].as_slice(),
        ["bootstrap", "list"].as_slice(),
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
        ["bootstrap", "add"].as_slice(),
        ["bootstrap", "remove"].as_slice(),
        ["send"].as_slice(),
        ["send", "--to", "phone"].as_slice(),
        // 文本那支缺目标时同样要立刻退出。
        //
        // ⚠️ **这两条只覆盖到 `choose_target`**：空数据目录里一台已配对设备都没有，
        // 所以 `--to phone` 也解析不出来，正文那一段一行都执行不到。
        // 真正覆盖正文来源的是下面的 `reading_the_body_from_a_pipe_never_hangs`
        // ——它先把一台设备写进配对表，让流程走得过第一关。
        ["send", "--text"].as_slice(),
        ["send", "--text", "你好"].as_slice(),
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

/// **写配置同样不得启动节点**，而且写完读得回来。
///
/// 与只读那条分开是因为失败形态不同：只读走错档只是慢，而**写**走错档会先花几秒起一个
/// 临时节点、改完再把它关掉——那期间本机会真的上线，一条本该完全本地的操作产生了网络
/// 流量（spec: `cli-config-surface` 的「读写配置不启动节点」）。
#[test]
fn writing_settings_never_starts_a_node() {
    let tmp = tempfile::tempdir().expect("tempdir");

    let set = run(tmp.path(), &["config", "set", "device-name", "书房 Mac"]);
    assert!(
        set.status.success(),
        "写入失败: {}",
        String::from_utf8_lossy(&set.stderr)
    );
    assert!(!started_a_node(tmp.path()), "config set 启动了节点");

    let get = run(tmp.path(), &["config", "get", "device-name"]);
    assert_eq!(String::from_utf8_lossy(&get.stdout).trim(), "书房 Mac");
    assert!(!started_a_node(tmp.path()), "config get 启动了节点");

    let add = run(
        tmp.path(),
        &[
            "bootstrap",
            "add",
            "/ip4/198.51.100.7/tcp/4001/p2p/12D3KooWDpJ7As7BWAwRMfu1VU2WCqNjvq387JEYKDBj4kx6nXTN",
        ],
    );
    assert!(
        add.status.success(),
        "添加失败: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(!started_a_node(tmp.path()), "bootstrap add 启动了节点");
}

/// 不认识的配置项以**用法错误**退出，并把可用的那些列出来。
///
/// 静默收下一个没人读的键是最坏的形态：用户以为自己配好了。
#[test]
fn an_unknown_setting_is_a_usage_error_that_lists_the_valid_ones() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let output = run(tmp.path(), &["config", "set", "nonesuch", "x"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("device-name"), "没列出可用的键: {stderr}");
    assert!(stderr.contains("receive-dir"), "没列出可用的键: {stderr}");
}

/// **环境变量压住持久化配置，而被压住的那个值仍要给出来。**
///
/// 从进程外测是唯一可靠的做法：环境变量是进程级的，在并行跑的单测里改它会互相踩。
/// 少了 `configured` 那一半，设置界面只能显示一个用户改不动的值，于是用户会反复修改
/// 一个不生效的输入框。
#[test]
fn an_environment_override_hides_the_configured_value_but_still_reports_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let configured = tmp.path().join("configured-drop");
    let overriding = tmp.path().join("env-drop");

    let set = run(
        tmp.path(),
        &[
            "config",
            "set",
            "receive-dir",
            &configured.to_string_lossy(),
        ],
    );
    assert!(set.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_swarmdrop"))
        .args(["config", "list", "--json"])
        .arg("--data-dir")
        .arg(tmp.path())
        .env("SWARMDROP_RECEIVE_DIR", &overriding)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("执行 swarmdrop");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(stdout.trim()).expect("stdout 应当是合法 JSON");
    let row = rows
        .iter()
        .find(|row| row["key"] == "receive-dir")
        .expect("清单里应当有接收落点");

    assert_eq!(row["source"], "env");
    assert_eq!(row["value"], overriding.to_string_lossy().as_ref());
    assert_eq!(
        row["configured"],
        configured.to_string_lossy().as_ref(),
        "被压住的那个值必须给出来"
    );
    assert_eq!(row["overriddenBy"], "SWARMDROP_RECEIVE_DIR");
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
/// **管道里的 `send --text` 读标准输入，绝不拉起 `$EDITOR`，也不起节点。**
///
/// 这条要先把一台已配对设备写进记录里才有意义：`send` 的顺序是「解析目标 → 取正文」，
/// 空数据目录下前一步就退出了，正文那一段一行都执行不到——本文件此前那三条 `--text`
/// 用例正是这样，与 `["send", "--to", "phone"]` 是同一个断言，零增量。
///
/// 走到正文之后，管道里的 stdin 接的是空 ⇒ 读到 EOF ⇒ 空正文 ⇒ 用法错误。
/// **拉起编辑器会挂死**（一个全屏程序接到一条管道上，两边都动不了），
/// 所以这条同时是「分流判据没写反」的看守。
#[test]
fn reading_the_body_from_a_pipe_never_hangs() {
    use swarmdrop_core::host::PairedDeviceStore;
    use swarmdrop_host::device::{OsInfo, PairedDeviceInfo};

    let tmp = tempfile::tempdir().expect("tempdir");
    let peer_id = swarmdrop_net::SecretKey::generate().node_id();
    let store = swarmdrop_host_fs::JsonFileIdentityStore::new(tmp.path());
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(store.save_paired_devices(&[PairedDeviceInfo::new(
            peer_id,
            OsInfo::default(),
            1,
        )]))
        .expect("写入配对表");

    let output = run(
        tmp.path(),
        &["send", "--text", "--to", &peer_id.to_string()],
    );

    assert_eq!(
        output.status.code(),
        Some(2),
        "空正文应当以用法错误退出: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("不能为空"),
        "报的不是空正文，说明没走到读标准输入那一步: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!started_a_node(tmp.path()), "正文校验之前就把节点起了");
}

/// 拉起一条**长驻**命令，stdout 接管道以便读它的输出。
///
/// 与 [`run`] 的区别是它**不等进程结束**——长驻命令永远不结束，用 `.output()` 等于
/// 让测试永久挂住。这也是 `watch` 不能加进 [`record_commands_never_start_a_node`]
/// 那张表的原因：那里用的正是 `.output()`。
fn spawn(dir: &Path, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_swarmdrop"))
        .args(args)
        .arg("--data-dir")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("执行 swarmdrop")
}

/// **`watch` 不得启动节点，且没有节点时也要照常给出基线。**
///
/// 两件事一起测是因为它们是同一条承诺的两半：订阅只观察本机发生的事，所以既不该起
/// 节点，也不该因为没有节点就报错退出——调用方（agent harness 的插件）完全可能先拉起
/// 订阅再拉起节点。少了后半条，宿主就得自己写一套重试与竞态处理。
#[test]
fn watch_without_a_node_emits_a_baseline_and_starts_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut child = spawn(tmp.path(), &["watch", "--json"]);

    // 读到第一行就说明它已经做完了启动时该做的一切。**不用计时判据**：
    // 耗时随机器波动，而「基线出来了没有」是确定的。
    let stdout = child.stdout.take().expect("stdout");
    let mut line = String::new();
    std::io::BufReader::new(stdout)
        .read_line(&mut line)
        .expect("读取基线");

    let _ = child.kill();
    let _ = child.wait();

    let event: serde_json::Value = serde_json::from_str(line.trim()).expect("基线应当是合法 JSON");
    assert_eq!(event["kind"], "baseline", "第一帧必须是基线: {line}");
    assert_eq!(event["seq"], 0, "序号从 0 起: {line}");
    assert_eq!(event["v"], 1, "每条都要带 schema 版本: {line}");
    assert_eq!(
        event["nodeRunning"], false,
        "没有常驻节点时基线必须如实说: {line}"
    );

    assert!(
        !started_a_node(tmp.path()),
        "watch 启动了节点——它只观察，不该有任何数据包因它离开本机"
    );
}

/// **`SIGTERM` 必须以成功退出。**
///
/// 这是本命令最常见的收摊路径，不是补充场景：agent harness 结束子进程与服务管理器停止
/// 服务用的都是它。而 `tokio::signal::ctrl_c()` 在 Unix 上**只接 `SIGINT`**——不额外接
/// `SIGTERM` 的话，最常见的那条正常停止会走不到清理、退出码非零，而调用方把非零读作
/// 失败并触发重启或告警。
///
/// 拿 `watch` 当代表：三条长驻命令（`watch` / `mcp` / `start` 前台）共用同一个
/// [`crate::runtime::signal::shutdown`]，而只有它不需要真的起一个节点。
#[cfg(unix)]
#[test]
fn a_long_running_command_exits_successfully_on_sigterm() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let mut child = spawn(tmp.path(), &["watch", "--json"]);

    // 等第一帧——它同时证明信号监听已经注册好了（注册发生在拼基线之前的那次 select）。
    let stdout = child.stdout.take().expect("stdout");
    let mut line = String::new();
    std::io::BufReader::new(stdout)
        .read_line(&mut line)
        .expect("读取基线");

    let killed = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .expect("发送 SIGTERM");
    assert!(killed.success(), "kill 本身失败了");

    let status = child.wait().expect("等待退出");
    assert_eq!(
        status.code(),
        Some(0),
        "SIGTERM 之后退出码必须是 0——用户主动结束一次订阅不是失败"
    );
}
