import { describe, expect, it } from "vitest";
import {
  fromInboxFiles,
  fromOfferFiles,
  fromProjectionFiles,
  fromSelectedFiles,
  type ProgressFileInput,
  type ProjectionFileInput,
} from "./adapters";

const FILES: ProjectionFileInput[] = [
  { fileId: 0, name: "README.md", relativePath: "README.md", size: 2300, transferredBytes: 0 },
  { fileId: 1, name: "photo.jpg", relativePath: "img/photo.jpg", size: 7_600_000, transferredBytes: 0 },
];

describe("fromProjectionFiles —— 不变量 1：progress 是覆盖层，不是替代品", () => {
  it("条目的身份与数量只由 projection 决定，progress 多出来的 fileId 不会新增行", () => {
    // 一条属于**别的会话**的进度混进来（Web 端串会话那个 bug 的形状）。
    const progress: ProgressFileInput[] = [
      { fileId: 0, transferred: 2300, status: "completed" },
      { fileId: 99, transferred: 123, status: "completed" },
    ];

    const items = fromProjectionFiles("s1", FILES, { phase: "active", progress });

    expect(items).toHaveLength(2);
    expect(items.map((i) => i.name)).toEqual(["README.md", "photo.jpg"]);
  });

  it("反复用不同会话的进度调用，输出互不污染", () => {
    const a = fromProjectionFiles("s1", FILES, {
      phase: "active",
      progress: [{ fileId: 0, transferred: 2300, status: "completed" }],
    });
    const b = fromProjectionFiles("s2", FILES, {
      phase: "active",
      progress: [{ fileId: 1, transferred: 7_600_000, status: "completed" }],
    });
    const aAgain = fromProjectionFiles("s1", FILES, {
      phase: "active",
      progress: [{ fileId: 0, transferred: 2300, status: "completed" }],
    });

    expect(a).toHaveLength(2);
    expect(b).toHaveLength(2);
    // 切回来必须与第一次完全一致——条目数不随调用次数增长。
    expect(aAgain).toEqual(a);
    expect(a[0].id).not.toBe(b[0].id);
  });

  it("progress 只覆盖字节与状态，名称/大小/路径仍来自 projection", () => {
    const items = fromProjectionFiles("s1", FILES, {
      phase: "active",
      progress: [{ fileId: 1, transferred: 3_800_000, status: "transferring" }],
    });

    const photo = items[1];
    expect(photo.name).toBe("photo.jpg");
    expect(photo.size).toBe(7_600_000);
    expect(photo.relativePath).toBe("img/photo.jpg");
    expect(photo.status).toBe("transferring");
    expect(photo.progress).toBe(50);
  });
});

describe("fromProjectionFiles —— 不变量 2：终态忽略进度", () => {
  it("终态会话不读残留的进度快照", () => {
    // 会话已结束，但 store 里还留着它跑到一半时的进度事件。
    const stale: ProgressFileInput[] = [
      { fileId: 0, transferred: 1150, status: "transferring" },
    ];

    const items = fromProjectionFiles("s1", FILES, {
      phase: "terminal",
      terminalReason: "cancelled",
      progress: stale,
    });

    // 若读了 stale，第一条会是 transferring + 50%。
    expect(items[0].status).toBe("cancelled");
    expect(items[0].progress).toBeUndefined();
  });

  it("判定在函数内部，调用方不传 progress 也是同一结果", () => {
    const withStale = fromProjectionFiles("s1", FILES, {
      phase: "terminal",
      terminalReason: "completed",
      progress: [{ fileId: 0, transferred: 1150, status: "transferring" }],
    });
    const without = fromProjectionFiles("s1", FILES, {
      phase: "terminal",
      terminalReason: "completed",
    });

    expect(withStale).toEqual(without);
  });
});

describe("fromProjectionFiles —— 状态映射", () => {
  it("内核给的逐文件状态优先于按阶段推断", () => {
    const items = fromProjectionFiles("s1", FILES, {
      phase: "active",
      progress: [{ fileId: 0, transferred: 0, status: "completed" }],
    });
    expect(items[0].status).toBe("completed");
  });

  it("字节数已满即视为完成，即使会话仍在进行", () => {
    const done: ProjectionFileInput[] = [{ ...FILES[0], transferredBytes: 2300 }];
    expect(fromProjectionFiles("s1", done, { phase: "active" })[0].status).toBe("completed");
  });

  it("suspended 且传了一半 → paused；一点没传 → waiting", () => {
    const partial: ProjectionFileInput[] = [{ ...FILES[0], transferredBytes: 1150 }];
    expect(fromProjectionFiles("s1", partial, { phase: "suspended" })[0].status).toBe("paused");
    expect(fromProjectionFiles("s1", FILES, { phase: "suspended" })[0].status).toBe("waiting");
  });

  it("过期与拒绝跟取消同档，不算 error", () => {
    for (const reason of ["cancelled", "rejected", "expired"] as const) {
      const items = fromProjectionFiles("s1", FILES, { phase: "terminal", terminalReason: reason });
      expect(items[0].status).toBe("cancelled");
    }
    const failed = fromProjectionFiles("s1", FILES, {
      phase: "terminal",
      terminalReason: "fatal_error",
    });
    expect(failed[0].status).toBe("error");
  });

  it("等待对方接受的阶段一律 waiting", () => {
    for (const phase of ["offered", "waiting_accept"] as const) {
      expect(fromProjectionFiles("s1", FILES, { phase })[0].status).toBe("waiting");
    }
  });
});

describe("其余三个来源", () => {
  it("发送侧已选文件恒为 idle，ID 按来源键构造", () => {
    const items = fromSelectedFiles([
      { sourceId: "/tmp/a.txt", name: "a.txt", relativePath: "a.txt", size: 10 },
    ]);
    expect(items[0].status).toBe("idle");
    expect(items[0].id).toBe("source:/tmp/a.txt");
  });

  it("offer 里的目录条目被丢掉——层级由建树从路径派生", () => {
    const items = fromOfferFiles("s1", [
      { fileId: 0, name: "img", relativePath: "img", size: 0, isDirectory: true },
      { fileId: 1, name: "photo.jpg", relativePath: "img/photo.jpg", size: 100 },
    ]);
    expect(items).toHaveLength(1);
    expect(items[0].name).toBe("photo.jpg");
  });

  // offer 是「要不要收」的决策依据，不是进行中的传输。挂等待图标会暗示传输已经开始。
  it("offer 的状态是 idle 而不是 waiting", () => {
    const items = fromOfferFiles("s1", [
      { fileId: 1, name: "photo.jpg", relativePath: "photo.jpg", size: 100 },
    ]);
    expect(items[0].status).toBe("idle");
  });

  it("缺失的收件箱文件不给取图源", () => {
    const items = fromInboxFiles("item-1", [
      { id: 1, name: "gone.jpg", relativePath: "gone.jpg", size: 100, missing: true, previewSource: "opfs:/gone.jpg" },
      { id: 2, name: "here.jpg", relativePath: "here.jpg", size: 100, previewSource: "opfs:/here.jpg" },
    ]);
    expect(items[0].status).toBe("missing");
    expect(items[0].previewSource).toBeUndefined();
    expect(items[1].previewSource).toBe("opfs:/here.jpg");
  });

  // 宿主动作（打开 / 在文件夹中显示）按 sourceId 定位，它是收件箱文件行主键；
  // fileId 是传输侧的号。两者混用会打开另一个文件。sourceId 恒为 string，
  // 需要数字的端在自己的 adapter / 调用点解码一次。
  it("收件箱条目的 sourceId 是行主键，与 transferFileId 分开", () => {
    const items = fromInboxFiles("item-1", [
      { id: 42, transferFileId: 3, name: "a.txt", relativePath: "a.txt", size: 1 },
    ]);
    expect(items[0].sourceId).toBe("42");
    expect(items[0].fileId).toBe(3);
  });

  it("offer 与传输投影对同一个 fileId 给出同一个展示 ID", () => {
    const offered = fromOfferFiles("s1", [{ fileId: 7, name: "x", relativePath: "x", size: 1 }]);
    const active = fromProjectionFiles(
      "s1",
      [{ fileId: 7, name: "x", relativePath: "x", size: 1, transferredBytes: 0 }],
      { phase: "active" },
    );
    expect(offered[0].id).toBe(active[0].id);
  });
});
