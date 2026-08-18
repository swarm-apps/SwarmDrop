//! 设备列表渲染。

use serde_json::Value;

pub fn render(devices: &Value, json: bool) {
    if json {
        match serde_json::to_string_pretty(devices) {
            Ok(text) => println!("{text}"),
            Err(err) => eprintln!("序列化设备列表失败: {err}"),
        }
        return;
    }

    // 服务端可能回一个数组，也可能回一个带 `devices` 字段的对象——两种都收，
    // 因为这层通道是内部的，形状随命令面演进，渲染不该为此失败。
    let list = devices
        .as_array()
        .or_else(|| devices.get("devices").and_then(Value::as_array));

    let Some(list) = list else {
        println!("（无法解析设备列表）");
        return;
    };

    if list.is_empty() {
        println!("尚无已配对设备。执行 swarmdrop pair 生成邀请。");
        return;
    }

    for device in list {
        let name = device
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| device.pointer("/osInfo/name").and_then(Value::as_str))
            .or_else(|| device.pointer("/osInfo/hostname").and_then(Value::as_str))
            .unwrap_or("（未命名）");
        let id = device
            .get("peerId")
            .and_then(Value::as_str)
            .unwrap_or("（无标识）");
        let online = device
            .get("isOnline")
            .and_then(Value::as_bool)
            .or_else(|| device.get("online").and_then(Value::as_bool));
        let mark = match online {
            Some(true) => "●",
            Some(false) => "○",
            None => " ",
        };
        println!("{mark} {name}");
        println!("   {id}");
    }
}
