//! `pair`：生成配对邀请，或以一个邀请完成配对。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::ipc::{Request, Response};
use crate::runtime::session::Session;

pub async fn run(
    data_dir: &DataDir,
    json: bool,
    invite: Option<String>,
    no_qr: bool,
) -> CliResult<()> {
    let session = Session::open(data_dir, json).await?;

    let result = match invite {
        Some(invite) => accept(&session, json, invite).await,
        None => generate(&session, json, no_qr).await,
    };

    session.close().await;
    result
}

/// 生成一张新邀请。
///
/// 常驻节点在跑时**必须由它签发**：邀请里带的是签发者的可拨地址，本进程另起一个节点
/// 签出来的码指向一个即将消失的临时节点——扫码方会拿到一张拨不通的码。
///
/// 没有常驻节点时用临时节点签发，但**签完不能就走**：邀请的可拨地址就是这个临时节点的，
/// 命令一退出它就没了，那张码当场作废。所以这条路径会**保持节点在线直到配对完成或用户
/// 中断**——邀请的有效期本质上等于签发者的在线时长。
async fn generate(session: &Session, json: bool, no_qr: bool) -> CliResult<()> {
    let invite = match session.ask(&Request::PairGenerate).await? {
        Some(Response::Data { payload }) => payload
            .get("invite")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(|| CliError::NodeUnavailable("节点返回的邀请为空".into()))?,
        Some(Response::Error { message }) => return Err(CliError::NodeUnavailable(message)),
        Some(Response::Ok) | None => {
            let node = session
                .local()
                .ok_or_else(|| CliError::NodeUnavailable("节点不可用".into()))?;
            node.manager
                .pairing()
                .encode_invite(&node.secret_key, swarmdrop_invite::TransportPolicy::Auto)
                .await
                .map_err(|err| CliError::NodeUnavailable(format!("生成邀请失败: {err}")))?
        }
    };

    crate::render::pair::render_invite(&invite, json, no_qr);

    // 临时节点：必须撑到配对完成，否则这张码随进程一起消失。
    if let Some(node) = session.local() {
        crate::render::pair::render_waiting(json);
        wait_for_pairing(node).await?;
    }
    Ok(())
}

/// 等到有设备配对成功，或用户中断。
async fn wait_for_pairing(node: &crate::runtime::boot::RunningNode) -> CliResult<()> {
    use swarmdrop_core::host::CoreEvent;

    let mut events = node.events.subscribe();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => return Err(CliError::Aborted),
            event = events.recv() => match event {
                Some(CoreEvent::PairedDeviceAdded { device, .. }) => {
                    crate::render::pair::render_paired_with(&device.os_info, false);
                    return Ok(());
                }
                // 事件通道断开只发生在节点关停时。
                None => return Err(CliError::Aborted),
                Some(_) => {}
            },
        }
    }
}

/// 以一个邀请完成配对。
///
/// **先在本地解码一次**再交给节点：解码失败是用户把串抄错了（用法错误），与「对端连不上」
/// 是两回事，而退出码要区分它们——脚本对这两种的处置不同（一个是改参数重来，一个是等对方
/// 上线再试）。不先解码的话，两种失败会一起落进「对端不可达」。
async fn accept(session: &Session, json: bool, invite: String) -> CliResult<()> {
    swarmdrop_invite::PairInvite::decode(&invite)
        .map_err(|err| CliError::Usage(format!("邀请串无效: {err}；请确认完整复制了整条链接")))?;

    let outcome = match session
        .ask(&Request::PairAccept {
            invite: invite.clone(),
        })
        .await?
    {
        Some(Response::Ok) | Some(Response::Data { .. }) => Ok(()),
        Some(Response::Error { message }) => Err(CliError::PeerUnreachable(message)),
        None => {
            let node = session
                .local()
                .ok_or_else(|| CliError::NodeUnavailable("节点不可用".into()))?;
            node.manager
                .pairing()
                .pair_with_invite(&invite)
                .await
                .map(|_| ())
                .map_err(|err| CliError::PeerUnreachable(format!("配对失败: {err}")))
        }
    };

    outcome?;
    crate::render::pair::render_paired(json);
    Ok(())
}
