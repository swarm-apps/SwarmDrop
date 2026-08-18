//! `pair`：生成配对邀请，或以一个邀请完成配对。

use crate::adapter::paths::DataDir;
use crate::exit::{CliError, CliResult};
use crate::runtime::session::Session;

pub async fn run(
    data_dir: &DataDir,
    json: bool,
    invite: Option<String>,
    no_qr: bool,
) -> CliResult<()> {
    match invite {
        Some(invite) => accept(data_dir, json, &invite).await,
        None => generate(data_dir, json, no_qr).await,
    }
}

/// 生成一张新邀请。
async fn generate(data_dir: &DataDir, json: bool, no_qr: bool) -> CliResult<()> {
    let session = Session::open(data_dir, json).await?;
    let Some(node) = session.local() else {
        // 常驻节点在跑时，邀请必须由**那个**节点签发：邀请里带的是签发者的可拨地址，
        // 本进程另起一个节点签出来的码指向一个即将消失的临时节点。
        session.close().await;
        return Err(CliError::NodeUnavailable(
            "节点正在后台运行；请先 swarmdrop stop 再生成邀请，或在该节点上生成".into(),
        ));
    };

    let result = node
        .manager
        .pairing()
        .encode_invite(&node.secret_key, swarmdrop_invite::TransportPolicy::Auto)
        .await;

    let invite = match result {
        Ok(invite) => invite,
        Err(err) => {
            session.close().await;
            return Err(CliError::NodeUnavailable(format!("生成邀请失败: {err}")));
        }
    };

    crate::render::pair::render_invite(&invite, json, no_qr);
    session.close().await;
    Ok(())
}

/// 以一个邀请完成配对。
async fn accept(data_dir: &DataDir, json: bool, invite: &str) -> CliResult<()> {
    let session = Session::open(data_dir, json).await?;
    let Some(node) = session.local() else {
        session.close().await;
        return Err(CliError::NodeUnavailable(
            "节点正在后台运行；请先 swarmdrop stop 再配对".into(),
        ));
    };

    let outcome = node.manager.pairing().pair_with_invite(invite).await;
    session.close().await;

    match outcome {
        Ok((response, _paired)) => {
            crate::render::pair::render_paired(&response, json);
            Ok(())
        }
        Err(err) => Err(CliError::PeerUnreachable(format!("配对失败: {err}"))),
    }
}
