import installer from "./NativeSwarmdropCore";
import {
  MobileCore as NativeMobileCore,
  type ForeignEventBus,
  type ForeignKeychainProvider,
  type MobileCoreEvent,
  MobileCoreEvent_Tags,
  type MobileDevice,
  type MobileIdentity,
  type MobileNetworkStatus,
  type MobilePairingCode,
  type MobilePairingResult,
  type MobilePreparedTransfer,
  type MobileRemoteDeviceInfo,
  type MobileTransferFile,
  type MobileTransferOffer,
  type MobileTransferSession,
} from "./generated/swarmdrop_mobile_core";
import * as swarmdrop_mobile_core from "./generated/swarmdrop_mobile_core";

let rustInstalled = false;
if (!rustInstalled) {
  installer.installRustCrate();
  rustInstalled = true;
}

let initialized = false;
if (!initialized) {
  swarmdrop_mobile_core.default.initialize();
  initialized = true;
}

export { MobileCoreEvent_Tags };
export type { ForeignEventBus, ForeignKeychainProvider, MobileCoreEvent };

export type DeviceIdentity = MobileIdentity;

export type NetworkStatus = Omit<
  MobileNetworkStatus,
  "status" | "peerId" | "publicAddr" | "connectedPeers" | "discoveredPeers"
> & {
  status: "stopped" | "running";
  peerId: string | null;
  publicAddr: string | null;
  connectedPeers: number;
  discoveredPeers: number;
};

export type DeviceInfo = Omit<MobileDevice, "connection"> & {
  connection: "lan" | "dcutr" | "relay" | null;
};

export type PairingCode = MobilePairingCode;
export type RemoteDeviceInfo = MobileRemoteDeviceInfo;
export type PairingResult = MobilePairingResult;

export type TransferFile = Omit<MobileTransferFile, "relativePath" | "size"> & {
  relativePath: string | null;
  size: number;
};

export type PreparedTransfer = Omit<MobilePreparedTransfer, "totalSize" | "files"> & {
  totalSize: number;
  files: TransferFile[];
};

export type TransferSession = Omit<MobileTransferSession, "direction" | "status" | "totalSize" | "completedSize"> & {
  direction: "incoming" | "outgoing";
  status:
    | "waitingForRuntime"
    | "pending"
    | "accepted"
    | "rejected"
    | "cancelled"
    | "completed"
    | "failed";
  totalSize: number;
  completedSize: number;
};

export type TransferOffer = Omit<MobileTransferOffer, "files" | "totalSize"> & {
  files: Array<Omit<TransferFile, "uri">>;
  totalSize: number;
};

export type KeychainProvider = ForeignKeychainProvider;
export type EventBus = ForeignEventBus;

export type MobileCorePort = {
  initializeIdentity(): Promise<DeviceIdentity>;
  startNode(customBootstrapNodes?: string[]): Promise<void>;
  shutdownNode(): Promise<void>;
  networkStatus(): Promise<NetworkStatus>;
  listDevices(filter?: "all" | "connected" | "paired"): Promise<DeviceInfo[]>;
  generatePairingCode(expiresInSecs: number): Promise<PairingCode>;
  lookupDeviceByCode(code: string): Promise<RemoteDeviceInfo>;
  requestPairing(peerId: string, code: string | null, addrs: string[]): Promise<PairingResult>;
  respondPairingRequest(pendingId: number, code: string | null, accept: boolean): Promise<void>;
  prepareSend(files: TransferFile[]): Promise<PreparedTransfer>;
  sendPrepared(
    preparedId: string,
    peerId: string,
    fileIds?: string[],
  ): Promise<TransferSession>;
  acceptReceive(sessionId: string, destinationUri: string): Promise<TransferSession>;
  rejectReceive(sessionId: string): Promise<void>;
  cancelTransfer(sessionId: string): Promise<void>;
  transferSessions(): Promise<TransferSession[]>;
  transferSession(sessionId: string): Promise<TransferSession | null>;
};

export async function uniffiInitAsync(): Promise<void> {
  // Native JSI bindings are initialized at module load.
}

export async function initializeIdentity(): Promise<DeviceIdentity> {
  const core = createUnavailableMobileCore();
  return core.initializeIdentity();
}

export function createMobileCore(
  keychain: ForeignKeychainProvider,
  eventBus: ForeignEventBus,
): MobileCorePort {
  const native = new NativeMobileCore(keychain, eventBus);
  return wrapNativeCore(native);
}

export function createUnavailableMobileCore(): MobileCorePort {
  return {
    async initializeIdentity() {
      return {
        peerId: "native-bridge-pending",
        created: false,
      };
    },
    async startNode() {},
    async shutdownNode() {},
    async networkStatus() {
      return {
        status: "stopped",
        peerId: null,
        listenAddrs: [],
        natStatus: "unknown",
        publicAddr: null,
        connectedPeers: 0,
        discoveredPeers: 0,
        relayReady: false,
        relayPeers: [],
        bootstrapConnected: false,
      };
    },
    async listDevices() {
      return [];
    },
    async generatePairingCode() {
      throw new Error("native bridge is not generated yet");
    },
    async lookupDeviceByCode() {
      throw new Error("native bridge is not generated yet");
    },
    async requestPairing() {
      throw new Error("native bridge is not generated yet");
    },
    async respondPairingRequest() {
      throw new Error("native bridge is not generated yet");
    },
    async prepareSend(files) {
      return {
        preparedId: "native-bridge-pending",
        totalSize: files.reduce((sum, file) => sum + file.size, 0),
        files,
      };
    },
    async sendPrepared() {
      throw new Error("native bridge is not generated yet");
    },
    async acceptReceive() {
      throw new Error("native bridge is not generated yet");
    },
    async rejectReceive() {
      throw new Error("native bridge is not generated yet");
    },
    async cancelTransfer() {
      throw new Error("native bridge is not generated yet");
    },
    async transferSessions() {
      return [];
    },
    async transferSession() {
      return null;
    },
  };
}

function wrapNativeCore(native: NativeMobileCore): MobileCorePort {
  return {
    initializeIdentity: () => native.initializeIdentity(),
    startNode: (customBootstrapNodes = []) => native.startNode(customBootstrapNodes),
    shutdownNode: () => native.shutdownNode(),
    networkStatus: async () => toNetworkStatus(await native.networkStatus()),
    listDevices: async (filter = "all") => (await native.listDevices(filter)).map(toDeviceInfo),
    generatePairingCode: (expiresInSecs) => native.generatePairingCode(BigInt(expiresInSecs)),
    lookupDeviceByCode: (code) => native.lookupDeviceByCode(code),
    requestPairing: (peerId, code, addrs) => native.requestPairing(peerId, code ?? undefined, addrs),
    respondPairingRequest: (pendingId, code, accept) =>
      native.respondPairingRequest(BigInt(pendingId), code ?? undefined, accept),
    prepareSend: async (files) => toPreparedTransfer(await native.prepareSend(files.map(toNativeFile))),
    sendPrepared: async (preparedId, peerId, fileIds = []) =>
      toTransferSession(await native.sendPrepared(preparedId, peerId, fileIds)),
    acceptReceive: async (sessionId, destinationUri) =>
      toTransferSession(await native.acceptReceive(sessionId, destinationUri)),
    rejectReceive: (sessionId) => native.rejectReceive(sessionId),
    cancelTransfer: (sessionId) => native.cancelTransfer(sessionId),
    transferSessions: async () => (await native.transferSessions()).map(toTransferSession),
    transferSession: async (sessionId) => {
      const session = await native.transferSession(sessionId);
      return session === undefined ? null : toTransferSession(session);
    },
  };
}

function toNetworkStatus(status: MobileNetworkStatus): NetworkStatus {
  return {
    ...status,
    status: status.status === "running" ? "running" : "stopped",
    peerId: status.peerId ?? null,
    publicAddr: status.publicAddr ?? null,
    connectedPeers: Number(status.connectedPeers),
    discoveredPeers: Number(status.discoveredPeers),
  };
}

function toDeviceInfo(device: MobileDevice): DeviceInfo {
  return {
    ...device,
    connection: toConnection(device.connection),
  };
}

function toNativeFile(file: TransferFile): MobileTransferFile {
  return {
    ...file,
    relativePath: file.relativePath ?? undefined,
    size: BigInt(file.size),
  };
}

function toPreparedTransfer(prepared: MobilePreparedTransfer): PreparedTransfer {
  return {
    ...prepared,
    totalSize: Number(prepared.totalSize),
    files: prepared.files.map((file) => ({
      ...file,
      relativePath: file.relativePath ?? null,
      size: Number(file.size),
    })),
  };
}

function toTransferSession(session: MobileTransferSession): TransferSession {
  return {
    ...session,
    direction: session.direction === "incoming" ? "incoming" : "outgoing",
    status: toTransferStatus(session.status),
    totalSize: Number(session.totalSize),
    completedSize: Number(session.completedSize),
  };
}

function toConnection(value: string | undefined): DeviceInfo["connection"] {
  if (value === "lan" || value === "dcutr" || value === "relay") {
    return value;
  }
  return null;
}

function toTransferStatus(value: string): TransferSession["status"] {
  switch (value) {
    case "waitingForRuntime":
    case "pending":
    case "accepted":
    case "rejected":
    case "cancelled":
    case "completed":
    case "failed":
      return value;
    default:
      return "pending";
  }
}

export default {
  swarmdrop_mobile_core,
};
