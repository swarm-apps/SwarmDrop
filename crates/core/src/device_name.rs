//! 改名编排：落盘 → 本机 `OsInfo` → identify `agent_version` → 事件。
//!
//! 放 core 的自由函数而不是 [`NetManager`] 的方法，是因为它要答的第二个问题是
//! 「**节点没起时怎么改名**」——那时根本没有 `NetManager`。挂成方法，三端就得各写一遍
//! `if let Some(m) = manager { … } else { device_config.save(…) }`，而消灭三端各写一遍
//! 的编排正是本模块的立意。收 `Option<&NetManager<T>>` 则两条分支都留在 core 内，
//! 宿主只有一次调用（三端持有的本来就是 `Option`：桌面 `Mutex<Option<NetManager>>`、
//! 移动 `Mutex<Option<NetManager<…>>>`、Web 恒 `Some`）。

use crate::AppResult;
use crate::device::DeviceName;
use crate::host::{CoreEvent, DeviceConfig, EventBus};
use crate::network::{NetManager, TransferRuntime};

/// 改本机设备名：写盘 + 让已连接的对端立刻看到 + 通知本端各处 UI。
///
/// `name = None`（或归一化后为空）表示清空，回退到系统 hostname。归一化由
/// [`DeviceName::parse`] 在更外层完成——本函数只认已归一化的值。
///
/// `net = None` 表示节点尚未启动（onboarding，或设置页早于 `start`）：只落盘，
/// 不推网络，返回 `Ok`。新名字会在节点下次启动时由组合根从 `DeviceConfig` 端口读回。
///
/// # 三步顺序不可调换
///
/// 1. `device_config.save_device_name` —— **失败即整体失败，后续一步都不做**
/// 2. [`NetManager::set_local_device_name`] —— 内存态（配对请求 / 新邀请的 display 名字
///    来源）+ identify 的 `agent_version`，**这两半在那里是一次调用**，本函数不再分开做
/// 3. publish [`CoreEvent::DeviceRenamed`]
///
/// **持久化必须在最前。** 反过来（先广播再落盘）一旦落盘失败，用户看到的是「改成功了」，
/// 下次启动却变回旧名字——**名字自己回滚**是最难向用户解释的状态。当前顺序的失败模式则是
/// 可自愈的：第 1 步成功、第 2 步失败（现实中只可能是 actor 已关停），持久化已生效，节点
/// 下次启动自然带上新名字。原子性做不到时，把不可自愈的那一步放最前是次优解。
pub async fn rename_device<T: TransferRuntime>(
    name: Option<DeviceName>,
    device_config: &dyn DeviceConfig,
    events: &dyn EventBus,
    net: Option<&NetManager<T>>,
) -> AppResult<()> {
    // ① 落盘。失败即整体失败，不推网络。
    device_config.save_device_name(name.clone()).await?;

    let display_name = match net {
        // ② 内存态 + identify 一次做完（成对性由 NetManager 那侧保证）。
        Some(net) => net
            .set_local_device_name(name.clone())
            .await?
            .display_name(),
        // 节点未启动：core 没有 hostname 可回退（它的唯一装配点在 `runtime::start_node`），
        // 于是 display_name 退化成名字本身；清空时为空串，调用方按自己那套 hostname 展示。
        None => name
            .as_ref()
            .map(|n| n.as_str().to_string())
            .unwrap_or_default(),
    };

    // ③ 通知本端各处消费者（多窗口 / MCP / 设置页与设备卡片）。
    events
        .publish(CoreEvent::DeviceRenamed {
            name: name.map(DeviceName::into_string),
            display_name,
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::rename_device;
    use crate::device::DeviceName;
    use crate::host::{CoreEvent, DeviceConfig, MemoryHost};
    use crate::network::NetManager;

    /// onboarding 路径：节点还没起，改名只落盘、不报错，事件照发。
    ///
    /// 泛型参数只影响 `net = None` 那条分支取不到的类型，随便给一个满足
    /// `TransferRuntime` 的即可（`()` 有空实现）。
    #[tokio::test]
    async fn rename_without_running_node_only_persists() {
        let host = MemoryHost::new();
        let name = DeviceName::parse("小李的 MacBook").expect("非空");

        rename_device(Some(name.clone()), &host, &host, None::<&NetManager<()>>)
            .await
            .expect("节点未启动时改名必须成功");

        assert_eq!(
            host.load_device_name()
                .await
                .as_ref()
                .map(DeviceName::as_str),
            Some("小李的 MacBook"),
            "名字必须已落盘——节点下次启动要从这里读回来"
        );
        assert!(
            host.events().iter().any(|event| matches!(
                event,
                CoreEvent::DeviceRenamed { name, display_name }
                    if name.as_deref() == Some("小李的 MacBook")
                        && display_name == "小李的 MacBook"
            )),
            "即使没有节点也要发 DeviceRenamed —— onboarding 回显与设置页都靠它"
        );
    }

    /// 清空同样走「只落盘」分支：`None` 是合法值，不是错误。
    #[tokio::test]
    async fn clearing_without_running_node_is_ok() {
        let host = MemoryHost::new();
        host.save_device_name(DeviceName::parse("旧名字"))
            .await
            .expect("预置旧名字");

        rename_device(None, &host, &host, None::<&NetManager<()>>)
            .await
            .expect("清空必须成功");

        assert!(
            host.load_device_name().await.is_none(),
            "清空后端口里不应再有名字"
        );
    }
}
