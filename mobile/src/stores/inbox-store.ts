import { create } from "zustand";

export type InboxContentKind = "file" | "folder" | "mixed";

export interface InboxPreviewItem {
  id: string;
  title: string;
  sourceName: string;
  kind: InboxContentKind;
  receivedAt: number;
  missing: boolean;
  archived: boolean;
}

interface InboxState {
  loading: boolean;
  items: InboxPreviewItem[];
  lastRefreshedAt: number | null;
  refresh: () => Promise<void>;
}

export const useInboxStore = create<InboxState>()((set) => ({
  loading: false,
  items: [],
  lastRefreshedAt: null,
  async refresh() {
    set({ loading: true });
    try {
      set({ lastRefreshedAt: Date.now() });
    } finally {
      set({ loading: false });
    }
  },
}));
