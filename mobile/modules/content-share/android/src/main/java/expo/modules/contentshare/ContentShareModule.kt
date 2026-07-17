package expo.modules.contentshare

import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.core.net.toUri
import expo.modules.kotlin.modules.Module
import expo.modules.kotlin.modules.ModuleDefinition

/**
 * 零拷贝分享一个 content:// 文档（SAF 保存目录下的收件箱文件）。
 *
 * expo-sharing 只接受 file:// 并要求把文件复制进它的 FileProvider 可读范围，
 * SAF 文件走它必须先整份拷进 cache（大文件慢 + 缓存膨胀）。这里直接对已持有
 * 读权限的 document URI 发 ACTION_SEND：framework 的 migrateExtraStreamToClipData
 * 会把 EXTRA_STREAM 提升为 ClipData，配合 FLAG_GRANT_READ_URI_PERMISSION
 * 给目标应用临时读授权——与系统相册的分享是同一条路径。
 */
class ContentShareModule : Module() {
  override fun definition() = ModuleDefinition {
    Name("ContentShare")

    AsyncFunction("shareContentUri") { uri: String, fileName: String, dialogTitle: String? ->
      // MIME 从原始文件名的扩展名查（document URI 末段是编码后的 docId，取不出可读
      // 扩展名），查不到回退 */*——chooser 面板按通配收窄候选应用。用 MimeTypeMap 而非
      // URLConnection.guessContentTypeFromName：后者对含 '#' 的文件名(如 "C#笔记.pdf")
      // 有 AOSP StringIndexOutOfBounds 崩溃 bug。
      val extension = fileName.substringAfterLast('.', "").lowercase()
      val mimeType =
        MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension) ?: "*/*"
      val send = Intent(Intent.ACTION_SEND).apply {
        putExtra(Intent.EXTRA_STREAM, uri.toUri())
        setTypeAndNormalize(mimeType)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      val chooser = Intent.createChooser(send, dialogTitle).apply {
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      appContext.throwingActivity.startActivity(chooser)
    }
  }
}
