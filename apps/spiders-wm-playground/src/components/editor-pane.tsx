import { createElement, useEffect, useRef, type ComponentType } from "react";

import MonacoReactEditor, { type BeforeMount, type OnMount } from "@monaco-editor/react";
import { initVimMode, type VimAdapterInstance } from "monaco-vim";

import { FileTree } from "./file-tree.tsx";
import type { EditorFile, EditorFileId, FileTreeDirectoryNode } from "./playground-types.ts";

const monacoTheme = "spiders-terminal";
const MonacoEditor = MonacoReactEditor as unknown as ComponentType<Record<string, unknown>>;

function themeColor(name: string) {
  if (typeof window === "undefined") {
    return "";
  }

  const value = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return value;
}

const beforeMonacoMount: BeforeMount = (monaco) => {
  monaco.editor.defineTheme(monacoTheme, {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: themeColor("--color-terminal-faint") },
      { token: "keyword", foreground: themeColor("--color-terminal-wait") },
      { token: "string", foreground: themeColor("--color-terminal-info") },
      { token: "number", foreground: themeColor("--color-terminal-warn") },
      { token: "type.identifier", foreground: themeColor("--color-file-ts") },
      { token: "delimiter", foreground: themeColor("--color-terminal-muted") },
    ],
    colors: {
      "editor.background": themeColor("--color-terminal-bg-subtle"),
      "editor.foreground": themeColor("--color-terminal-fg"),
      "editorLineNumber.foreground": themeColor("--color-terminal-faint"),
      "editorLineNumber.activeForeground": themeColor("--color-terminal-dim"),
      "editorCursor.foreground": themeColor("--color-terminal-fg-strong"),
      "editor.selectionBackground": themeColor("--color-terminal-bg-active"),
      "editor.inactiveSelectionBackground": themeColor("--color-terminal-bg-hover"),
      "editorLineHighlightBackground": themeColor("--color-terminal-bg-bar"),
      "editorIndentGuide.background1": themeColor("--color-terminal-border"),
      "editorIndentGuide.activeBackground1": themeColor("--color-terminal-border-strong"),
      "editorWhitespace.foreground": themeColor("--color-terminal-border"),
      "editorGutter.background": themeColor("--color-terminal-bg-subtle"),
    },
  });
};

export function EditorPane({
  files,
  fileTree: root,
  activeFileId,
  buffers,
  onSelectFile,
  onChangeBuffer,
}: {
  files: EditorFile[];
  fileTree: FileTreeDirectoryNode;
  activeFileId: EditorFileId;
  buffers: Record<EditorFileId, string>;
  onSelectFile: (fileId: EditorFileId) => void;
  onChangeBuffer: (fileId: EditorFileId, value: string) => void;
}) {
  const vimStatusRef = useRef<HTMLDivElement | null>(null);
  const vimAdapterRef = useRef<VimAdapterInstance | null>(null);
  const filesById = Object.fromEntries(files.map((file) => [file.id, file])) as Record<
    EditorFileId,
    EditorFile
  >;
  const activeFile = filesById[activeFileId]!;
  const dirtyFiles = new Set(
    files.filter((file) => buffers[file.id] !== file.initialValue).map((file) => file.id),
  );

  const handleEditorMount: OnMount = (editor, monaco) => {
    monaco.editor.setTheme(monacoTheme);

    if (vimAdapterRef.current) {
      vimAdapterRef.current.dispose();
    }

    vimAdapterRef.current = initVimMode(editor, vimStatusRef.current);
  };

  useEffect(() => {
    return () => {
      vimAdapterRef.current?.dispose();
      vimAdapterRef.current = null;
    };
  }, []);

  return (
    <section className="grid h-full min-h-0 w-full min-w-0 grid-cols-1 overflow-hidden border border-terminal-border bg-terminal-bg-subtle lg:grid-cols-[16rem_minmax(0,1fr)]">
      <aside className="min-h-0 overflow-auto border-b border-terminal-border bg-terminal-bg-subtle lg:border-r lg:border-b-0">
        <div className="py-1">
          <FileTree
            node={root}
            filesById={filesById}
            activeFileId={activeFileId}
            dirtyFiles={dirtyFiles}
            onSelect={onSelectFile}
          />
        </div>
      </aside>

      <div className="flex min-h-0 flex-col overflow-hidden">
        <div className="flex items-center gap-px border-b border-terminal-border bg-terminal-bg-bar px-1 pt-1 text-xs">
          {files.map((file) => {
            const active = file.id === activeFileId;

            return (
              <button
                key={file.id}
                type="button"
                className={[
                  "flex items-center gap-1 border border-b-0 px-2 py-1",
                  active
                    ? "border-terminal-border-strong bg-terminal-bg-subtle text-terminal-fg-strong"
                    : "border-terminal-border bg-terminal-bg-panel text-terminal-dim hover:text-terminal-fg",
                ].join(" ")}
                onClick={() => onSelectFile(file.id)}
              >
                <span className={file.iconTone}>{file.icon}</span>
                <span>{file.label}</span>
                {dirtyFiles.has(file.id) ? <span className="text-terminal-warn">+</span> : null}
              </button>
            );
          })}
        </div>

        <div className="border-b border-terminal-border bg-terminal-bg-panel px-2 py-1 text-xs text-terminal-faint">
          {activeFile.path}
        </div>

        <div className="min-h-0 flex-1 overflow-hidden">
          {createElement(MonacoEditor, {
            beforeMount: beforeMonacoMount,
            defaultLanguage: activeFile.language,
            height: "100%",
            language: activeFile.language,
            onChange: (value: string | undefined) => onChangeBuffer(activeFile.id, value ?? ""),
            onMount: handleEditorMount,
            options: {
              automaticLayout: true,
              cursorBlinking: "solid",
              cursorSmoothCaretAnimation: "off",
              fontFamily:
                '"JetBrainsMono Nerd Font", "Symbols Nerd Font Mono", "IBM Plex Mono", monospace',
              fontLigatures: false,
              fontSize: 14,
              glyphMargin: false,
              lineHeight: 20,
              minimap: { enabled: false },
              padding: { top: 8, bottom: 8 },
              renderLineHighlight: "line",
              roundedSelection: false,
              scrollBeyondLastLine: false,
              scrollbar: {
                alwaysConsumeMouseWheel: false,
                horizontalScrollbarSize: 8,
                verticalScrollbarSize: 8,
              },
              smoothScrolling: false,
              tabSize: 2,
              wordWrap: "off",
            },
            path: activeFile.modelPath,
            theme: monacoTheme,
            value: buffers[activeFile.id],
          })}
        </div>

        <div
          ref={vimStatusRef}
          className="border-t border-terminal-border bg-terminal-bg-bar px-2 py-1 text-xs text-terminal-muted"
        />
      </div>
    </section>
  );
}