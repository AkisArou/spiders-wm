import { useEffect, useState } from "react";

import rootConfigSource from "./editor-fixtures/config.ts?raw";
import rootStylesheetSource from "./editor-fixtures/index.css?raw";
import layoutStylesheetSource from "./layouts/master-stack/index.css?raw";
import layoutSource from "./layouts/master-stack/index.tsx?raw";
import layout from "./layouts/master-stack/index.tsx";
import { EditorPane } from "./components/editor-pane.tsx";
import { PreviewPane } from "./components/preview-pane.tsx";
import { SystemPane } from "./components/system-pane.tsx";
import type { EditorFile, EditorFileId, FileTreeDirectoryNode } from "./components/playground-types.ts";
import type { LayoutContext, LayoutWindow } from "@spiders-wm/sdk/layout";
import { computePreview, type PreviewComputation } from "./wasm-preview.ts";

type TabId = "preview" | "editor" | "system";

const mockWindows: LayoutWindow[] = [
  {
    id: "win-1",
    app_id: "foot",
    title: "Terminal 1",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
    focused: true,
  },
  {
    id: "win-2",
    app_id: "foot",
    title: "Terminal 2",
    class: "foot",
    instance: "foot",
    shell: "xdg_toplevel",
  },
  {
    id: "win-3",
    app_id: "zen",
    title: "Spec Draft",
    class: "zen-browser",
    instance: "zen",
    shell: "xdg_toplevel",
  },
  {
    id: "win-4",
    app_id: "slack",
    title: "Engineering",
    class: "Slack",
    instance: "slack",
    shell: "xdg_toplevel",
  },
  {
    id: "win-5",
    app_id: "spotify",
    title: "Now Playing",
    class: "Spotify",
    instance: "spotify",
    shell: "xdg_toplevel",
    floating: true,
  },
];

const mockContext: LayoutContext = {
  monitor: {
    name: "DP-1",
    width: 3440,
    height: 1440,
    scale: 1,
  },
  workspace: {
    name: "1:dev",
    workspaces: ["1:dev", "2:web", "3:chat"],
    windowCount: mockWindows.length,
  },
  windows: mockWindows,
  state: {
    prototype: true,
  },
};

const tabs: Array<{ id: TabId; label: string }> = [
  { id: "preview", label: "1:preview" },
  { id: "editor", label: "2:editor" },
  { id: "system", label: "3:system" },
];

const editorFiles: EditorFile[] = [
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
    id: "layout-tsx",
    label: "index.tsx",
    path: "~/.config/spiders-wm/layouts/master-stack/index.tsx",
    modelPath: "file:///home/demo/.config/spiders-wm/layouts/master-stack/index.tsx",
    language: "typescript",
    initialValue: layoutSource,
    icon: "",
    iconTone: "text-file-tsx",
  },
  {
    id: "layout-css",
    label: "index.css",
    path: "~/.config/spiders-wm/layouts/master-stack/index.css",
    modelPath: "file:///home/demo/.config/spiders-wm/layouts/master-stack/index.css",
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
    { kind: "file", fileId: "config" },
    { kind: "file", fileId: "root-css" },
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

export default function App() {
  const [activeTab, setActiveTab] = useState<TabId>("preview");
  const [activeFileId, setActiveFileId] = useState<EditorFileId>("layout-tsx");
  const [editorBuffers, setEditorBuffers] = useState<Record<EditorFileId, string>>(() => ({
    config: rootConfigSource,
    "root-css": rootStylesheetSource,
    "layout-tsx": layoutSource,
    "layout-css": layoutStylesheetSource,
  }));
  const [preview, setPreview] = useState<PreviewComputation | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    void (async () => {
      try {
        const nextPreview = await computePreview(
          layout(mockContext),
          mockContext.windows,
          layoutStylesheetSource,
          mockContext.monitor.width,
          mockContext.monitor.height,
        );

        if (!cancelled) {
          setPreview(nextPreview);
          setPreviewError(null);
        }
      } catch (error) {
        if (!cancelled) {
          setPreviewError(
            error instanceof Error ? error.message : "Failed to initialize wasm preview.",
          );
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, []);

  const activeFile = editorFiles.find((file) => file.id === activeFileId) ?? editorFiles[0];

  if (!activeFile) {
    return null;
  }

  const dirtyFileCount = editorFiles.filter(
    (file) => editorBuffers[file.id] !== file.initialValue,
  ).length;

  return (
    <div className="flex h-screen flex-col overflow-hidden bg-terminal-bg text-terminal-fg">
      <div className="border-b border-terminal-border bg-terminal-bg-subtle px-2 py-1 text-xs text-terminal-faint">
        ssh spiders@playground  ::  ~/.config/spiders-wm  ::  {mockContext.workspace.name}
      </div>

      <div className="border-b border-terminal-border bg-terminal-bg-subtle px-2 pt-1">
        <div className="flex items-end gap-px text-xs leading-none">
          {tabs.map((tab) => {
            const active = tab.id === activeTab;

            return (
              <button
                key={tab.id}
                type="button"
                className={[
                  "border border-b-0 px-2 py-1 font-medium tracking-tight",
                  active
                    ? "border-terminal-border-strong bg-terminal-bg text-terminal-fg-strong"
                    : "border-terminal-border bg-terminal-bg-bar text-terminal-dim hover:text-terminal-fg",
                ].join(" ")}
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
          <PreviewPane preview={preview} previewError={previewError} context={mockContext} />
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
            context={mockContext}
          />
        ) : null}
      </div>
    </div>
  );
}