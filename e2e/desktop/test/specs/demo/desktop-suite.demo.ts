// 录制主片入口：同一 WDIO worker 中顺序注册三个独立场景，确保 Tauri 应用只启动一次。
// 各场景仍可单独运行，方便补录；本文件只负责批量录制时的生命周期收拢。
import "./desktop-home.demo";
import "./send-file.demo";
import "./inbox.demo";
