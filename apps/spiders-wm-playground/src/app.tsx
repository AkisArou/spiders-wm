import { useEffect, useEffectEvent, useMemo, useState } from "react";

import type { LayoutContext, LayoutWindow } from "@spiders-wm/sdk/layout";

import { EditorPane } from "./tabs/editor/index.tsx";
import { PreviewPane } from "./tabs/preview/index.tsx";
import { SystemPane } from "./tabs/system/index.tsx";
import type {
  EditorFile,
  EditorFileId,
  FileTreeDirectoryNode,
} from "./tabs/editor/types.ts";
import * as configBindingsSourceModule from "./spiders-wm/config/bindings.ts?raw";
import * as configInputsSourceModule from "./spiders-wm/config/inputs.ts?raw";
import * as configLayoutsSourceModule from "./spiders-wm/config/layouts.ts?raw";
import * as rootConfigSourceModule from "./spiders-wm/config.ts?raw";
import * as rootStylesheetSourceModule from "./spiders-wm/index.css?raw";
import * as layoutStylesheetSourceModule from "./spiders-wm/layouts/master-stack/index.css?raw";
import * as layoutSourceModule from "./spiders-wm/layouts/master-stack/index.tsx?raw";
import layout from "./spiders-wm/layouts/master-stack/index.tsx";
import {
  matchesBindingEvent,
  parseBindingsSource,
} from "./playground-bindings.ts";
import { cn } from "./utils/cn.ts";
import {
  applyPreviewCommand,
  computePreview,
  type PreviewComputation,
  type PreviewSessionCommand,
  type PreviewSessionWindow,
} from "./wasm-preview.ts";

type TabId = "preview" | "editor" | "system";

type PlaygroundWindow = PreviewSessionWindow;

const monitor = {
  name: "DP-1",
  width: 3440,
  height: 1440,
  scale: 1,
};

const tabs: Array<{ id: TabId; label: string }> = [
  { id: "preview", label: "1:preview" },
  { id: "editor", label: "2:editor" },
  { id: "system", label: "3:system" },
];

const previewCommandGate: {
  lastDispatchedCommand: { key: string; timeStamp: number } | null;
  pendingCommandKey: string | null;
} = {
  lastDispatchedCommand: null,
  pendingCommandKey: null,
};

const configBindingsSource = rawModuleSource(configBindingsSourceModule);
const configInputsSource = rawModuleSource(configInputsSourceModule);
const configLayoutsSource = rawModuleSource(configLayoutsSourceModule);
const rootConfigSource = rawModuleSource(rootConfigSourceModule);
const rootStylesheetSource = rawModuleSource(rootStylesheetSourceModule);
const layoutStylesheetSource = rawModuleSource(layoutStylesheetSourceModule);
const layoutSource = rawModuleSource(layoutSourceModule);

const editorFiles: EditorFile[] = [
  {
    id: "root-css",
    label: "index.css",
    path: "~/.config/spiders-wm/index.css",
    modelPath: "file:///home/demo/.config/spiders-wm/index.css",
    language: "css",
    initialValue: rootStylesheetSource,
    icon: "",
    iconTone: "text-file-css",
  },
  {
    id: "config",
    label: "config.ts",
    path: "~/.config/spiders-wm/config.ts",
    modelPath: "file:///home/demo/.config/spiders-wm/config.ts",
    language: "typescript",
    initialValue: rootConfigSource,
    icon: "",
    iconTone: "text-file-ts",
  },
  {
    id: "config-bindings",
    label: "bindings.ts",
    path: "~/.config/spiders-wm/config/bindings.ts",
    modelPath: "file:///home/demo/.config/spiders-wm/config/bindings.ts",
    language: "typescript",
    initialValue: configBindingsSource,
    icon: "",
    iconTone: "text-file-ts",
  },
  {
    id: "config-inputs",
    label: "inputs.ts",
    path: "~/.config/spiders-wm/config/inputs.ts",
    modelPath: "file:///home/demo/.config/spiders-wm/config/inputs.ts",
    language: "typescript",
    initialValue: configInputsSource,
    icon: "",
    iconTone: "text-file-ts",
  },
  {
    id: "config-layouts",
    label: "layouts.ts",
    path: "~/.config/spiders-wm/config/layouts.ts",
    modelPath: "file:///home/demo/.config/spiders-wm/config/layouts.ts",
    language: "typescript",
    initialValue: configLayoutsSource,
    icon: "",
    iconTone: "text-file-ts",
  },
  {
    id: "layout-tsx",
    label: "index.tsx",
    path: "~/.config/spiders-wm/layouts/master-stack/index.tsx",
    modelPath:
      "file:///home/demo/.config/spiders-wm/layouts/master-stack/index.tsx",
    language: "typescript",
    initialValue: layoutSource,
    icon: "",
    iconTone: "text-file-tsx",
  },
  {
    id: "layout-css",
    label: "index.css",
    path: "~/.config/spiders-wm/layouts/master-stack/index.css",
    modelPath:
      "file:///home/demo/.config/spiders-wm/layouts/master-stack/index.css",
    language: "css",
    initialValue: layoutStylesheetSource,
    icon: "",
    iconTone: "text-file-css",
  },
];

const fileTree: FileTreeDirectoryNode = {
  kind: "directory",
  name: "~/.config/spiders-wm",
  defaultOpen: true,
  children: [
    { kind: "file", fileId: "root-css" },
    { kind: "file", fileId: "config" },
    {
      kind: "directory",
      name: "config",
      defaultOpen: true,
      children: [
        { kind: "file", fileId: "config-bindings" },
        { kind: "file", fileId: "config-inputs" },
        { kind: "file", fileId: "config-layouts" },
      ],
    },
    {
      kind: "directory",
      name: "layouts",
      defaultOpen: true,
      children: [
        {
          kind: "directory",
          name: "master-stack",
          defaultOpen: true,
          children: [
            { kind: "file", fileId: "layout-tsx" },
            { kind: "file", fileId: "layout-css" },
          ],
        },
      ],
    },
  ],
};

const initialWorkspaceNames = parseWorkspaceNames(rootConfigSource);

const initialWindows: PlaygroundWindow[] = [
  {
    id: "win-1",
    app_id: "foot",
    title: "Terminal 1",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
    focused: true,
    workspaceName: "1",
  },
  {
    id: "win-2",
    app_id: "zen",
    title: "Spec Draft",
    class: "zen-browser",
    instance: "zen",
    shell: "xdg_toplevel",
    workspaceName: "1",
  },
  {
    id: "win-3",
    app_id: "slack",
    title: "Engineering",
    class: "Slack",
    instance: "slack",
    shell: "xdg_toplevel",
    workspaceName: "1",
  },
  {
    id: "win-4",
    app_id: "foot",
    title: "Terminal 2",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
    workspaceName: "2",
  },
  {
    id: "win-5",
    app_id: "zen",
    title: "Reference",
    class: "zen-browser",
    instance: "zen",
    shell: "xdg_toplevel",
    workspaceName: "2",
  },
  {
    id: "win-6",
    app_id: "foot",
    title: "Terminal 3",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
    workspaceName: "3",
  },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<TabId>("preview");
  const [activeFileId, setActiveFileId] =
    useState<EditorFileId>("config-bindings");
  const [activeWorkspaceName, setActiveWorkspaceName] = useState<string>(
    initialWorkspaceNames[0] ?? "1:dev",
  );
  const [editorBuffers, setEditorBuffers] = useState<
    Record<EditorFileId, string>
  >(() => ({
    config: rootConfigSource,
    "config-bindings": configBindingsSource,
    "config-inputs": configInputsSource,
    "config-layouts": configLayoutsSource,
    "root-css": rootStylesheetSource,
    "layout-tsx": layoutSource,
    "layout-css": layoutStylesheetSource,
  }));
  const [windows, setWindows] = useState<PlaygroundWindow[]>(
    () => initialWindows,
  );
  const [preview, setPreview] = useState<PreviewComputation | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [lastAction, setLastAction] = useState<string>(
    "Alt+Return spawns foot",
  );
  const configSource = editorBuffers.config;
  const bindingsSource = editorBuffers["config-bindings"];
  const workspaceNames = useMemo(
    () => parseWorkspaceNames(configSource),
    [configSource],
  );
  const bindingState = useMemo(
    () => parseBindingsSource(bindingsSource),
    [bindingsSource],
  );
  const visibleWindows = useMemo(
    () =>
      windows
        .filter((window) => window.workspaceName === activeWorkspaceName)
        .map(stripWorkspaceName),
    [activeWorkspaceName, windows],
  );
  const previewSessionState = useMemo(
    () => ({
      activeWorkspaceName,
      workspaceNames,
      windows,
    }),
    [activeWorkspaceName, workspaceNames, windows],
  );
  const previewContext = useMemo<LayoutContext>(
    () => ({
      monitor,
      workspace: {
        name: activeWorkspaceName,
        workspaces: workspaceNames,
        windowCount: visibleWindows.length,
      },
      windows: visibleWindows,
      state: {
        prototype: true,
        lastAction,
      },
    }),
    [activeWorkspaceName, lastAction, visibleWindows, workspaceNames],
  );

  useEffect(() => {
    if (!workspaceNames.includes(activeWorkspaceName) && workspaceNames[0]) {
      setActiveWorkspaceName(workspaceNames[0]);
    }
  }, [activeWorkspaceName, workspaceNames]);

  const refreshPreview = useEffectEvent(async () => {
    const nextPreview = await computePreview(
      layout(previewContext),
      previewContext.windows,
      layoutStylesheetSource,
      monitor.width,
      monitor.height,
      previewSessionState,
    );

    return nextPreview;
  });

  useEffect(() => {
    if (activeTab === "editor") {
      return undefined;
    }

    let cancelled = false;

    void refreshPreview()
      .then((nextPreview) => {
        if (!cancelled) {
          setPreview(nextPreview);
          setPreviewError(null);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          setPreviewError(
            error instanceof Error
              ? error.message
              : "Failed to initialize wasm preview.",
          );
        }
      });

    return () => {
      cancelled = true;
    };
  }, [activeTab, previewContext, previewSessionState]);

  const dispatchPreviewCommand = useEffectEvent(
    async (command: PreviewSessionCommand, actionLabel?: string) => {
      const pendingCommandKey = `${command.name}:${String(command.arg ?? "")}`;

      if (previewCommandGate.pendingCommandKey === pendingCommandKey) {
        return;
      }

      previewCommandGate.pendingCommandKey = pendingCommandKey;

      try {
        const nextState = await applyPreviewCommand(
          previewSessionState,
          command,
          preview?.snapshotRoot ?? null,
        );

        setWindows(nextState.windows);
        setActiveWorkspaceName(nextState.activeWorkspaceName);

        if (actionLabel) {
          setLastAction(
            describePreviewAction(
              actionLabel,
              command,
              nextState.activeWorkspaceName,
              workspaceNames,
            ),
          );
        }
      } finally {
        if (previewCommandGate.pendingCommandKey === pendingCommandKey) {
          previewCommandGate.pendingCommandKey = null;
        }
      }
    },
  );

  useEffect(() => {
    if (activeTab === "editor") {
      return undefined;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.repeat) {
        return;
      }

      if (event.defaultPrevented) {
        return;
      }

      const matchedEntry = bindingState.entries.find((entry) =>
        matchesBindingEvent(entry, event, bindingState.mod),
      );

      if (!matchedEntry) {
        return;
      }

      const commandKey = [
        matchedEntry.commandName,
        String(matchedEntry.commandArg ?? ""),
        event.code,
        event.altKey,
        event.ctrlKey,
        event.metaKey,
        event.shiftKey,
      ].join(":");
      const lastDispatchedCommand = previewCommandGate.lastDispatchedCommand;

      if (
        lastDispatchedCommand &&
        lastDispatchedCommand.key === commandKey &&
        Math.abs(event.timeStamp - lastDispatchedCommand.timeStamp) < 100
      ) {
        return;
      }

      previewCommandGate.lastDispatchedCommand = {
        key: commandKey,
        timeStamp: event.timeStamp,
      };

      event.preventDefault();

      const command: PreviewSessionCommand = {
        name: matchedEntry.commandName,
        arg: matchedEntry.commandArg,
      };

      void (async () => {
        try {
          await dispatchPreviewCommand(command, matchedEntry.chord);
        } catch (error) {
          setPreviewError(
            error instanceof Error
              ? error.message
              : "Failed to apply preview command.",
          );
        }
      })();
    };

    window.addEventListener("keydown", onKeyDown);

    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [activeTab, bindingState]);

  const activeFile =
    editorFiles.find((file) => file.id === activeFileId) ?? editorFiles[0];

  if (!activeFile) {
    return null;
  }

  const dirtyFileCount = editorFiles.filter(
    (file) => editorBuffers[file.id] !== file.initialValue,
  ).length;

  return (
    <div className="bg-terminal-bg text-terminal-fg flex h-screen flex-col overflow-hidden">
      <div className="border-terminal-border bg-terminal-bg-subtle border-b px-2 pt-1">
        <div className="flex items-end gap-px text-xs leading-none">
          {tabs.map((tab) => {
            const active = tab.id === activeTab;

            return (
              <button
                key={tab.id}
                type="button"
                className={cn(
                  "border border-b-0 px-2 py-1 font-medium tracking-tight transition-opacity",
                  active
                    ? "border-terminal-border-strong bg-terminal-bg text-terminal-fg-strong"
                    : "border-terminal-border bg-terminal-bg-bar text-terminal-dim opacity-70 hover:opacity-100 hover:text-terminal-fg",
                )}
                onClick={() => setActiveTab(tab.id)}
              >
                {tab.label}
              </button>
            );
          })}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden p-2">
        {activeTab === "preview" ? (
          <PreviewPane
            preview={preview}
            previewError={previewError}
            context={previewContext}
            onSelectWorkspace={(workspaceName) => {
              const workspaceIndex = workspaceNames.indexOf(workspaceName);

              if (workspaceIndex < 0 || workspaceName === activeWorkspaceName) {
                return;
              }

              void dispatchPreviewCommand(
                { name: "view_workspace", arg: workspaceIndex + 1 },
                `click ${workspaceName}`,
              );
            }}
          />
        ) : null}

        {activeTab === "editor" ? (
          <EditorPane
            files={editorFiles}
            fileTree={fileTree}
            activeFileId={activeFileId}
            buffers={editorBuffers}
            onSelectFile={setActiveFileId}
            onChangeBuffer={(fileId, value) => {
              setEditorBuffers((current) => ({
                ...current,
                [fileId]: value,
              }));
            }}
          />
        ) : null}

        {activeTab === "system" ? (
          <SystemPane
            preview={preview}
            previewError={previewError}
            activeFile={activeFile}
            dirtyFileCount={dirtyFileCount}
            bindingsSource={editorBuffers["config-bindings"]}
            context={previewContext}
          />
        ) : null}
      </div>
    </div>
  );
}

function parseWorkspaceNames(source: string): string[] {
  const workspacesSource =
    source.match(/\bworkspaces:\s*\[([\s\S]*?)\]/)?.[1] ?? "";
  const workspaces = Array.from(
    workspacesSource.matchAll(/"([^"]+)"/g),
    ([, name]) => name ?? "",
  ).filter(Boolean);

  return workspaces.length > 0 ? workspaces : ["1", "2", "3"];
}

function describePreviewAction(
  chord: string,
  command: PreviewSessionCommand,
  activeWorkspaceName: string,
  workspaceNames: string[],
): string {
  switch (command.name) {
    case "view_workspace":
      return `${chord} -> ${activeWorkspaceName}`;
    case "assign_workspace": {
      const targetWorkspace =
        typeof command.arg === "number"
          ? workspaceNames[command.arg - 1]
          : null;

      return `${chord} -> move to ${targetWorkspace ?? "workspace"}`;
    }
    case "focus_dir":
      return `${chord} -> focus ${String(command.arg ?? "window")}`;
    case "swap_dir":
      return `${chord} -> swap ${String(command.arg ?? "window")}`;
    case "resize_dir":
      return `${chord} -> resize ${String(command.arg ?? "window")}`;
    case "resize_tiled":
      return `${chord} -> resize tiled ${String(command.arg ?? "window")}`;
    case "toggle_floating":
      return `${chord} -> toggle floating`;
    case "kill_client":
      return `${chord} -> close window`;
    case "spawn":
      return `${chord} -> spawn foot`;
    case "cycle_layout":
      return `${chord} -> layout unchanged`;
    default:
      return `${chord} -> ${command.name}`;
  }
}

function stripWorkspaceName(window: PlaygroundWindow): LayoutWindow {
  const { workspaceName: _workspaceName, ...layoutWindow } = window;

  return layoutWindow;
}

function rawModuleSource(module: string | { default?: string }): string {
  if (typeof module === "string") {
    return module;
  }

  return typeof module.default === "string" ? module.default : "";
}
