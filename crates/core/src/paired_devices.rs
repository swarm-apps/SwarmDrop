//! 已配对设备列表的读写算法，以及需要「节点在不在跑」分支的两条宿主编排。
//!
//! 端口 [`PairedDeviceStore`] 只提供「整份读 / 整份写」两个动作，本模块是它之上**唯一**
//! 的业务层。两个高度，读的时候分开看：
//!
//! - **列表算法** —— [`upsert`] / [`update_policy`] / [`remove`]：纯粹的 load-改-save，
//!   不认识 `NetManager`，也不发事件。三端 host 都调这里，语义分叉在结构上不可能再发生
//!   （历史教训：Web 曾自己写过一份 upsert，把用户设的信任策略整条替换掉）。
//! - **宿主编排** —— [`unpair`] / [`set_receive_policy`]：收 `Option<&NetManager>`，把
//!   「节点在跑 → 走内存表 + 事件；没跑 → 只改持久化」这条分支收进 core。形态照抄
//!   [`rename_device`](crate::device_name::rename_device)，理由也一样：分支留在宿主侧，
//!   三端就会各写一遍，然后各自漂。移动端此前正是漏了「节点没跑时补发
//!   `PairedDeviceRemoved`」，RN 侧于是收不到任何移除事件。
//!
//! 与 [`crate::identity`] 的分工：那边只管密钥材料（设备私钥 / WebRTC 证书），
//! 这边只管一份可导出、会整份覆写的业务数据。
//!
//! **解除配对不要直接调 [`remove`]**：节点运行时它只改持久化，共享内存表还留着该 peer，
//! presence 保活与重探会一直跑下去。入口是本模块的 [`unpair`]（它内部按节点是否在跑
//! 分派到 [`PairingManager::unpair`](crate::pairing::PairingManager::unpair) 或 [`remove`]）。

use swarmdrop_net::NodeId;

use crate::device::{DeviceReceivePolicy, DeviceTrustLevel, PairedDeviceInfo};
use crate::error::{AppError, AppResult};
use crate::host::{CoreEvent, EventBus, PairedDeviceStore};
use crate::network::{NetManager, TransferRuntime};

/// 添加或刷新一个已配对设备，并返回更新后的列表。
///
/// 已存在的条目**只更新 `os_info` / `paired_at`**，保留 `trust_level` /
/// `receive_policy` / `trust_confirmed`。
///
/// **为什么不整条替换。** 调用方传进来的 `device` 来自
/// [`PairedDeviceInfo::new`](crate::device::PairedDeviceInfo::new)——配对成功回调构造的
/// 就是它，恒为默认信任级别（`Collaborator` + 该级别的默认收件策略）。整条替换意味着
/// 「对一台已配对设备再走一次邀请配对」会把用户手工调过的值静默重置：`trust_level` 从
/// `Owned` 掉回 `Collaborator`，用户收紧过的 `receive_policy` 也会被放回默认。而
/// `receive_policy` 是**被真正裁决的**（入站 offer 经 `swarmdrop_transfer::policy`
/// 判定），不是展示字段，所以这不是显示问题，是一次静默的策略变更。
pub async fn upsert<S>(store: &S, device: PairedDeviceInfo) -> AppResult<Vec<PairedDeviceInfo>>
where
    S: PairedDeviceStore + ?Sized,
{
    let mut devices = store.load_paired_devices().await?;
    if let Some(existing) = devices
        .iter_mut()
        .find(|item| item.peer_id == device.peer_id)
    {
        existing.os_info = device.os_info;
        existing.paired_at = device.paired_at;
    } else {
        devices.push(device);
    }
    store.save_paired_devices(&devices).await?;
    Ok(devices)
}

/// 更新已配对设备的可信策略，返回**更新后的那台设备**。
///
/// 返回单台而非整份列表（与 [`upsert`] / [`remove`] 不同）：调用方要的从来只有这一台，
/// 而「返回列表 + 调用方 `find(peer_id).ok_or("未找到已配对设备")`」是第二遍存在性检查
/// ——本函数找不到时已经返回 `Err` 了，那个 `ok_or` 分支永远走不到，却在两端各写了一遍。
pub async fn update_policy<S>(
    store: &S,
    peer_id: &NodeId,
    trust_level: DeviceTrustLevel,
    receive_policy: Option<DeviceReceivePolicy>,
) -> AppResult<PairedDeviceInfo>
where
    S: PairedDeviceStore + ?Sized,
{
    let mut devices = store.load_paired_devices().await?;
    let Some(device) = devices.iter_mut().find(|item| &item.peer_id == peer_id) else {
        return Err(AppError::Identity("未找到已配对设备".to_string()));
    };

    device.trust_level = trust_level;
    // `None` = 「按新级别取默认」。**带上这台设备当前的策略**——用户显式设过的保存位置与
    // 代收授权要留住，判据见 [`DeviceReceivePolicy::for_trust_level`]。
    //
    // 此前这里传的是「没有上一份」，于是同一个产品动作有两种行为：桌面 UI 走自己那份 JS
    // 副本会保留保存位置，而任何直接传 `None` 的路径（MCP 工具、将来的批量操作）会把它清掉，
    // 表现是自动接收静默失效而开关还开着。规则收进内核之后这条缝没了。
    device.receive_policy = receive_policy.unwrap_or_else(|| {
        DeviceReceivePolicy::for_trust_level(trust_level, Some(&device.receive_policy))
    });
    device.trust_confirmed = true;
    let updated = device.clone();

    store.save_paired_devices(&devices).await?;
    Ok(updated)
}

/// 从持久化列表移除一个已配对设备，并返回更新后的列表。
///
/// **只动持久化**，也不发事件。节点运行时另有一份共享内存表（presence 白名单与
/// `is_paired` 的事实源），删它才是 presence 撤销的开关——所以宿主要的入口是
/// [`unpair`]，不是本函数。
pub async fn remove<S>(store: &S, peer_id: &NodeId) -> AppResult<Vec<PairedDeviceInfo>>
where
    S: PairedDeviceStore + ?Sized,
{
    let mut devices = store.load_paired_devices().await?;
    devices.retain(|item| &item.peer_id != peer_id);
    store.save_paired_devices(&devices).await?;
    Ok(devices)
}

/// 解除配对（宿主唯一入口），返回更新后的列表。
///
/// `net = None` 表示节点未运行：此时共享内存表根本不存在，持久化删除就是全部副作用，
/// 但**事件仍要由这里补发**——core 的 event bus 在那条路径上不在场，而
/// [`PairingManager::unpair`](crate::pairing::PairingManager::unpair) 只在节点在跑时发。
/// 这一句就是本函数存在的全部理由：桌面此前在命令层手工补了它，移动端忘了补，于是
/// 「节点没起时解除配对」在 RN 侧收不到任何移除事件——同一段分支写两遍必然漂的实例。
///
/// 幂等性与 `PairingManager::unpair` 一致：两处都不含该 peer 时是 no-op，**不发事件**
/// （保持「事件 == 集合真的变了」这个不变量，避免下游把重复点击当成两次变更）。
pub async fn unpair<T>(
    peer_id: &NodeId,
    store: &dyn PairedDeviceStore,
    events: &dyn EventBus,
    net: Option<&NetManager<T>>,
) -> AppResult<Vec<PairedDeviceInfo>>
where
    T: TransferRuntime,
{
    match net {
        // 节点在跑：持久化 → 共享内存表 → 事件三个副作用是原子的，全部在 PairingManager
        // 里，事件也由它自己发（那条路径上 event bus 在场）。
        Some(net) => net.pairing().unpair(peer_id).await,
        None => {
            let devices = store.load_paired_devices().await?;
            if !devices.iter().any(|item| &item.peer_id == peer_id) {
                return Ok(devices);
            }
            let devices = remove(store, peer_id).await?;
            let _ = events
                .publish(CoreEvent::PairedDeviceRemoved { peer_id: *peer_id })
                .await;
            Ok(devices)
        }
    }
}

/// 改一台已配对设备的信任级别与收件策略（宿主唯一入口）。
///
/// 落盘由 [`update_policy`] 完成；节点在跑时还要把新值推进共享内存表，否则
/// 「策略已保存、本次运行仍按旧策略裁决入站 offer」——`swarmdrop_transfer::policy`
/// 读的是内存表那份。两端此前各写一遍这个 `if let Some(manager)`。
///
/// **不发 [`CoreEvent`]**：目前没有对应的事件变体，桌面靠自己的 `DevicesChanged` 广播
/// 刷新列表。要补的话是一条独立增量（新增变体会波及三端全部 event adapter 的穷尽 match），
/// 不在本函数的职责里。
pub async fn set_receive_policy<T>(
    peer_id: &NodeId,
    trust_level: DeviceTrustLevel,
    receive_policy: Option<DeviceReceivePolicy>,
    store: &dyn PairedDeviceStore,
    net: Option<&NetManager<T>>,
) -> AppResult<PairedDeviceInfo>
where
    T: TransferRuntime,
{
    let updated = update_policy(store, peer_id, trust_level, receive_policy).await?;
    if let Some(net) = net {
        net.pairing().add_paired_device(updated.clone());
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use swarmdrop_net::SecretKey;

    use crate::device::{DeviceTrustLevel, OsInfo, PairedDeviceInfo};
    use crate::host::{MemoryHost, PairedDeviceStore};
    use crate::network::NetManager;

    fn memory_host() -> MemoryHost {
        MemoryHost::new()
    }

    fn paired_device(name: &str) -> PairedDeviceInfo {
        PairedDeviceInfo::new(
            SecretKey::generate().node_id(),
            OsInfo {
                name: None,
                hostname: name.to_string(),
                os: "test".to_string(),
                platform: "test".to_string(),
                arch: "test".to_string(),
                capabilities: Vec::new(),
            },
            1,
        )
    }

    #[tokio::test]
    async fn upsert_paired_device_should_insert_then_replace() {
        let host = memory_host();
        let mut device = paired_device("first");
        let peer_id = device.peer_id;

        let devices = super::upsert(&host, device.clone()).await.unwrap();
        assert_eq!(devices.len(), 1);

        device.os_info.hostname = "second".to_string();
        device.trust_level = DeviceTrustLevel::Owned;
        let devices = super::upsert(&host, device).await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].peer_id, peer_id);
        assert_eq!(devices[0].os_info.hostname, "second");
        assert_eq!(devices[0].trust_level, DeviceTrustLevel::Collaborator);
    }

    /// upsert 的保留语义必须覆盖 `receive_policy`，而不只是 `trust_level`。
    ///
    /// 传进来的 `PairedDeviceInfo::new(...)` 恒为默认策略，整条替换会把用户收紧过的
    /// 收件策略静默放回默认——那是被 `swarmdrop_transfer::policy` 真正裁决的字段。
    #[tokio::test]
    async fn upsert_paired_device_should_keep_receive_policy() {
        let host = memory_host();
        let mut stored = paired_device("first");
        let peer_id = stored.peer_id;
        stored.trust_level = DeviceTrustLevel::Owned;
        stored.receive_policy.auto_accept = false;
        stored.receive_policy.max_transfer_bytes = Some(1024);
        stored.receive_policy.allow_relay_auto_accept = false;
        stored.trust_confirmed = false;
        host.save_paired_devices(&[stored]).await.unwrap();

        // 再次配对：回调构造的是默认策略的 PairedDeviceInfo::new(...)
        let repaired = PairedDeviceInfo::new(peer_id, OsInfo::default(), 2);
        let devices = super::upsert(&host, repaired).await.unwrap();

        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].trust_level, DeviceTrustLevel::Owned);
        assert!(!devices[0].receive_policy.auto_accept);
        assert_eq!(devices[0].receive_policy.max_transfer_bytes, Some(1024));
        assert!(!devices[0].receive_policy.allow_relay_auto_accept);
        assert!(!devices[0].trust_confirmed);
        assert_eq!(devices[0].paired_at, 2);
    }

    #[tokio::test]
    async fn update_paired_device_policy_should_confirm_trust() {
        let host = memory_host();
        let device = paired_device("first");
        let peer_id = device.peer_id;
        host.save_paired_devices(&[device]).await.unwrap();

        let updated = super::update_policy(&host, &peer_id, DeviceTrustLevel::Owned, None)
            .await
            .unwrap();

        assert_eq!(updated.trust_level, DeviceTrustLevel::Owned);
        assert!(updated.receive_policy.auto_accept);
        assert!(updated.trust_confirmed);
        // 返回值必须与落盘的那份同源，否则调用方拿到的是「还没保存的乐观值」。
        let persisted = host.load_paired_devices().await.unwrap();
        assert_eq!(persisted[0].trust_level, DeviceTrustLevel::Owned);
    }

    #[tokio::test]
    async fn update_paired_device_policy_should_reject_unknown_peer() {
        let host = memory_host();
        let unknown = SecretKey::generate().node_id();

        // 存在性检查只在这一处 —— 调用方不必也不该再 find 一遍。
        assert!(
            super::update_policy(&host, &unknown, DeviceTrustLevel::Owned, None)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn remove_paired_device_should_persist_filtered_list() {
        let host = memory_host();
        let first = paired_device("first");
        let second = paired_device("second");
        let first_peer = first.peer_id;

        host.save_paired_devices(&[first, second]).await.unwrap();

        let devices = super::remove(&host, &first_peer).await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_ne!(devices[0].peer_id, first_peer);
    }

    /// **回归锚点：节点未运行时解除配对必须发 `PairedDeviceRemoved`。**
    ///
    /// 这条路径上 core 的 event bus 不在场（`PairingManager` 根本没起），事件只能由本模块
    /// 补发。桌面此前在命令层手工补了，移动端忘了补 —— RN 侧于是收不到任何移除事件。
    /// 这条红了就说明那个分叉又回来了。
    #[tokio::test]
    async fn unpair_without_running_node_persists_and_publishes() {
        use crate::host::CoreEvent;

        let host = memory_host();
        let device = paired_device("first");
        let peer_id = device.peer_id;
        host.save_paired_devices(&[device]).await.unwrap();

        let devices = super::unpair(&peer_id, &host, &host, None::<&NetManager<()>>)
            .await
            .expect("节点未启动时解除配对必须成功");

        assert!(devices.is_empty());
        assert!(host.load_paired_devices().await.unwrap().is_empty());
        assert!(
            host.events().iter().any(|event| matches!(
                event,
                CoreEvent::PairedDeviceRemoved { peer_id: removed } if removed == &peer_id
            )),
            "节点没跑也要发 PairedDeviceRemoved —— 否则 UI 上设备不会消失"
        );
    }

    /// 幂等：列表里本来就没有该 peer 时是 no-op，**不发事件**
    /// （与 `PairingManager::unpair` 同一条不变量：事件 == 集合真的变了）。
    #[tokio::test]
    async fn unpair_without_running_node_is_idempotent_without_event() {
        let host = memory_host();
        let unknown = SecretKey::generate().node_id();

        let devices = super::unpair(&unknown, &host, &host, None::<&NetManager<()>>)
            .await
            .expect("no-op 不是错误");

        assert!(devices.is_empty());
        assert!(host.events().is_empty(), "没变就不能发事件");
    }

    /// 节点未运行时改策略只落盘，不报错——UI 在 `start` 之前就能改信任级别。
    #[tokio::test]
    async fn set_receive_policy_without_running_node_only_persists() {
        let host = memory_host();
        let device = paired_device("first");
        let peer_id = device.peer_id;
        host.save_paired_devices(&[device]).await.unwrap();

        let updated = super::set_receive_policy(
            &peer_id,
            DeviceTrustLevel::Owned,
            None,
            &host,
            None::<&NetManager<()>>,
        )
        .await
        .expect("节点未启动时改策略必须成功");

        assert_eq!(updated.trust_level, DeviceTrustLevel::Owned);
        assert_eq!(
            host.load_paired_devices().await.unwrap()[0].trust_level,
            DeviceTrustLevel::Owned
        );
    }
}
