import * as SecureStore from "expo-secure-store";
import type { ForeignKeychainProvider } from "react-native-swarmdrop-core";

const IDENTITY_KEY = "swarmdrop.identity.keypair";
const PAIRED_DEVICES_KEY = "swarmdrop.paired-devices";
const WEBRTC_CERTIFICATE_KEY = "swarmdrop.webrtc-direct.certificate";

export class Keychain implements ForeignKeychainProvider {
  async loadIdentity(): Promise<ArrayBuffer | undefined> {
    const encoded = await SecureStore.getItemAsync(IDENTITY_KEY);
    return encoded === null ? undefined : base64ToBytes(encoded);
  }

  async saveIdentity(keypair: ArrayBuffer): Promise<void> {
    await SecureStore.setItemAsync(IDENTITY_KEY, bytesToBase64(keypair));
  }

  async deleteIdentity(): Promise<void> {
    await SecureStore.deleteItemAsync(IDENTITY_KEY);
  }

  async loadWebrtcCertificatePem(): Promise<string | undefined> {
    return (await SecureStore.getItemAsync(WEBRTC_CERTIFICATE_KEY)) ?? undefined;
  }

  async saveWebrtcCertificatePem(pem: string): Promise<void> {
    await SecureStore.setItemAsync(WEBRTC_CERTIFICATE_KEY, pem);
  }

  async deleteWebrtcCertificatePem(): Promise<void> {
    await SecureStore.deleteItemAsync(WEBRTC_CERTIFICATE_KEY);
  }

  async loadPairedDevicesJson(): Promise<string> {
    return (await SecureStore.getItemAsync(PAIRED_DEVICES_KEY)) ?? "[]";
  }

  async savePairedDevicesJson(devicesJson: string): Promise<void> {
    await SecureStore.setItemAsync(PAIRED_DEVICES_KEY, devicesJson);
  }
}

function bytesToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i]);
  }
  return btoa(binary);
}

function base64ToBytes(value: string): ArrayBuffer {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes.buffer;
}
