import {
  type CheckContext,
  type DownloadHandle,
  type KeyValueStorage,
  type Progress,
  type ReleaseInfo,
  semverComparator,
  type UpdateAdapter,
  type UpgradeType,
} from "@swarm-hive/sdk";
import { relaunch } from "@tauri-apps/plugin-process";
import { LazyStore } from "@tauri-apps/plugin-store";
import { type CheckOptions, check, type Update } from "@tauri-apps/plugin-updater";

const VALID_UPGRADE_TYPES: readonly string[] = ["prompt", "force", "silent"];

/** server 在 Tauri 动态 update JSON 里附带的 swarmhive 元数据(rawJson.swarmhive)。 */
interface SwarmHiveMeta {
  upgrade_type?: string;
  min_version?: string | null;
  rollout_percent?: number;
  channel?: string;
}

/** 把 plugin-updater 的 Update 归一化成 SDK ReleaseInfo(对齐 sdk 的 normalizeTauri)。 */
function normalize(update: Update): ReleaseInfo {
  const raw = (update.rawJson ?? {}) as Record<string, unknown>;
  const sh = (raw.swarmhive ?? {}) as SwarmHiveMeta;
  const ut = sh.upgrade_type ?? "prompt";
  return {
    version: update.version,
    url: typeof raw.url === "string" ? raw.url : "",
    signature: typeof raw.signature === "string" ? raw.signature : undefined,
    // plugin-updater 把 wire 的 notes 映射到 update.body、pub_date 映射到 update.date。
    notes: update.body ?? undefined,
    pubDate: update.date ?? undefined,
    upgradeType: VALID_UPGRADE_TYPES.includes(ut) ? (ut as UpgradeType) : "prompt",
    minVersion: sh.min_version ?? undefined,
    rolloutPercent: sh.rollout_percent,
    channel: sh.channel ?? "stable",
  };
}

/**
 * 下载进度 + 瞬时速度跟踪(500ms 节流)。放在 adapter(平台 UI 关注点),**不进 sdk-core**。
 * 搬自 SwarmDrop 桌面端的速度计算。
 */
class DownloadSpeedTracker {
  private total = 0;
  private downloaded = 0;
  private lastEmitAt = 0;
  private lastEmitBytes = 0;
  private speed = 0;

  constructor(
    private readonly onProgress: (p: Progress) => void,
    private readonly throttleMs = 500,
  ) {}

  started(contentLength: number | undefined): void {
    this.total = contentLength ?? 0;
    this.downloaded = 0;
    this.lastEmitAt = Date.now();
    this.lastEmitBytes = 0;
    this.speed = 0;
    this.emit();
  }

  progress(chunkLength: number): void {
    this.downloaded += chunkLength;
    const now = Date.now();
    const dt = now - this.lastEmitAt;
    if (dt >= this.throttleMs) {
      this.speed = ((this.downloaded - this.lastEmitBytes) * 1000) / dt;
      this.lastEmitAt = now;
      this.lastEmitBytes = this.downloaded;
      this.emit();
    }
  }

  finished(): void {
    // 服务端没给 contentLength 时,用已下载量兜底,保证最终 percent = 1。
    if (this.total === 0) this.total = this.downloaded;
    this.emit(1);
  }

  private emit(percentOverride?: number): void {
    const percent = percentOverride ?? (this.total > 0 ? this.downloaded / this.total : 0);
    this.onProgress({
      downloaded: this.downloaded,
      total: this.total,
      percent,
      speed: this.speed || undefined,
    });
  }
}

/** 用 @tauri-apps/plugin-store(LazyStore)实现 SDK 的 KeyValueStorage。 */
function createTauriStorage(storeFileName: string): KeyValueStorage {
  const store = new LazyStore(storeFileName);
  return {
    async get(key: string): Promise<string | null> {
      const value = await store.get<string>(key);
      return value ?? null;
    },
    async set(key: string, value: string): Promise<void> {
      await store.set(key, value);
      await store.save();
    },
  };
}

export interface TauriAdapterOptions {
  /** plugin-store 持久化文件名,默认 `swarmhive-update.json`。 */
  storeFileName?: string;
  /** 透传给 plugin-updater `check()` 的额外项(proxy / timeout / target / allowDowngrades);headers 由 adapter 接管。 */
  checkOptions?: Omit<CheckOptions, "headers">;
}

/**
 * 创建 Tauri 平台的 UpdateAdapter。endpoint(含 {{target}}/{{arch}}/{{current_version}}
 * 占位)在 `tauri.conf.json` 的 `plugins.updater.endpoints` 配置,本 adapter 不重复拼 URL。
 */
export function createTauriAdapter(opts: TauriAdapterOptions = {}): UpdateAdapter {
  const storage = createTauriStorage(opts.storeFileName ?? "swarmhive-update.json");
  // 平台句柄不可序列化,缓存进闭包(复刻 SwarmDrop 的 _pendingDesktopUpdate);
  // check 写、download/install 读。
  let pending: Update | null = null;

  return {
    storage,
    compare: semverComparator,

    async check(ctx: CheckContext): Promise<ReleaseInfo | null> {
      const update = await check({
        ...opts.checkOptions,
        headers: { "X-Client-Id": ctx.clientId },
      });
      pending = update;
      return update ? normalize(update) : null;
    },

    async download(
      release: ReleaseInfo,
      onProgress: (p: Progress) => void,
    ): Promise<DownloadHandle> {
      if (!pending) {
        throw new Error("no pending update — call check() before download()");
      }
      const tracker = new DownloadSpeedTracker(onProgress);
      await pending.download((event) => {
        switch (event.event) {
          case "Started":
            tracker.started(event.data.contentLength);
            break;
          case "Progress":
            tracker.progress(event.data.chunkLength);
            break;
          case "Finished":
            tracker.finished();
            break;
        }
      });
      return { release, payload: pending };
    },

    async install(handle: DownloadHandle): Promise<void> {
      const update = (handle.payload as Update | undefined) ?? pending;
      if (!update) {
        throw new Error("no pending update — call download() before install()");
      }
      await update.install();
      // Tauri 安装后不会自动重启,显式 relaunch 让新版本生效。
      await relaunch();
    },
  };
}
