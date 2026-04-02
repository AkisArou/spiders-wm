import type { LayoutContext } from "@spiders-wm/sdk/layout";

import type { PreviewComputation } from "../wasm-preview.ts";
import type { EditorFile } from "./playground-types.ts";

const panelClass = "flex min-h-0 flex-col overflow-hidden border border-terminal-border bg-terminal-bg-subtle";
const barClass = "flex items-center justify-between border-b border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-dim";

export function SystemPane({
  preview,
  previewError,
  activeFile,
  dirtyFileCount,
  context,
}: {
  preview: PreviewComputation | null;
  previewError: string | null;
  activeFile: EditorFile;
  dirtyFileCount: number;
  context: LayoutContext;
}) {
  const logLines = [
    {
      level: previewError ? "error" : preview ? "info" : "wait",
      scope: "bindings",
      message: previewError
        ? previewError
        : preview
          ? "spiders-web-bindings returned a preview tree"
          : "waiting for wasm bindings",
    },
    {
      level: dirtyFileCount > 0 ? "warn" : "info",
      scope: "editor",
      message:
        dirtyFileCount > 0
          ? `${dirtyFileCount} modified buffer(s) not persisted`
          : "buffer contents match imported fixtures",
    },
    {
      level: "info",
      scope: "editor",
      message: `active buffer ${activeFile.path}`,
    },
    {
      level: preview?.diagnostics.length ? "warn" : "info",
      scope: "scene",
      message: `${preview?.diagnostics.length ?? 0} diagnostic(s) reported`,
    },
  ];

  return (
    <section className="grid h-full min-h-0 w-full min-w-0 grid-cols-1 gap-2 xl:grid-cols-[minmax(0,1.4fr)_20rem]">
      <div className={panelClass}>
        <div className={barClass}>system://log</div>
        <div className="min-h-0 flex-1 overflow-auto p-2 text-sm leading-5 text-terminal-muted">
          <div className="grid gap-1">
            {logLines.map((line, index) => (
              <div key={`${line.scope}-${index}`} className="flex gap-3 border border-terminal-border bg-terminal-bg-panel px-2 py-1">
                <span
                  className={[
                    "w-12 shrink-0",
                    line.level === "error"
                      ? "text-terminal-error"
                      : line.level === "warn"
                        ? "text-terminal-warn"
                        : line.level === "wait"
                          ? "text-terminal-wait"
                          : "text-terminal-info",
                  ].join(" ")}
                >
                  {line.level}
                </span>
                <span className="w-16 shrink-0 text-terminal-dim">{line.scope}</span>
                <span>{line.message}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="grid min-h-0 gap-2 xl:grid-rows-[auto_auto_minmax(0,1fr)]">
        <div className={panelClass}>
          <div className={barClass}>system://state</div>
          <div className="grid gap-1 p-2 text-sm text-terminal-muted">
            <div className="flex justify-between border border-terminal-border bg-terminal-bg-panel px-2 py-1">
              <span>workspace</span>
              <span className="text-terminal-fg-strong">{context.workspace.name}</span>
            </div>
            <div className="flex justify-between border border-terminal-border bg-terminal-bg-panel px-2 py-1">
              <span>focused</span>
              <span className="text-terminal-fg-strong">
                {context.windows.find((window) => window.focused)?.title ?? "none"}
              </span>
            </div>
            <div className="flex justify-between border border-terminal-border bg-terminal-bg-panel px-2 py-1">
              <span>dirty</span>
              <span className="text-terminal-fg-strong">{dirtyFileCount}</span>
            </div>
            <div className="flex justify-between border border-terminal-border bg-terminal-bg-panel px-2 py-1">
              <span>preview</span>
              <span className="text-terminal-fg-strong">
                {previewError ? "degraded" : preview ? "ready" : "booting"}
              </span>
            </div>
          </div>
        </div>

        <div className={panelClass}>
          <div className={barClass}>system://files</div>
          <div className="grid gap-1 p-2 text-sm text-terminal-muted">
            <div className="border border-terminal-border bg-terminal-bg-panel px-2 py-1 text-terminal-fg-strong">
              {activeFile.path}
            </div>
          </div>
        </div>

        <div className={panelClass}>
          <div className={barClass}>system://diagnostics</div>
          <div className="min-h-0 overflow-auto p-2 text-sm text-terminal-muted">
            {preview?.diagnostics.length ? (
              <div className="grid gap-1">
                {preview.diagnostics.map((diagnostic, index) => (
                  <div
                    key={`${diagnostic.source}-${diagnostic.path ?? "root"}-${index}`}
                    className="border border-terminal-border bg-terminal-bg-panel px-2 py-1"
                  >
                    <div className="flex items-center gap-2 text-xs">
                      <span className={diagnostic.level === "error" ? "text-terminal-error" : "text-terminal-warn"}>
                        {diagnostic.level}
                      </span>
                      <span className="text-terminal-dim">{diagnostic.source}</span>
                    </div>
                    <div className="mt-1">{diagnostic.message}</div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-terminal-faint">no diagnostics</div>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}