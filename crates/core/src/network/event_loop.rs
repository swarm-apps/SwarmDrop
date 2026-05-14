//! libp2p 事件到 core 事件的转换。

use swarm_p2p_core::NodeEvent;

use super::SharedNetRefs;
use crate::device_manager::DeviceFilter;
use crate::error::AppResult;
use crate::host::{CoreEvent, EventBus};
use crate::protocol::AppRequest;

/// 处理单个 libp2p 事件，并通过宿主事件总线发布稳定的 core 事件。
///
/// 这个函数只处理网络、设备发现和配对请求这类跨宿主通用逻辑。
/// 文件传输请求仍由宿主层处理，后续会继续迁移到 core。
pub async fn handle_core_node_event<TTransfer, B>(
    shared: &SharedNetRefs<TTransfer>,
    event: &NodeEvent<AppRequest>,
    event_bus: &B,
) -> AppResult<()>
where
    B: EventBus + ?Sized,
{
    shared.devices.handle_event(event);

    match event {
        NodeEvent::Listening { addr } => {
            if let Ok(mut addrs) = shared.listen_addrs.write() {
                addrs.push(addr.clone());
            }
            publish_network_status(shared, event_bus).await?;
        }
        NodeEvent::NatStatusChanged {
            status,
            public_addr,
        } => {
            if let Ok(mut ns) = shared.nat_status.write() {
                *ns = status.clone();
            }
            if let Ok(mut pa) = shared.public_addr.write() {
                *pa = public_addr.clone();
            }
            publish_network_status(shared, event_bus).await?;
        }
        NodeEvent::RelayReservationAccepted { relay_peer_id, .. } => {
            if let Ok(mut relay_peers) = shared.relay_peers.write() {
                relay_peers.insert(*relay_peer_id);
            }
            publish_network_status(shared, event_bus).await?;
        }
        NodeEvent::PeerDisconnected { peer_id } => {
            if let Ok(mut relay_peers) = shared.relay_peers.write() {
                relay_peers.remove(peer_id);
            }
            publish_devices_and_status(shared, event_bus).await?;
        }
        NodeEvent::PeerConnected { .. }
        | NodeEvent::IdentifyReceived { .. }
        | NodeEvent::PeersDiscovered { .. }
        | NodeEvent::PingSuccess { .. }
        | NodeEvent::HolePunchSucceeded { .. } => {
            publish_devices_and_status(shared, event_bus).await?;
        }
        NodeEvent::InboundRequest {
            peer_id,
            pending_id,
            request: AppRequest::Pairing(request),
        } => {
            shared
                .pairing
                .cache_inbound_request(*peer_id, *pending_id, request);
            event_bus
                .publish(CoreEvent::PairingRequestReceived {
                    peer_id: *peer_id,
                    pending_id: *pending_id,
                    request: request.clone(),
                })
                .await?;
        }
        NodeEvent::InboundRequest {
            request: AppRequest::Transfer(_),
            ..
        }
        | NodeEvent::HolePunchFailed { .. } => {}
    }

    Ok(())
}

async fn publish_devices_and_status<TTransfer, B>(
    shared: &SharedNetRefs<TTransfer>,
    event_bus: &B,
) -> AppResult<()>
where
    B: EventBus + ?Sized,
{
    let devices = shared.devices.get_devices(DeviceFilter::All);
    event_bus
        .publish(CoreEvent::DevicesChanged { devices })
        .await?;
    publish_network_status(shared, event_bus).await
}

async fn publish_network_status<TTransfer, B>(
    shared: &SharedNetRefs<TTransfer>,
    event_bus: &B,
) -> AppResult<()>
where
    B: EventBus + ?Sized,
{
    event_bus
        .publish(CoreEvent::NetworkStatusChanged {
            status: shared.build_network_status(),
        })
        .await
}
