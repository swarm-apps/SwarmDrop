import { useCallback, useState } from "react";
import { getMobileCore } from "@/core/mobile-core";

// u64 在 TS 端是 bigint,这里直接用字面量,后续无需手动 BigInt(...)
const PAIRING_CODE_TTL_SECS = 600n;

export function usePairingCodeGenerator() {
  const [code, setCode] = useState<string | null>(null);
  const [expiresAt, setExpiresAt] = useState<Date | null>(null);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const generate = useCallback(async () => {
    setGenerating(true);
    setError(null);
    try {
      const info = await getMobileCore().generatePairingCode(
        PAIRING_CODE_TTL_SECS,
      );
      setCode(info.code);
      setExpiresAt(new Date(Number(info.expiresAt) * 1000));
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      console.warn("[pairing] generatePairingCode failed:", err);
    } finally {
      setGenerating(false);
    }
  }, []);

  const reset = useCallback(() => {
    setCode(null);
    setExpiresAt(null);
    setError(null);
  }, []);

  return { code, expiresAt, generating, error, generate, reset };
}
