/**
 * 文件浏览器的统一展示模型 —— 三端共用一份。
 *
 * 收敛前三端各有一份 `FileBrowserItem`，分歧不只是字段名：移动端 `size` 是 `bigint`
 * （uniffi 的产物）、预览源叫 `localUri`，桌面叫 `previewUrl` 且少两个状态。
 * 这里取并集，**分歧在各端 adapter 的边界解决**，不在表现层。
 */

/** 文件在浏览器里的呈现状态。取三端并集——`paused` / `cancelled` 此前只有移动端有。 */
export type FileBrowserStatus =
  | "idle"
  | "waiting"
  | "transferring"
  | "paused"
  | "completed"
  | "cancelled"
  | "error"
  | "missing";

export type FileBrowserView = "tree" | "grid";

/** 视图偏好按场景分别记忆；三端共用这一组 key，别各写各的字符串。 */
export type FileBrowserScope = "send" | "transfer" | "inbox";

export interface FileBrowserItem {
  /** 全局唯一的展示 ID，由 `identity.ts` 的三个构造器生成（scope 前缀 + 业务键）。 */
  id: string;
  /**
   * 调用方原始记录 ID，用于把显式操作映射回业务命令。
   *
   * **恒为 string**。曾是 `string | number`（收件箱那一路是行主键、发送侧是来源路径），
   * 于是每个消费点都得自己 `Number(...)` / `String(...)` 收窄一次——而这类转换**没有
   * 反馈回路**：漏了就是运行时的 `NaN`，编译期一声不吭。这条已经收过一次账
   * （`fromInboxFiles` 一度漏给 `sourceId`，桌面的「打开」变成 `Number(undefined)`）。
   * 需要数字的端在**自己的 adapter 里**解码一次，不散在渲染点。
   */
  sourceId?: string;
  fileId?: number;
  name: string;
  relativePath: string;
  /**
   * 字节数。**统一为 `number`**：移动端的 `bigint` 在它自己的 adapter 里 `Number(...)` 转。
   * 文件大小不会触到 `Number.MAX_SAFE_INTEGER`（9 PB）。
   */
  size: number;
  status: FileBrowserStatus;
  /** 0–100。只在 `transferring` / `paused` 时有意义。 */
  progress?: number;
  /**
   * **可直接渲染的**预览 URL。桌面走这条：`convertFileSrc` 给的 asset URL 本身就能当
   * `<img src>`，资源协议自己会流式读并降采样，没有再过一遍缩放管线的必要。
   *
   * 与下面的 `previewSource` **是两件事，不是两个名字**。它们曾被合成一个字段，代价是
   * 表现层只能靠「调用方有没有传取图源函数」反推自己拿到的是哪一种——那等于让组件去猜
   * 它跑在哪一端，而类型系统一个字都表达不出来。
   */
  previewUrl?: string;
  /**
   * **需要解析的**取图源，语义按端不同：Web 是 OPFS 相对路径、移动是 `file://` URI。
   * 由各端注入的取图源函数换成字节，再经缩略图管线缩放。
   *
   * L1 与 L2 都不解释它的内容，组件也不会从它自行拼 URL——那条边界在桌面时代就立过
   * （见 `dev-notes/knowledge/file-browser.md` 的「操作与安全边界」）。
   */
  previewSource?: string;
}

/* ─── 树结构（中立形态） ─── */

/**
 * 建树的中立产物：**嵌套树**。
 *
 * 两端消费方式不同，所以这里不迁就任何一方：桌面在 L2 里把它投影成 `@headless-tree`
 * 要的扁平 `Map` + `dataLoader`，移动端用自己的 `flattenVisibleNodes` 摊成 FlashList 的行。
 */
export interface FileBrowserDirectoryNode {
  type: "directory";
  id: string;
  name: string;
  /** 带尾斜杠——目录类操作（如批量移除）用它做前缀匹配。 */
  relativePath: string;
  /** 根的直接子节点为 0。 */
  depth: number;
  /** 递归计数：含所有后代文件。 */
  fileCount: number;
  /** 递归求和：含所有后代文件。 */
  size: number;
  children: FileBrowserTreeNode[];
}

export interface FileBrowserFileNode {
  type: "file";
  id: string;
  depth: number;
  item: FileBrowserItem;
}

export type FileBrowserTreeNode = FileBrowserDirectoryNode | FileBrowserFileNode;

/**
 * 摊平后的一行。
 *
 * 目录节点在这里**不带 `children`**——列表行携带整棵子树会让虚拟列表的 diff 变成深比较，
 * 一个几百文件的会话里那是每帧几百次无谓的对象遍历。层级关系已经由 `depth` 表达，
 * 行本身不需要再持有它。
 */
export type FileBrowserTreeRow =
  | Omit<FileBrowserDirectoryNode, "children">
  | FileBrowserFileNode;

export interface FileBrowserTree {
  /** 根的直接子节点（根本身不是节点——它没有可展示的名字）。 */
  roots: FileBrowserTreeNode[];
  /** 所有目录节点的 id，供各端初始化展开态。 */
  directoryIds: ReadonlySet<string>;
  /** 全部文件的字节总和。 */
  totalSize: number;
  /** 全部文件数（不含目录）。 */
  totalCount: number;
}
