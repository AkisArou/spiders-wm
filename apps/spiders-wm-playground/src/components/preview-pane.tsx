import type { LayoutContext, LayoutWindow } from "@spiders-wm/sdk/layout";

import type {
  PreviewComputation,
  PreviewDiagnostic,
  PreviewSnapshotNode,
  PreviewTreeNode,
} from "../wasm-preview.ts";

const panelClass = "flex min-h-0 flex-col overflow-hidden border border-terminal-border bg-terminal-bg-subtle";
const barClass = "flex items-center justify-between border-b border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-dim";

export function previewPane({
  preview,
  previewError,
  context,
}: {
  preview: PreviewComputation | null;
  previewError: string | null;
  context: LayoutContext;
}) {
  return (
    <section className="grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2 xl:grid-cols-[minmax(0,1.55fr)_22rem]">
      <div className={panelClass}>
        <div className={barClass}>
          <span>
            preview://live  ::  {context.monitor.name}  ::  {context.monitor.width}x{context.monitor.height}
          </span>
          <span>{context.windows.length} windows</span>
        </div>

        {previewError ? (
          <div className="p-3 text-sm text-terminal-error">{previewError}</div>
        ) : preview?.treeRoot ? (
          <div className="grid min-h-0 flex-1 grid-rows-[minmax(18rem,1fr)_minmax(12rem,0.9fr)]">
            <div className="min-h-0 overflow-hidden p-2">
              {preview.snapshotRoot ? (
                <GeometryPreview
                  root={preview.snapshotRoot}
                  fallbackWidth={context.monitor.width}
                  fallbackHeight={context.monitor.height}
                />
              ) : (
                <div className="flex h-full items-center justify-center border border-terminal-border bg-terminal-bg-subtle text-sm text-terminal-faint">
                  no snapshot
                </div>
              )}
            </div>

            <div className="min-h-0 overflow-auto border-t border-terminal-border p-2">
              <LayoutTreeNode node={preview.treeRoot} />
            </div>
          </div>
        ) : (
          <div className="p-3 text-sm text-terminal-faint">loading wasm preview...</div>
        )}
      </div>

      <div className="grid min-h-0 gap-2 xl:grid-rows-[auto_auto_minmax(0,1fr)]">
        <div className={panelClass}>
          <div className={barClass}>session://windows</div>
          <div className="grid gap-3 p-2 text-sm text-terminal-muted">
            <WindowList windows={context.windows} emptyLabel="no windows" />
          </div>
        </div>

        <div className={panelClass}>
          <div className={barClass}>session://unclaimed</div>
          <div className="p-2 text-sm text-terminal-muted">
            <WindowList windows={preview?.unclaimedWindows ?? []} emptyLabel="all claimed" />
          </div>
        </div>

        <div className={panelClass}>
          <div className={barClass}>scene://diagnostics</div>
          <div className="min-h-0 overflow-auto p-2 text-sm">
            <DiagnosticsList diagnostics={preview?.diagnostics ?? []} />
          </div>
        </div>
      </div>
    </section>
  );
}

export function PreviewPane({
  preview,
  previewError,
  context,
}: {
  preview: PreviewComputation | null;
  previewError: string | null;
  context: LayoutContext;
}) {
  return previewPane({ preview, previewError, context });
}

function GeometryPreview({
  root,
  fallbackWidth,
  fallbackHeight,
}: {
  root: PreviewSnapshotNode;
  fallbackWidth: number;
  fallbackHeight: number;
}) {
  const stageWidth = root.rect.width || fallbackWidth;
  const stageHeight = root.rect.height || fallbackHeight;

  return (
    <div
      className="relative h-full min-h-72 w-full overflow-hidden border border-terminal-border bg-terminal-bg-subtle"
      style={{
        aspectRatio: `${stageWidth} / ${stageHeight}`,
        backgroundImage:
          "linear-gradient(color-mix(in srgb, var(--color-terminal-grid) 35%, transparent) 1px, transparent 1px), linear-gradient(90deg, color-mix(in srgb, var(--color-terminal-grid) 35%, transparent) 1px, transparent 1px)",
        backgroundSize: "24px 24px",
      }}
    >
      {root.children.map((child, index) => (
        <GeometryNode
          key={`${child.id ?? child.type}-${index}`}
          node={child}
          stageWidth={stageWidth}
          stageHeight={stageHeight}
        />
      ))}
    </div>
  );
}

function GeometryNode({
  node,
  stageWidth,
  stageHeight,
}: {
  node: PreviewSnapshotNode;
  stageWidth: number;
  stageHeight: number;
}) {
  const left = (node.rect.x / stageWidth) * 100;
  const top = (node.rect.y / stageHeight) * 100;
  const width = (node.rect.width / stageWidth) * 100;
  const height = (node.rect.height / stageHeight) * 100;

  return (
    <div
      className={[
        "absolute overflow-hidden border text-xs text-terminal-fg",
        node.type === "window"
          ? "border-terminal-warn bg-terminal-warn/10"
          : "border-terminal-grid bg-terminal-grid/10",
      ].join(" ")}
      style={{
        left: `${left}%`,
        top: `${top}%`,
        width: `${width}%`,
        height: `${height}%`,
      }}
    >
      <div className="flex items-center justify-between border-b border-current/20 bg-terminal-bg-subtle/80 px-1 py-0.5 text-xs text-terminal-dim">
        <span>{node.id ?? node.type}</span>
        <span>
          {Math.round(node.rect.width)}x{Math.round(node.rect.height)}
        </span>
      </div>

      {node.children.map((child, index) => (
        <GeometryNode
          key={`${child.id ?? child.type}-${index}`}
          node={child}
          stageWidth={stageWidth}
          stageHeight={stageHeight}
        />
      ))}
    </div>
  );
}

function LayoutTreeNode({ node, depth = 0 }: { node: PreviewTreeNode; depth?: number }) {
  return (
    <div className="text-sm leading-5 text-terminal-muted">
      <div
        className="flex items-center gap-2 border border-terminal-border bg-terminal-bg-panel px-2 py-1"
        style={{ marginLeft: `${depth * 12}px` }}
      >
        <span className="text-terminal-dim">{node.type}</span>
        <span className="text-terminal-fg-strong">{node.id ?? "_"}</span>
        {node.rect ? (
          <span className="text-terminal-faint">
            {Math.round(node.rect.width)}x{Math.round(node.rect.height)}
          </span>
        ) : null}
        <span className="ml-auto text-terminal-faint">{node.claimedWindows.length}</span>
      </div>

      <div className="mt-1 grid gap-1">
        {node.claimedWindows.length > 0 ? (
          <div className="text-xs text-terminal-faint" style={{ marginLeft: `${depth * 12 + 12}px` }}>
            {node.claimedWindows.map((window) => window.title ?? window.id).join("  |  ")}
          </div>
        ) : null}

        {node.children.map((child) => (
          <LayoutTreeNode key={child.path} node={child} depth={depth + 1} />
        ))}
      </div>
    </div>
  );
}

function WindowList({ windows, emptyLabel }: { windows: LayoutWindow[]; emptyLabel: string }) {
  if (windows.length === 0) {
    return <div className="text-terminal-faint">{emptyLabel}</div>;
  }

  return (
    <div className="grid gap-1">
      {windows.map((window) => (
        <div key={window.id} className="flex items-center gap-2 border border-terminal-border bg-terminal-bg-panel px-2 py-1">
          <span className="text-terminal-fg-strong">{window.title ?? window.id}</span>
          <span className="ml-auto text-terminal-faint">{window.app_id ?? "unknown"}</span>
        </div>
      ))}
    </div>
  );
}

function DiagnosticsList({ diagnostics }: { diagnostics: PreviewDiagnostic[] }) {
  if (diagnostics.length === 0) {
    return <div className="text-terminal-faint">no diagnostics</div>;
  }

  return (
    <div className="grid gap-1">
      {diagnostics.map((diagnostic, index) => (
        <div
          key={`${diagnostic.source}-${diagnostic.path ?? "root"}-${index}`}
          className="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-terminal-muted"
        >
          <div className="flex items-center gap-2 text-xs">
            <span className={diagnostic.level === "error" ? "text-terminal-error" : "text-terminal-warn"}>
              {diagnostic.level}
            </span>
            <span className="text-terminal-dim">{diagnostic.source}</span>
            {diagnostic.path ? <span className="ml-auto text-terminal-faint">{diagnostic.path}</span> : null}
          </div>
          <div className="mt-1">{diagnostic.message}</div>
        </div>
      ))}
    </div>
  );
}