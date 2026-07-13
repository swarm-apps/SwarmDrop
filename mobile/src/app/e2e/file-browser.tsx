import { Redirect, useLocalSearchParams } from "expo-router";
import { useEffect, useMemo, useState } from "react";
import { Pressable, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import {
  MobileTerminalReason,
  MobileTransferDirection,
  MobileTransferPhase,
  type MobileTransferProjection,
} from "react-native-swarmdrop-core";
import { useShallow } from "zustand/react/shallow";
import {
  buildFileBrowserTree,
  FileBrowser,
  type FileBrowserActions,
  type FileBrowserItem,
  type FileBrowserScope,
  flattenVisibleNodes,
  fromInboxFiles,
  fromOfferFiles,
  fromProjection,
  fromSelectedFiles,
  inboxFileId,
  isPathInsideDirectory,
  normalizeRelativePath,
  removeSelectedDirectory,
  selectedFileId,
  sessionFileId,
} from "@/components/file-browser";
import { Text } from "@/components/ui/text";
import {
  DEFAULT_FILE_BROWSER_VIEWS,
  usePreferencesStore,
} from "@/stores/preferences-store";

const FIXTURE_ASSERTIONS_PASS = fixtureAssertionsPass();

export default function FileBrowserFixtureScreen() {
  const params = useLocalSearchParams<{
    count?: string;
    scope?: string;
  }>();
  const [count, setCount] = useState(() => fixtureCount(params.count));
  const [scope, setScope] = useState<FileBrowserScope>(() =>
    fixtureScope(params.scope),
  );
  const [resetVersion, setResetVersion] = useState(0);
  const [initialScrollIndex, setInitialScrollIndex] = useState<
    number | undefined
  >();
  const { fileBrowserViews, setFileBrowserView } = usePreferencesStore(
    useShallow((state) => ({
      fileBrowserViews: state.fileBrowserViews,
      setFileBrowserView: state.setFileBrowserView,
    })),
  );
  const [files, setFiles] = useState(() =>
    makeSelectedFiles(fixtureCount(params.count)),
  );
  const items = useMemo(
    () =>
      fromSelectedFiles(files).map((item, index) => ({
        ...item,
        status: fixtureStatus(index),
        ...(index % 7 === 2 ? { progress: 37 } : {}),
      })),
    [files],
  );
  const actions = useMemo<FileBrowserActions>(
    () => ({
      removeItem: (item) =>
        setFiles((current) =>
          current.filter((file) => selectedFileId(file.sourceId) !== item.id),
        ),
      removeDirectory: (directory) =>
        setFiles((current) => removeSelectedDirectory(current, directory)),
    }),
    [],
  );

  useEffect(() => {
    for (const fixtureScope of ["send", "transfer", "inbox"] as const) {
      setFileBrowserView(
        fixtureScope,
        DEFAULT_FILE_BROWSER_VIEWS[fixtureScope],
      );
    }
  }, [setFileBrowserView]);

  if (!__DEV__) return <Redirect href="/" />;

  return (
    <SafeAreaView
      className="bg-background"
      style={{ flex: 1 }}
      edges={["top"]}
      testID="file-browser-fixture-screen"
    >
      <View className="gap-2 px-5 pt-3">
        <Text testID="file-browser-fixture-ready">
          fixture ready · {scope} · {files.length}
        </Text>
        {FIXTURE_ASSERTIONS_PASS ? (
          <Text testID="file-browser-fixture-model-pass">model pass</Text>
        ) : (
          <Text testID="file-browser-fixture-model-fail">model fail</Text>
        )}
        <View className="flex-row gap-2">
          {(["send", "transfer", "inbox"] as const).map((fixtureScope) => (
            <Text
              key={fixtureScope}
              testID={`file-browser-fixture-${fixtureScope}-${fileBrowserViews[fixtureScope]}`}
            >
              {fixtureScope}:{fileBrowserViews[fixtureScope]}
            </Text>
          ))}
        </View>
        <View className="flex-row gap-2">
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Remove foo directory"
            testID="file-browser-fixture-remove-foo"
            onPress={() =>
              setFiles((current) => removeSelectedDirectory(current, "foo"))
            }
            className="min-h-11 flex-1 items-center justify-center rounded-lg border border-border"
          >
            <Text>Remove foo</Text>
          </Pressable>
          {([1, 100, 1_000, 10_000] as const).map((fixtureSize) => (
            <Pressable
              key={fixtureSize}
              accessibilityRole="button"
              accessibilityState={{ selected: count === fixtureSize }}
              testID={`file-browser-fixture-count-${fixtureSize}`}
              onPress={() => {
                for (const fixtureScope of [
                  "send",
                  "transfer",
                  "inbox",
                ] as const) {
                  setFileBrowserView(
                    fixtureScope,
                    DEFAULT_FILE_BROWSER_VIEWS[fixtureScope],
                  );
                }
                setCount(fixtureSize);
                setFiles(makeSelectedFiles(fixtureSize));
                setInitialScrollIndex(undefined);
                setResetVersion((current) => current + 1);
              }}
              className="min-h-11 min-w-12 items-center justify-center rounded-lg border border-border px-1"
            >
              <Text className="text-[10px]">{fixtureSize}</Text>
            </Pressable>
          ))}
        </View>
        <View className="flex-row gap-2">
          {(["send", "transfer", "inbox"] as const).map((fixtureScope) => (
            <Pressable
              key={fixtureScope}
              accessibilityRole="button"
              accessibilityState={{ selected: scope === fixtureScope }}
              testID={`file-browser-fixture-scope-${fixtureScope}`}
              onPress={() => setScope(fixtureScope)}
              className="min-h-11 flex-1 items-center justify-center rounded-lg border border-border"
            >
              <Text>{fixtureScope}</Text>
            </Pressable>
          ))}
        </View>
        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Jump to final file"
          testID="file-browser-fixture-jump-end"
          onPress={() => {
            setInitialScrollIndex(Math.max(0, files.length - 1));
            setResetVersion((current) => current + 1);
          }}
          className="min-h-11 items-center justify-center rounded-lg border border-border"
        >
          <Text>Jump to end</Text>
        </Pressable>
        {files.some((file) => file.relativePath.startsWith("foobar/")) ? (
          <Text testID="file-browser-fixture-foobar-present">
            foobar present
          </Text>
        ) : null}
      </View>
      <View style={{ flex: 1 }}>
        <FileBrowser
          items={items}
          scope={scope}
          actions={actions}
          resetKey={`${scope}:${count}:${resetVersion}`}
          initialScrollIndex={initialScrollIndex}
          testID="file-browser-fixture"
          title="WebDriver fixture"
        />
      </View>
    </SafeAreaView>
  );
}

function fixtureCount(raw?: string) {
  const value = Number(raw ?? 100);
  return Number.isInteger(value) && value >= 1 && value <= 10_000 ? value : 100;
}

function fixtureScope(raw?: string): FileBrowserScope {
  return raw === "transfer" || raw === "inbox" ? raw : "send";
}

function makeSelectedFiles(count: number) {
  return Array.from({ length: count }, (_, index) => ({
    sourceId: `fixture://source/${index}`,
    name: `fixture-${String(index).padStart(5, "0")}.txt`,
    relativePath:
      index === 0
        ? "foo/same.txt"
        : index === 1
          ? "foobar/same.txt"
          : `fixture-${String(index).padStart(5, "0")}.txt`,
    size: BigInt(index + 1),
  }));
}

function fixtureStatus(index: number): FileBrowserItem["status"] {
  return [
    "idle",
    "waiting",
    "transferring",
    "paused",
    "completed",
    "cancelled",
    "error",
    "missing",
  ][index % 8] as FileBrowserItem["status"];
}

function fixtureAssertionsPass() {
  const selected = makeSelectedFiles(3);
  const selectedItems = fromSelectedFiles(selected);
  const normalized = normalizeRelativePath("root\\nested//report.txt");
  const remaining = removeSelectedDirectory(selected, "foo");
  const offer = fromOfferFiles("offer-a", [
    {
      fileId: 1,
      name: "folder",
      relativePath: "folder",
      size: 0n,
      isDirectory: true,
    },
    {
      fileId: 2,
      name: "report.txt",
      relativePath: "folder/report.txt",
      size: 12n,
      isDirectory: false,
    },
  ]);
  const suspended = fromProjection(
    projectionFixture(MobileTransferPhase.Suspended),
  );
  const completed = fromProjection(
    projectionFixture(
      MobileTransferPhase.Terminal,
      MobileTerminalReason.Completed,
    ),
  );
  const cancelled = fromProjection(
    projectionFixture(
      MobileTransferPhase.Terminal,
      MobileTerminalReason.Cancelled,
    ),
  );
  const failed = fromProjection(
    projectionFixture(
      MobileTransferPhase.Terminal,
      MobileTerminalReason.FatalError,
    ),
  );
  const inbox = fromInboxFiles("inbox-a", [
    {
      id: 7,
      relativePath: "photo.jpg",
      name: "photo.jpg",
      size: 8n,
      checksum: "fixture",
      localPath: "file:///fixture/photo.jpg",
      missing: false,
    },
    {
      id: 8,
      relativePath: "missing.txt",
      name: "missing.txt",
      size: 9n,
      checksum: "fixture",
      localPath: "file:///fixture/missing.txt",
      missing: true,
    },
  ]);
  const duplicateTree = buildFileBrowserTree([
    { ...offer[0], id: "duplicate-a" },
    { ...offer[0], id: "duplicate-b" },
  ]);
  const duplicateRows = flattenVisibleNodes(
    duplicateTree,
    duplicateTree.directoryIds,
  );

  return (
    selectedFileId("a") !== selectedFileId("b") &&
    sessionFileId("a", 1) !== sessionFileId("b", 1) &&
    inboxFileId("a", 1) !== inboxFileId("b", 1) &&
    normalized === "root/nested/report.txt" &&
    isPathInsideDirectory("foo/a.txt", "foo") &&
    !isPathInsideDirectory("foobar/a.txt", "foo") &&
    remaining.length === 2 &&
    remaining.some((file) => file.relativePath === "foobar/same.txt") &&
    offer.length === 1 &&
    offer[0]?.relativePath === "folder/report.txt" &&
    suspended[0]?.status === "paused" &&
    completed[0]?.status === "completed" &&
    cancelled[0]?.status === "cancelled" &&
    failed[0]?.status === "error" &&
    selectedItems[0]?.localUri === "fixture://source/0" &&
    inbox[0]?.localUri === "file:///fixture/photo.jpg" &&
    inbox[1]?.localUri === undefined &&
    inbox[1]?.status === "missing" &&
    duplicateRows.length === 3 &&
    duplicateRows[1]?.id !== duplicateRows[2]?.id
  );
}

function projectionFixture(
  phase: MobileTransferPhase,
  terminalReason?: MobileTerminalReason,
): MobileTransferProjection {
  return {
    sessionId: `fixture-${phase}-${terminalReason ?? "none"}`,
    direction: MobileTransferDirection.Send,
    peerId: "fixture-peer",
    peerName: "Fixture peer",
    phase,
    terminalReason,
    recoverable: phase === MobileTransferPhase.Suspended,
    epoch: 1n,
    totalSize: 10n,
    transferredBytes: phase === MobileTransferPhase.Suspended ? 5n : 0n,
    startedAt: 1n,
    updatedAt: 2n,
    files: [
      {
        fileId: 1,
        name: "projection.txt",
        relativePath: "projection.txt",
        size: 10n,
        transferredBytes: phase === MobileTransferPhase.Suspended ? 5n : 0n,
      },
    ],
  };
}
