import { useState } from "react";

import type { LayoutContext, LayoutWindow } from "@spiders-wm/sdk/layout";

import { cn } from "../../utils/cn.ts";
import type {
  PreviewComputation,
  PreviewDiagnostic,
  PreviewSnapshotNode,
  PreviewTreeNode,
} from "../../wasm-preview.ts";

const panelClass =
  "flex min-h-0 flex-col overflow-hidden border border-terminal-border bg-terminal-bg-subtle";
const barClass =
  "flex items-center justify-between border-b border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-dim";

export function previewPane({
  preview,
  previewError,
  context,
  showSidebar,
  onToggleSidebar,
  onSelectWorkspace,
}: {
  preview: PreviewComputation | null;
  previewError: string | null;
  context: LayoutContext;
  showSidebar: boolean;
  onToggleSidebar: () => void;
  onSelectWorkspace?: (workspaceName: string) => void;
}) {
  const workspaceNames = context.workspace.workspaces ?? [
    context.workspace.name,
  ];
  const focusedWindowTitle =
    context.windows.find((window) => window.focused)?.title ?? "";

  return (
    <section
      className={cn(
        "grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2",
        showSidebar
          ? "xl:grid-cols-[minmax(0,1.55fr)_22rem]"
          : "xl:grid-cols-[minmax(0,1fr)]",
      )}
    >
      <div className={panelClass}>
        <div className="border-terminal-border bg-terminal-bg-bar text-terminal-dim grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 border-b px-2 py-1 text-xs">
          <div className="flex min-w-0 items-center gap-1 overflow-hidden">
            {workspaceNames.map((workspaceName) => (
              <button
                key={workspaceName}
                type="button"
                onClick={() => {
                  onSelectWorkspace?.(workspaceName);
                }}
                className={cn(
                  "border px-2 py-0.5",
                  workspaceName === context.workspace.name
                    ? "border-terminal-info bg-terminal-info/10 text-terminal-info"
                    : "border-terminal-border bg-terminal-bg-subtle text-terminal-dim hover:text-terminal-fg",
                )}
              >
                {workspaceName}
              </button>
            ))}
          </div>

          <div className="text-terminal-fg-strong min-w-0 truncate px-2">
            {focusedWindowTitle}
          </div>

          <div className="flex items-center gap-2 justify-self-end">
            <span>{context.windows.length} windows</span>
            <button
              type="button"
              onClick={onToggleSidebar}
              aria-label="Toggle info"
              title="Toggle info"
              className={cn(
                "border px-2 py-0.5 text-xs",
                showSidebar
                  ? "border-terminal-info text-terminal-info bg-terminal-info/10"
                  : "border-terminal-border text-terminal-dim bg-terminal-bg-subtle",
              )}
            >
              Toggle info
            </button>
          </div>
        </div>

        {previewError ? (
          <div className="text-terminal-error p-3 text-sm">{previewError}</div>
        ) : preview?.treeRoot ? (
          <div className="min-h-0 flex-1 overflow-hidden">
            {preview.snapshotRoot ? (
              <GeometryPreview
                root={preview.snapshotRoot}
                windows={context.windows}
                fallbackWidth={context.monitor.width}
                fallbackHeight={context.monitor.height}
              />
            ) : (
              <div className="border-terminal-border bg-terminal-bg-subtle text-terminal-faint flex h-full items-center justify-center border text-sm">
                no snapshot
              </div>
            )}
          </div>
        ) : preview ? (
          <div className="text-terminal-warn grid h-full place-items-center p-3 text-sm">
            preview returned no resolved root
          </div>
        ) : (
          <div className="text-terminal-faint p-3 text-sm">
            loading wasm preview...
          </div>
        )}
      </div>

      {showSidebar ? (
        <div className="grid min-h-0 gap-2 xl:grid-rows-[auto_auto_minmax(10rem,0.85fr)_minmax(12rem,1fr)]">
          <div className={panelClass}>
            <div className={barClass}>session://windows</div>
            <div className="text-terminal-muted grid gap-3 p-2 text-sm">
              <WindowList windows={context.windows} emptyLabel="no windows" />
            </div>
          </div>

          <div className={panelClass}>
            <div className={barClass}>session://unclaimed</div>
            <div className="text-terminal-muted p-2 text-sm">
              <WindowList
                windows={preview?.unclaimedWindows ?? []}
                emptyLabel="all claimed"
              />
            </div>
          </div>

          <div className={panelClass}>
            <div className={barClass}>scene://diagnostics</div>
            <div className="min-h-0 overflow-auto p-2 text-sm">
              <DiagnosticsList diagnostics={preview?.diagnostics ?? []} />
            </div>
          </div>

          <div className={panelClass}>
            <div className={barClass}>scene://tree</div>
            <div className="min-h-0 overflow-auto p-2">
              {preview?.treeRoot ? (
                <LayoutTreeNode node={preview.treeRoot} />
              ) : (
                <div className="text-terminal-faint text-sm">
                  no resolved tree
                </div>
              )}
            </div>
          </div>
        </div>
      ) : null}
    </section>
  );
}

export function PreviewPane({
  preview,
  previewError,
  context,
  onSelectWorkspace,
}: {
  preview: PreviewComputation | null;
  previewError: string | null;
  context: LayoutContext;
  onSelectWorkspace?: (workspaceName: string) => void;
}) {
  const [showSidebar, setShowSidebar] = useState(false);

  return previewPane({
    preview,
    previewError,
    context,
    showSidebar,
    onToggleSidebar: () => {
      setShowSidebar((current) => !current);
    },
    onSelectWorkspace,
  });
}

function GeometryPreview({
  root,
  windows,
  fallbackWidth,
  fallbackHeight,
}: {
  root: PreviewSnapshotNode;
  windows: LayoutWindow[];
  fallbackWidth: number;
  fallbackHeight: number;
}) {
  const stageWidth = root.rect.width || fallbackWidth;
  const stageHeight = root.rect.height || fallbackHeight;
  const windowsById = new Map(windows.map((window) => [window.id, window]));
  const windowNodes = collectWindowNodes(root);

  return (
    <div
      className="bg-terminal-bg-subtle relative h-full min-h-72 w-full overflow-hidden"
      style={{
        aspectRatio: `${stageWidth} / ${stageHeight}`,
        backgroundColor: "var(--color-terminal-bg-subtle)",
        backgroundImage:
          "linear-gradient(color-mix(in srgb, var(--color-terminal-bg) 72%, transparent), color-mix(in srgb, var(--color-terminal-bg-subtle) 58%, transparent)), url('/archlinux-logo.svg')",
        backgroundPosition: "center, center",
        backgroundRepeat: "no-repeat, no-repeat",
        backgroundSize: "cover, min(34rem, 56%)",
      }}
    >
      {windowNodes.map((node, index) => (
        <GeometryWindowNode
          key={`${node.windowId ?? node.id ?? node.type}-${index}`}
          node={node}
          window={
            node.windowId ? (windowsById.get(node.windowId) ?? null) : null
          }
          stageWidth={stageWidth}
          stageHeight={stageHeight}
        />
      ))}
    </div>
  );
}

function GeometryWindowNode({
  node,
  window,
  stageWidth,
  stageHeight,
}: {
  node: PreviewSnapshotNode;
  window: LayoutWindow | null;
  stageWidth: number;
  stageHeight: number;
}) {
  const left = (node.rect.x / stageWidth) * 100;
  const top = (node.rect.y / stageHeight) * 100;
  const width = (node.rect.width / stageWidth) * 100;
  const height = (node.rect.height / stageHeight) * 100;
  const focused = Boolean(window?.focused);
  const isFoot = window?.app_id === "foot";

  return (
    <div
      className={cn(
        "text-terminal-fg absolute overflow-hidden border text-xs",
        focused
          ? "border-terminal-info bg-terminal-bg-active"
          : "border-terminal-border-strong bg-terminal-bg-panel",
      )}
      style={{
        left: `${left}%`,
        top: `${top}%`,
        width: `${width}%`,
        height: `${height}%`,
      }}
    >
      <div className="bg-terminal-bg-subtle/80 text-terminal-dim flex items-center justify-between border-b border-current/20 px-1 py-0.5 text-xs">
        <span>{window?.title ?? node.id ?? node.type}</span>
        <span>
          {Math.round(node.rect.width)}x{Math.round(node.rect.height)}
        </span>
      </div>

      {isFoot ? <FootTerminal focused={focused} /> : null}

      {!isFoot ? <WindowSurface window={window} /> : null}
    </div>
  );
}

function collectWindowNodes(root: PreviewSnapshotNode): PreviewSnapshotNode[] {
  const nodes: PreviewSnapshotNode[] = [];
  const queue = [...root.children];

  while (queue.length > 0) {
    const node = queue.shift();

    if (!node) {
      continue;
    }

    if (node.type === "window") {
      nodes.push(node);
      continue;
    }

    queue.unshift(...node.children);
  }

  return nodes;
}

function LayoutTreeNode({
  node,
  depth = 0,
}: {
  node: PreviewTreeNode;
  depth?: number;
}) {
  return (
    <div className="text-terminal-muted text-sm leading-5">
      <div
        className="border-terminal-border bg-terminal-bg-panel flex items-center gap-2 border px-2 py-1"
        style={{ marginLeft: `${depth * 12}px` }}
      >
        <span className="text-terminal-dim">{node.type}</span>
        <span className="text-terminal-fg-strong">{node.id ?? "_"}</span>
        {node.rect ? (
          <span className="text-terminal-faint">
            {Math.round(node.rect.width)}x{Math.round(node.rect.height)}
          </span>
        ) : null}
        <span className="text-terminal-faint ml-auto">
          {node.claimedWindows.length}
        </span>
      </div>

      <div className="mt-1 grid gap-1">
        {node.claimedWindows.length > 0 ? (
          <div
            className="text-terminal-faint text-xs"
            style={{ marginLeft: `${depth * 12 + 12}px` }}
          >
            {node.claimedWindows
              .map((window) => window.title ?? window.id)
              .join("  |  ")}
          </div>
        ) : null}

        {node.children.map((child) => (
          <LayoutTreeNode key={child.path} node={child} depth={depth + 1} />
        ))}
      </div>
    </div>
  );
}

function WindowList({
  windows,
  emptyLabel,
}: {
  windows: LayoutWindow[];
  emptyLabel: string;
}) {
  if (windows.length === 0) {
    return <div className="text-terminal-faint">{emptyLabel}</div>;
  }

  return (
    <div className="grid gap-1">
      {windows.map((window) => (
        <div
          key={window.id}
          className={cn(
            "flex items-center gap-2 border px-2 py-1",
            window.focused
              ? "border-terminal-info bg-terminal-bg-active"
              : "border-terminal-border bg-terminal-bg-panel",
          )}
        >
          <span className="text-terminal-fg-strong">
            {window.title ?? window.id}
          </span>
          {window.floating ? (
            <span className="text-terminal-warn">float</span>
          ) : null}
          <span className="text-terminal-faint ml-auto">
            {window.app_id ?? "unknown"}
          </span>
        </div>
      ))}
    </div>
  );
}

function WindowSurface({
  window,
}: {
  window: LayoutWindow | null;
}) {
  return (
    <div className="text-terminal-muted flex h-[calc(100%-1.5rem)] flex-col p-2 text-sm">
      <div>
        <div className="text-terminal-fg-strong">
          {window?.app_id ?? "window"}
        </div>
        <div className="text-terminal-dim mt-1">
          {window?.title ?? "unbound node"}
        </div>
      </div>
    </div>
  );
}

function FootTerminal({ focused }: { focused: boolean }) {
  return (
    <div className="bg-terminal-bg text-terminal-fg flex h-[calc(100%-1.5rem)] items-start px-2 py-2 text-sm">
      <div>
        <span className={focused ? "text-terminal-info" : "text-terminal-faint"}>
          akisarou@spiders
        </span>
        <span className="text-terminal-dim">:</span>
        <span className="text-terminal-wait">~/.config/spiders-wm</span>
        <span className="text-terminal-dim">$ </span>
        <span className="foot-cursor bg-terminal-fg-strong inline-block h-4 w-2 align-[-0.125rem]" />
      </div>
    </div>
  );
}

function DiagnosticsList({
  diagnostics,
}: {
  diagnostics: PreviewDiagnostic[];
}) {
  if (diagnostics.length === 0) {
    return <div className="text-terminal-faint">no diagnostics</div>;
  }

  return (
    <div className="grid gap-1">
      {diagnostics.map((diagnostic, index) => (
        <div
          key={`${diagnostic.source}-${diagnostic.path ?? "root"}-${index}`}
          className="border-terminal-border bg-terminal-bg-panel text-terminal-muted border px-2 py-1"
        >
          <div className="flex items-center gap-2 text-xs">
            <span
              className={
                diagnostic.level === "error"
                  ? "text-terminal-error"
                  : "text-terminal-warn"
              }
            >
              {diagnostic.level}
            </span>
            <span className="text-terminal-dim">{diagnostic.source}</span>
            {diagnostic.path ? (
              <span className="text-terminal-faint ml-auto">
                {diagnostic.path}
              </span>
            ) : null}
          </div>
          <div className="mt-1">{diagnostic.message}</div>
        </div>
      ))}
    </div>
  );
}
