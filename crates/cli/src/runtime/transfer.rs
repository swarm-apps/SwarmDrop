//! 发送编排：解析目标 → 枚举文件 → 准备 → 发出 → 等终态。
//!
//! 放在 runtime 而非 cmd，是因为**两条路径都要用它**：本进程自持节点时直接调，
//! 常驻节点在跑时由通道服务端调。两处各写一遍会立刻漂移。

use std::path::{Path, PathBuf};

use swarmdrop_core::protocol::TransferOrigin;
use swarmdrop_core::transfer::HostEnumeratedFile;

use crate::exit::{CliError, CliResult};

use super::boot::RunningNode;

/// 发送结果。
pub struct SendOutcome {
    pub session_id: uuid::Uuid,
    pub file_count: usize,
    pub total_bytes: u64,
}

/// 把文件或目录发给一台已配对设备，**阻塞到传输终态**。
pub async fn send_files(
    node: &RunningNode,
    paths: &[PathBuf],
    to: &str,
    show_progress: bool,
) -> CliResult<SendOutcome> {
    let (peer_id, peer_name) = resolve_target(node, to)?;

    let files = collect_files(paths)?;
    if files.is_empty() {
        return Err(CliError::Usage("没有可发送的文件".into()));
    }
    let file_count = files.len();
    let total_bytes = files.iter().map(|f| f.size).sum();

    let transfer = node.manager.transfer_arc();
    let prepared_id = uuid::Uuid::new_v4();
    let prepared = transfer
        .prepare(prepared_id, files)
        .await
        .map_err(|err| CliError::TransferFailed(format!("准备发送失败: {err}")))?;

    let selected: Vec<u32> = prepared.files.iter().map(|f| f.file_id).collect();

    // **先订阅再发出**：反过来的话，一次极快的传输可能在订阅建立之前就结束，
    // 于是等待方永远等不到那条终态事件。
    let mut events = node.events.subscribe();

    let started = transfer
        .send_offer(
            &prepared.prepared_id,
            &peer_id,
            &peer_name,
            &selected,
            TransferOrigin::Human,
        )
        .await
        .map_err(|err| CliError::PeerUnreachable(format!("发出传输请求失败: {err}")))?;
    let session_id = started.session_id;

    wait_for_terminal(&mut events, session_id, show_progress).await?;

    Ok(SendOutcome {
        session_id,
        file_count,
        total_bytes,
    })
}

/// 等待这条会话进入终态。
///
/// 只认**本会话**的事件：同一个节点上可能有其他传输在跑，按事件类型而不看会话号会
/// 让一条无关传输的失败把本命令带下水。
async fn wait_for_terminal(
    events: &mut tokio::sync::mpsc::UnboundedReceiver<swarmdrop_core::host::CoreEvent>,
    session_id: uuid::Uuid,
    show_progress: bool,
) -> CliResult<()> {
    use swarmdrop_core::host::CoreEvent;

    // 「该不该画」在这里回答一次；之后的调用点无条件调用它。
    let progress = crate::render::send::Progress::new(show_progress);

    while let Some(event) = events.recv().await {
        match event {
            CoreEvent::TransferProgress { event } if event.session_id == session_id => {
                progress.update(event.transferred_bytes, event.total_bytes);
            }
            CoreEvent::TransferCompleted { event } if event.session_id == session_id => {
                // 进度条由 `Progress` 的 `Drop` 收尾——这里以及下面三条出口都不必各收一次。
                return Ok(());
            }
            CoreEvent::TransferFailed { event } if event.session_id == session_id => {
                return Err(CliError::TransferFailed(format!(
                    "传输失败: {}",
                    event.error
                )));
            }
            CoreEvent::TransferRejected { event } if event.session_id == session_id => {
                return Err(CliError::TransferFailed("对端拒绝了这次传输".into()));
            }
            _ => {}
        }
    }

    // 事件通道断开只发生在节点关停时（另一个终端跑了 `swarmdrop stop`，或进程被杀）。
    //
    // **不是 `Aborted`**：那个分类的退出码是 130（`128 + SIGINT`），脚本按惯例读作
    // 「人按了 Ctrl-C，别重试」。而这里是传输被外力打断，重试完全合理——分类错了会让
    // 一次本该恢复的中断被当成用户主动放弃。
    Err(CliError::TransferFailed("常驻节点已停止，传输中断".into()))
}

/// 把设备名或节点标识解析成一台已配对设备。
///
/// 允许用名字是因为节点标识对人不可读；名字重复时报错而不是随便挑一个——
/// 「发错设备」是不可撤销的。
fn resolve_target(node: &RunningNode, to: &str) -> CliResult<(String, String)> {
    let devices = node.manager.devices().get_devices(Default::default());

    let matched: Vec<_> = devices
        .iter()
        .filter(|d| {
            let id = d.peer_id.to_string();
            if id == to {
                return true;
            }
            let name = display_name(d);
            name.eq_ignore_ascii_case(to)
        })
        .collect();

    match matched.as_slice() {
        [] => Err(CliError::Usage(format!(
            "找不到已配对设备「{to}」；执行 swarmdrop device list 查看可用目标"
        ))),
        [device] => Ok((device.peer_id.to_string(), display_name(device))),
        multiple => Err(CliError::Usage(format!(
            "「{to}」匹配到 {} 台设备，请改用节点标识指定",
            multiple.len()
        ))),
    }
}

fn display_name(device: &swarmdrop_core::device::Device) -> String {
    device
        .os_info
        .name
        .clone()
        .unwrap_or_else(|| device.os_info.hostname.clone())
}

/// 展开命令行给的路径：文件直接收，目录递归展开。
///
/// 自己实现而不复用桌面那份目录扫描：那份的产物带着跨 IPC 的类型（要给前端渲染选择列表），
/// 而这里要的是核心的 [`HostEnumeratedFile`]。形状不同，共享反而要多一层转换。
fn collect_files(paths: &[PathBuf]) -> CliResult<Vec<HostEnumeratedFile>> {
    let mut out = Vec::new();
    for path in paths {
        let meta = std::fs::metadata(path)
            .map_err(|err| CliError::Usage(format!("读取 {} 失败: {err}", path.display())))?;

        if meta.is_dir() {
            let root_name = file_name_of(path);
            walk(path, &root_name, &mut out)?;
        } else {
            out.push(entry_of(path, file_name_of(path), meta.len()));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<HostEnumeratedFile>) -> CliResult<()> {
    let entries = std::fs::read_dir(dir)
        .map_err(|err| CliError::Usage(format!("读取目录 {} 失败: {err}", dir.display())))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = file_name_of(&path);
        // 相对路径一律用 `/` 分隔：它要跨平台传给对端，Windows 的 `\` 在那边会变成文件名的一部分。
        let relative = format!("{prefix}/{name}");

        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk(&path, &relative, out)?;
        } else if meta.is_file() {
            out.push(entry_of(&path, relative, meta.len()));
        }
        // 其他类型（符号链接指向的特殊文件、设备文件等）跳过：它们没有可传输的字节。
    }
    Ok(())
}

fn entry_of(path: &Path, relative_path: String, size: u64) -> HostEnumeratedFile {
    HostEnumeratedFile {
        // 标识就是路径本身——本地文件访问实现按「先试 JSON、否则当路径」解回来。
        source_id: swarmdrop_core::host::FileSourceId(path.to_string_lossy().into_owned()),
        name: file_name_of(path),
        relative_path,
        size,
    }
}

fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 目录要递归展开，且相对路径带上根目录名——对端据此重建目录结构。
    #[test]
    fn directory_expands_recursively_with_relative_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("bundle");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), "aa").unwrap();
        std::fs::write(root.join("sub/b.txt"), "bbb").unwrap();

        let files = collect_files(&[root]).unwrap();

        assert_eq!(files.len(), 2);
        let mut paths: Vec<_> = files.iter().map(|f| f.relative_path.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["bundle/a.txt", "bundle/sub/b.txt"]);
    }

    /// 单个文件的相对路径就是文件名，不带任何目录前缀。
    #[test]
    fn single_file_has_bare_relative_path() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("solo.bin");
        std::fs::write(&file, [0u8; 8]).unwrap();

        let files = collect_files(&[file]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "solo.bin");
        assert_eq!(files[0].size, 8);
    }

    /// 路径不存在必须是用法错误，而不是等到传输开始才失败。
    #[test]
    fn missing_path_is_a_usage_error() {
        let err = collect_files(&[PathBuf::from("/definitely/not/here")]).unwrap_err();
        assert!(matches!(err, CliError::Usage(_)));
    }
}
