import ExpoModulesCore

/// 应用私有数据区的路径解析（iOS）。
///
/// 存在的理由是 expo-file-system 的 `Paths` 只暴露 `cache` / `bundle` / `document` /
/// `appleSharedContainers`，**没有 Application Support**。而 iOS 一旦开启文件共享
/// （`UIFileSharingEnabled` + `LSSupportsOpeningDocumentsInPlace`），`Documents` 整个目录
/// 就对用户可见——数据库与接收暂存不能再留在那里。
///
/// **不要改成从 `Paths.document.uri` 做字符串替换**。那等于把「`Documents` 与
/// `Library/Application Support` 是兄弟目录」这一容器布局实现细节当成契约（Apple 从未
/// 承诺它稳定），而且失败是静默的：路径算错就在一个不存在的位置建库，用户看到的是
/// 「数据全没了」而不是一条错误。
public class AppPathsModule: Module {
  public func definition() -> ModuleDefinition {
    Name("AppPaths")

    // 返回 `file://` URI 而非裸路径：与 `Paths.document.uri` 形态一致，Rust 侧
    // `parse_host_dir` 统一剥前缀 + percent-decode。
    //
    // ⚠️ Application Support 的路径**含空格**（`…/Library/Application Support`）。
    // `absoluteString` 会把它编码成 `%20`，正好由 `parse_host_dir` 的 percent-decode
    // 还原。换成 `path` 返回裸路径也能工作（decode 对无 `%` 的串幂等），但那样两端
    // 的形态就不一致了，日后谁改一边都可能踩空。
    Function("applicationSupportDirectory") { () -> String in
      let fileManager = FileManager.default
      guard
        let url = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
      else {
        throw ApplicationSupportUnavailableException()
      }
      // iOS 上 Application Support **默认不存在**，必须显式创建。漏掉这步的表现同样
      // 是静默失败：SQLite 会在一个不存在的父目录下开库。
      try fileManager.createDirectory(at: url, withIntermediateDirectories: true)
      return url.absoluteString
    }
  }
}

internal final class ApplicationSupportUnavailableException: Exception {
  override var reason: String {
    "无法定位 Application Support 目录"
  }
}
