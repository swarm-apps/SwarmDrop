import { Link2, X } from "lucide-react-native";
import { useCallback, useState } from "react";
import { Modal, Pressable, StyleSheet, Text, View } from "react-native";
import { getMobileCore } from "@/core/mobile-core";
import { useExpiresCountdown } from "@/hooks/useExpiresCountdown";
import { useNotificationStore } from "@/stores/notification-store";

const REQUEST_TTL_SECS = 60;

export function PairingRequestHost() {
  const current = useNotificationStore((s) => s.current);
  const respondStore = useNotificationStore((s) => s.respond);
  const [responding, setResponding] = useState(false);

  const open = current !== null && current.type === "pairing-request";
  const payload = open ? current.payload : null;
  const expiresAt = payload
    ? payload.receivedAt + REQUEST_TTL_SECS * 1000
    : null;

  const onExpire = useCallback(() => {
    if (current) respondStore(current.id);
  }, [current, respondStore]);

  const remaining = useExpiresCountdown(expiresAt, onExpire);

  const respondToRequest = useCallback(
    async (accept: boolean) => {
      if (!current || !payload || responding) return;
      setResponding(true);
      try {
        await getMobileCore().respondPairingRequest(
          Number(payload.pendingId),
          payload.code ?? null,
          accept,
        );
      } catch (err) {
        console.warn(
          `[pairing-host] ${accept ? "accept" : "reject"} failed:`,
          err,
        );
      } finally {
        respondStore(current.id);
        setResponding(false);
      }
    },
    [current, payload, respondStore, responding],
  );

  if (!open || !payload) {
    return null;
  }

  return (
    <Modal
      animationType="fade"
      transparent
      visible={open}
      onRequestClose={() => respondToRequest(false)}
    >
      <View style={styles.backdrop}>
        <View style={styles.card}>
          <View style={styles.header}>
            <View style={styles.iconWrap}>
              <Link2 color="#2563EB" size={22} />
            </View>
            <Text style={styles.title}>配对请求</Text>
            <View style={styles.spacer} />
            <Text style={styles.countdown}>
              {remaining > 0 ? `${remaining}s` : "已过期"}
            </Text>
          </View>

          <Text style={styles.body}>
            来自{" "}
            <Text style={styles.peerId}>{payload.peerId.slice(0, 12)}...</Text>{" "}
            的设备请求与你配对
            {payload.code ? `（配对码 ${payload.code}）` : ""}
          </Text>

          <View style={styles.actions}>
            <Pressable
              onPress={() => respondToRequest(true)}
              disabled={responding || remaining === 0}
              style={[
                styles.acceptButton,
                (responding || remaining === 0) && styles.disabled,
              ]}
            >
              <Text style={styles.acceptText}>接受</Text>
            </Pressable>
            <Pressable
              onPress={() => respondToRequest(false)}
              disabled={responding}
              style={styles.rejectButton}
            >
              <X color="#0F172A" size={16} />
              <Text style={styles.rejectText}>拒绝</Text>
            </Pressable>
          </View>
        </View>
      </View>
    </Modal>
  );
}

const styles = StyleSheet.create({
  backdrop: {
    alignItems: "center",
    backgroundColor: "rgba(15, 23, 42, 0.55)",
    flex: 1,
    justifyContent: "center",
    paddingHorizontal: 24,
  },
  card: {
    backgroundColor: "#FFFFFF",
    borderRadius: 16,
    gap: 16,
    padding: 20,
    width: "100%",
  },
  header: {
    alignItems: "center",
    flexDirection: "row",
    gap: 12,
  },
  iconWrap: {
    alignItems: "center",
    backgroundColor: "#EFF6FF",
    borderRadius: 999,
    height: 40,
    justifyContent: "center",
    width: 40,
  },
  title: {
    color: "#0F172A",
    fontSize: 17,
    fontWeight: "700",
  },
  spacer: {
    flex: 1,
  },
  countdown: {
    color: "#64748B",
    fontSize: 12,
  },
  body: {
    color: "#334155",
    fontSize: 14,
    lineHeight: 20,
  },
  peerId: {
    color: "#0F172A",
    fontFamily: "monospace",
    fontWeight: "700",
  },
  actions: {
    gap: 10,
  },
  acceptButton: {
    alignItems: "center",
    backgroundColor: "#2563EB",
    borderRadius: 10,
    justifyContent: "center",
    minHeight: 48,
  },
  acceptText: {
    color: "#FFFFFF",
    fontSize: 15,
    fontWeight: "700",
  },
  rejectButton: {
    alignItems: "center",
    backgroundColor: "#F1F5F9",
    borderRadius: 10,
    flexDirection: "row",
    gap: 6,
    justifyContent: "center",
    minHeight: 48,
  },
  rejectText: {
    color: "#0F172A",
    fontSize: 15,
    fontWeight: "600",
  },
  disabled: {
    opacity: 0.5,
  },
});
