import { useCallback, useEffect, useRef, useState } from "react";
import {
  commands,
  events,
  type AccountSnapshot,
  type AccountUpdate,
} from "@/lib/bindings";

export function useCloudAccounts(onUpdate?: (update: AccountUpdate) => void) {
  const [accounts, setAccounts] = useState<AccountSnapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const request = useRef(0);
  const updateRef = useRef(onUpdate);
  useEffect(() => {
    updateRef.current = onUpdate;
  }, [onUpdate]);
  const refresh = useCallback(async () => {
    const generation = ++request.current;
    try {
      const result = await commands.listCloudAccounts();
      if (request.current === generation) {
        setAccounts(result);
        setError(null);
      }
    } catch (error) {
      if (request.current === generation) setError(error);
    } finally {
      if (request.current === generation) setLoading(false);
    }
  }, []);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    events.cloudAccountUpdated
      .listen(({ payload }) => {
        updateRef.current?.(payload);
        void refresh();
      })
      .then((listener) => {
        if (disposed) listener();
        else {
          unlisten = listener;
          void refresh();
        }
      })
      .catch((error) => {
        if (!disposed) {
          setError(error);
          setLoading(false);
        }
      });
    return () => {
      disposed = true;
      ++request.current;
      unlisten?.();
    };
  }, [refresh]);
  return { accounts, loading, error, refresh };
}
