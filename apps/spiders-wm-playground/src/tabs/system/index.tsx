import type { LayoutContext } from "@spiders-wm/sdk/layout";

import {
  formatBindingToken,
  parseBindingsSource,
} from "../../playground-bindings.ts";
import { cn } from "../../utils/cn.ts";
import type { PreviewComputation } from "../../wasm-preview.ts";
import type { EditorFile } from "../editor/types.ts";

const panelClass =
  "flex min-h-0 flex-col overflow-hidden border border-terminal-border bg-terminal-bg-subtle";
const barClass =
  "flex items-center justify-between border-b border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-dim";

export function SystemPane({
  preview,
  previewError,
  activeFile,
  dirtyFileCount,
  bindingsSource,
  context,
}: {
  preview: PreviewComputation | null;
  previewError: string | null;
  activeFile: EditorFile;
  dirtyFileCount: number;
  bindingsSource: string;
  context: LayoutContext;
}) {
  const bindingState = parseBindingsSource(bindingsSource);
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
        <div className="text-terminal-muted min-h-0 flex-1 overflow-auto p-2 text-sm leading-5">
          <div className="grid gap-1">
            {logLines.map((line, index) => (
              <div
                key={`${line.scope}-${index}`}
                className="border-terminal-border bg-terminal-bg-panel flex gap-3 border px-2 py-1"
              >
                <span
                  className={cn(
                    "w-12 shrink-0",
                    line.level === "error"
                      ? "text-terminal-error"
                      : line.level === "warn"
                        ? "text-terminal-warn"
                        : line.level === "wait"
                          ? "text-terminal-wait"
                          : "text-terminal-info",
                  )}
                >
                  {line.level}
                </span>
                <span className="text-terminal-dim w-16 shrink-0">
                  {line.scope}
                </span>
                <span>{line.message}</span>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="grid min-h-0 gap-2 xl:grid-rows-[auto_minmax(14rem,1fr)_minmax(0,1fr)]">
        <div className={panelClass}>
          <div className={barClass}>system://state</div>
          <div className="text-terminal-muted grid gap-1 p-2 text-sm">
            <div className="border-terminal-border bg-terminal-bg-panel flex justify-between border px-2 py-1">
              <span>workspace</span>
              <span className="text-terminal-fg-strong">
                {context.workspace.name}
              </span>
            </div>
            <div className="border-terminal-border bg-terminal-bg-panel flex justify-between border px-2 py-1">
              <span>focused</span>
              <span className="text-terminal-fg-strong">
                {context.windows.find((window) => window.focused)?.title ??
                  "none"}
              </span>
            </div>
            <div className="border-terminal-border bg-terminal-bg-panel flex justify-between border px-2 py-1">
              <span>dirty</span>
              <span className="text-terminal-fg-strong">{dirtyFileCount}</span>
            </div>
            <div className="border-terminal-border bg-terminal-bg-panel flex justify-between border px-2 py-1">
              <span>preview</span>
              <span className="text-terminal-fg-strong">
                {previewError ? "degraded" : preview ? "ready" : "booting"}
              </span>
            </div>
            <div className="border-terminal-border bg-terminal-bg-panel text-terminal-fg-strong border px-2 py-1">
              {activeFile.path}
            </div>
          </div>
        </div>

        <div className={panelClass}>
          <div className={barClass}>system://bindings</div>
          <div className="text-terminal-muted min-h-0 overflow-auto p-2 text-sm">
            <div className="border-terminal-border bg-terminal-bg-panel mb-2 flex items-center justify-between border px-2 py-1">
              <span>mod</span>
              <span className="text-terminal-fg-strong">
                {formatBindingToken("mod", bindingState.mod)}
              </span>
            </div>
            {bindingState.entries.length ? (
              <div className="grid gap-1">
                {bindingState.entries.map((entry, index) => (
                  <div
                    key={`${entry.chord}-${index}`}
                    className="border-terminal-border bg-terminal-bg-panel grid gap-1 border px-2 py-1"
                  >
                    <div className="text-terminal-fg-strong">{entry.chord}</div>
                    <div className="text-terminal-dim">
                      {entry.commandLabel}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-terminal-faint">no bindings parsed</div>
            )}
          </div>
        </div>

        <div className={panelClass}>
          <div className={barClass}>system://diagnostics</div>
          <div className="text-terminal-muted min-h-0 overflow-auto p-2 text-sm">
            {preview?.diagnostics.length ? (
              <div className="grid gap-1">
                {preview.diagnostics.map((diagnostic, index) => (
                  <div
                    key={`${diagnostic.source}-${diagnostic.path ?? "root"}-${index}`}
                    className="border-terminal-border bg-terminal-bg-panel border px-2 py-1"
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
                      <span className="text-terminal-dim">
                        {diagnostic.source}
                      </span>
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
