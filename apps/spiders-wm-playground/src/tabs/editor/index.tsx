import {
  createElement,
  useEffect,
  useRef,
  useState,
  type ComponentType,
  type MutableRefObject,
} from "react";

import MonacoReactEditor, {
  type BeforeMount,
  type OnMount,
} from "@monaco-editor/react";
import { initVimMode, type VimAdapterInstance } from "monaco-vim";

import * as sdkApiTypesModule from "@spiders-wm/sdk/api.d.ts?raw";
import * as sdkCommandsTypesModule from "@spiders-wm/sdk/commands.d.ts?raw";
import * as sdkConfigTypesModule from "@spiders-wm/sdk/config.d.ts?raw";
import * as sdkCssTypesModule from "@spiders-wm/sdk/css.d.ts?raw";
import * as sdkJsxDevRuntimeTypesModule from "@spiders-wm/sdk/jsx-dev-runtime.d.ts?raw";
import * as sdkJsxRuntimeTypesModule from "@spiders-wm/sdk/jsx-runtime.d.ts?raw";
import * as sdkLayoutTypesModule from "@spiders-wm/sdk/layout.d.ts?raw";

import { copyTextToClipboard } from "./clipboard.ts";
import { downloadDirectoryNode } from "./download.ts";
import { FileTree } from "./file-tree.tsx";
import type {
  EditorFile,
  EditorFileId,
  FileTreeDirectoryNode,
} from "./types.ts";
import { cn } from "../../utils/cn.ts";

const monacoTheme = "spiders-terminal";
const workspaceRootUri = "file:///home/demo/.config/spiders-wm";
const workspaceNodeModulesUri = `${workspaceRootUri}/node_modules/@spiders-wm/sdk`;
const MonacoEditor = MonacoReactEditor as unknown as ComponentType<
  Record<string, unknown>
>;
let monacoEnvironmentConfigured = false;

const sdkTypeLibs = [
  {
    filePath: `${workspaceNodeModulesUri}/index.d.ts`,
    content: [
      'export * from "./api";',
      'export * from "./commands";',
      'export * from "./config";',
      'export * from "./css";',
      'export * from "./jsx-dev-runtime";',
      'export * from "./jsx-runtime";',
      'export * from "./layout";',
    ].join("\n"),
  },
  {
    filePath: `${workspaceNodeModulesUri}/api.d.ts`,
    content: rawModuleSource(sdkApiTypesModule),
  },
  {
    filePath: `${workspaceNodeModulesUri}/commands.d.ts`,
    content: rawModuleSource(sdkCommandsTypesModule),
  },
  {
    filePath: `${workspaceNodeModulesUri}/config.d.ts`,
    content: rawModuleSource(sdkConfigTypesModule),
  },
  {
    filePath: `${workspaceNodeModulesUri}/css.d.ts`,
    content: rawModuleSource(sdkCssTypesModule),
  },
  {
    filePath: `${workspaceNodeModulesUri}/jsx-dev-runtime.d.ts`,
    content: rawModuleSource(sdkJsxDevRuntimeTypesModule),
  },
  {
    filePath: `${workspaceNodeModulesUri}/jsx-runtime.d.ts`,
    content: rawModuleSource(sdkJsxRuntimeTypesModule),
  },
  {
    filePath: `${workspaceNodeModulesUri}/layout.d.ts`,
    content: rawModuleSource(sdkLayoutTypesModule),
  },
];

const beforeMonacoMount: BeforeMount = (monaco) => {
  if (!monacoEnvironmentConfigured) {
    const moduleResolutionKind =
      (
        monaco.languages.typescript.ModuleResolutionKind as Record<
          string,
          number
        >
      ).Bundler ?? monaco.languages.typescript.ModuleResolutionKind.NodeJs;

    monaco.languages.typescript.typescriptDefaults.setCompilerOptions({
      allowJs: true,
      allowImportingTsExtensions: true,
      allowNonTsExtensions: true,
      allowSyntheticDefaultImports: true,
      baseUrl: workspaceRootUri,
      esModuleInterop: true,
      jsx: monaco.languages.typescript.JsxEmit.ReactJSX,
      jsxImportSource: "@spiders-wm/sdk",
      module: monaco.languages.typescript.ModuleKind.ESNext,
      moduleResolution: moduleResolutionKind,
      paths: {
        "@spiders-wm/sdk": ["./node_modules/@spiders-wm/sdk/index.d.ts"],
        "@spiders-wm/sdk/*": ["./node_modules/@spiders-wm/sdk/*"],
      },
      resolveJsonModule: true,
      target: monaco.languages.typescript.ScriptTarget.ES2022,
    });

    monaco.languages.typescript.typescriptDefaults.setDiagnosticsOptions({
      noSemanticValidation: false,
      noSyntaxValidation: false,
    });

    monaco.languages.typescript.typescriptDefaults.setEagerModelSync(true);

    for (const lib of sdkTypeLibs) {
      monaco.languages.typescript.typescriptDefaults.addExtraLib(
        lib.content,
        lib.filePath,
      );
    }

    monacoEnvironmentConfigured = true;
  }

  monaco.editor.defineTheme(monacoTheme, {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "6A9955" },
      { token: "keyword", foreground: "569CD6" },
      { token: "string", foreground: "CE9178" },
      { token: "number", foreground: "B5CEA8" },
      { token: "type.identifier", foreground: "4EC9B0" },
      { token: "delimiter", foreground: "D4D4D4" },
    ],
    colors: {
      "editor.background": "#1F1F1F",
      "editor.foreground": "#D4D4D4",
      "editorLineNumber.foreground": "#858585",
      "editorLineNumber.activeForeground": "#C6C6C6",
      "editorCursor.foreground": "#AEAFAD",
      "editor.selectionBackground": "#264F78",
      "editor.inactiveSelectionBackground": "#3A3D41",
      editorLineHighlightBackground: "#2A2D2E",
      "editorIndentGuide.background1": "#404040",
      "editorIndentGuide.activeBackground1": "#707070",
      "editorWhitespace.foreground": "#3B3B3B",
      "editorGutter.background": "#1F1F1F",
      "editorBracketMatch.border": "#888888",
    },
  });
};

export function EditorPane({
  files,
  openFileIds,
  fileTree: root,
  activeFileId,
  buffers,
  vimEnabled,
  onSelectFile,
  onToggleVimMode,
  onCloseFile,
  onCloseOtherFiles,
  onCloseAllFiles,
  onChangeBuffer,
}: {
  files: EditorFile[];
  openFileIds: EditorFileId[];
  fileTree: FileTreeDirectoryNode;
  activeFileId: EditorFileId | null;
  buffers: Record<EditorFileId, string>;
  vimEnabled: boolean;
  onSelectFile: (fileId: EditorFileId) => void;
  onToggleVimMode: () => void;
  onCloseFile: (fileId: EditorFileId) => void;
  onCloseOtherFiles: (fileId: EditorFileId) => void;
  onCloseAllFiles: () => void;
  onChangeBuffer: (fileId: EditorFileId, value: string) => void;
}) {
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const vimStatusRef = useRef<HTMLDivElement | null>(null);
  const vimAdapterRef = useRef<VimAdapterInstance | null>(null);
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null);
  const openerDisposableRef = useRef<{ dispose: () => void } | null>(null);
  const pendingNavigationRef = useRef<PendingNavigation | null>(null);
  const [tabContextMenu, setTabContextMenu] =
    useState<TabContextMenuState | null>(null);
  const [copyFeedback, setCopyFeedback] = useState<
    "idle" | "copied" | "failed"
  >("idle");
  const copyFeedbackTimeoutRef = useRef<number | null>(null);
  const filesById = Object.fromEntries(
    files.map((file) => [file.id, file]),
  ) as Record<EditorFileId, EditorFile>;
  const openFiles = openFileIds
    .map((fileId) => filesById[fileId])
    .filter(Boolean);
  const activeFile = activeFileId ? (filesById[activeFileId] ?? null) : null;
  const dirtyFiles = new Set(
    files
      .filter((file) => buffers[file.id] !== file.initialValue)
      .map((file) => file.id),
  );

  const handleEditorMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    monaco.editor.setTheme(monacoTheme);
    syncEditorModels(monaco, files, buffers);
    openerDisposableRef.current?.dispose();
    openerDisposableRef.current = monaco.editor.registerEditorOpener({
      openCodeEditor: (
        _source: unknown,
        resource: { toString: () => string },
        selectionOrPosition?: EditorSelectionLike,
      ) => {
        const file = findFileByUri(files, resource.toString());

        if (!file) {
          return false;
        }

        pendingNavigationRef.current = {
          resource: resource.toString(),
          selectionOrPosition: normalizeSelection(selectionOrPosition),
        };
        onSelectFile(file.id);
        return true;
      },
    });
    syncVimMode(editor, vimStatusRef.current, vimAdapterRef, vimEnabled);
  };

  useEffect(() => {
    const monaco = monacoRef.current;

    if (!monaco) {
      return;
    }

    syncEditorModels(monaco, files, buffers);
  }, [activeFile, buffers, files]);

  useEffect(() => {
    const editor = editorRef.current;

    if (!editor) {
      return;
    }

    const pendingNavigation = pendingNavigationRef.current;

    if (!activeFile || !pendingNavigation) {
      return;
    }

    if (pendingNavigation.resource !== activeFile.modelPath) {
      return;
    }

    applyNavigation(editor, pendingNavigation.selectionOrPosition);
    pendingNavigationRef.current = null;
  }, [activeFile]);

  useEffect(() => {
    const editor = editorRef.current;

    if (!editor) {
      return;
    }

    syncVimMode(editor, vimStatusRef.current, vimAdapterRef, vimEnabled);
  }, [vimEnabled]);

  useEffect(() => {
    return () => {
      if (copyFeedbackTimeoutRef.current !== null) {
        window.clearTimeout(copyFeedbackTimeoutRef.current);
      }

      openerDisposableRef.current?.dispose();
      openerDisposableRef.current = null;
      vimAdapterRef.current?.dispose();
      vimAdapterRef.current = null;
      editorRef.current = null;

      const monaco = monacoRef.current;

      if (!monaco) {
        return;
      }

      for (const file of files) {
        monaco.editor.getModel(monaco.Uri.parse(file.modelPath))?.dispose();
      }

      monacoRef.current = null;
    };
  }, [files]);

  useEffect(() => {
    setCopyFeedback("idle");
  }, [activeFileId]);

  useEffect(() => {
    if (!tabContextMenu) {
      return;
    }

    const onPointerDown = () => {
      setTabContextMenu(null);
    };

    window.addEventListener("pointerdown", onPointerDown);

    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
    };
  }, [tabContextMenu]);

  return (
    <section className="border-terminal-border bg-terminal-bg-subtle relative grid h-full min-h-0 w-full min-w-0 grid-cols-1 overflow-hidden border lg:grid-cols-[16rem_minmax(0,1fr)]">
      <aside className="border-terminal-border bg-terminal-bg-subtle min-h-0 overflow-auto border-b lg:border-r lg:border-b-0">
        <div className="py-1">
          <FileTree
            node={root}
            filesById={filesById}
            activeFileId={activeFileId}
            dirtyFiles={dirtyFiles}
            onSelect={onSelectFile}
            onDownloadDirectory={async (node) => {
              await downloadDirectoryNode(node, filesById, buffers);
            }}
          />
        </div>
      </aside>

      <div className="flex min-h-0 flex-col overflow-hidden">
        <div className="border-terminal-border bg-terminal-bg-bar flex items-center gap-px border-b px-1 pt-1 text-xs">
          {openFiles.map((file) => {
            const active = file.id === activeFileId;

            return (
              <div
                key={file.id}
                className={cn(
                  "flex items-center border border-b-0",
                  active
                    ? "border-terminal-border-strong bg-terminal-bg-subtle text-terminal-fg-strong"
                    : "border-terminal-border bg-terminal-bg-panel text-terminal-dim hover:text-terminal-fg",
                )}
                onContextMenu={(event) => {
                  event.preventDefault();
                  setTabContextMenu({
                    fileId: file.id,
                    x: event.clientX,
                    y: event.clientY,
                  });
                }}
              >
                <button
                  type="button"
                  className="flex items-center gap-1 px-2 py-1"
                  onClick={() => onSelectFile(file.id)}
                >
                  <span className={file.iconTone}>{file.icon}</span>
                  <span>{file.label}</span>
                  {dirtyFiles.has(file.id) ? (
                    <span className="text-terminal-warn">+</span>
                  ) : null}
                </button>
                <button
                  type="button"
                  className="text-terminal-faint hover:text-terminal-fg px-1.5 py-1"
                  aria-label={`Close ${file.label}`}
                  onClick={(event) => {
                    event.stopPropagation();
                    onCloseFile(file.id);
                  }}
                >
                  x
                </button>
              </div>
            );
          })}

          <button
            type="button"
            className={cn(
              "mr-1 ml-auto flex items-center gap-1 border px-2 py-1",
              vimEnabled
                ? "border-terminal-info bg-terminal-info/10 text-terminal-info"
                : "border-terminal-border bg-terminal-bg-panel text-terminal-dim hover:text-terminal-fg",
            )}
            onClick={onToggleVimMode}
          >
            vim {vimEnabled ? "on" : "off"}
          </button>
        </div>

        <div className="border-terminal-border bg-terminal-bg-panel text-terminal-faint flex items-center justify-between gap-2 border-b px-2 py-1 text-xs">
          <span className="truncate">{activeFile?.path ?? "no file open"}</span>
          <button
            type="button"
            className="border-terminal-border bg-terminal-bg-bar text-terminal-dim hover:text-terminal-fg shrink-0 border px-2 py-0.5"
            disabled={!activeFile}
            onClick={async () => {
              if (!activeFile) {
                return;
              }

              try {
                await copyTextToClipboard(buffers[activeFile.id]);
                setCopyFeedback("copied");
              } catch {
                setCopyFeedback("failed");
              }

              if (copyFeedbackTimeoutRef.current !== null) {
                window.clearTimeout(copyFeedbackTimeoutRef.current);
              }

              copyFeedbackTimeoutRef.current = window.setTimeout(() => {
                setCopyFeedback("idle");
                copyFeedbackTimeoutRef.current = null;
              }, 1500);
            }}
          >
            {copyFeedback === "copied"
              ? "copied"
              : copyFeedback === "failed"
                ? "copy failed"
                : "copy"}
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-hidden">
          {activeFile ? (
            createElement(MonacoEditor, {
              beforeMount: beforeMonacoMount,
              defaultLanguage: activeFile.language,
              height: "100%",
              language: activeFile.language,
              onChange: (value: string | undefined) =>
                onChangeBuffer(activeFile.id, value ?? ""),
              onMount: handleEditorMount,
              options: {
                automaticLayout: true,
                contextmenu: true,
                cursorBlinking: "solid",
                cursorSmoothCaretAnimation: "off",
                definitionLinkOpensInPeek: false,
                fontFamily:
                  '"JetBrainsMono Nerd Font", "Symbols Nerd Font Mono", "IBM Plex Mono", monospace',
                fontLigatures: false,
                fontSize: 14,
                glyphMargin: false,
                gotoLocation: {
                  multipleDeclarations: "peek",
                  multipleDefinitions: "peek",
                  multipleImplementations: "peek",
                  multipleReferences: "peek",
                  multipleTypeDefinitions: "peek",
                },
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
            })
          ) : (
            <div className="text-terminal-faint grid h-full place-items-center text-sm">
              no file open
            </div>
          )}
        </div>

        <div
          ref={vimStatusRef}
          className="border-terminal-border bg-terminal-bg-bar text-terminal-muted border-t px-2 py-1 text-xs"
        />
      </div>

      {tabContextMenu ? (
        <div
          className="border-terminal-border bg-terminal-bg-panel fixed z-30 min-w-40 border py-1 text-sm shadow-[0_14px_40px_rgba(0,0,0,0.45)]"
          style={{
            left: `${tabContextMenu.x}px`,
            top: `${tabContextMenu.y}px`,
          }}
          onPointerDown={(event) => {
            event.stopPropagation();
          }}
        >
          <button
            type="button"
            className="text-terminal-muted hover:bg-terminal-bg-hover hover:text-terminal-fg flex w-full items-center px-3 py-1.5 text-left"
            onClick={() => {
              onCloseFile(tabContextMenu.fileId);
              setTabContextMenu(null);
            }}
          >
            <span>Close</span>
          </button>
          <button
            type="button"
            className="text-terminal-muted hover:bg-terminal-bg-hover hover:text-terminal-fg flex w-full items-center px-3 py-1.5 text-left"
            onClick={() => {
              onCloseOtherFiles(tabContextMenu.fileId);
              setTabContextMenu(null);
            }}
          >
            <span>Close others</span>
          </button>
          <button
            type="button"
            className="text-terminal-muted hover:bg-terminal-bg-hover hover:text-terminal-fg flex w-full items-center px-3 py-1.5 text-left"
            onClick={() => {
              onCloseAllFiles();
              setTabContextMenu(null);
            }}
          >
            <span>Close all</span>
          </button>
        </div>
      ) : null}
    </section>
  );
}

function syncEditorModels(
  monaco: Parameters<OnMount>[1],
  files: EditorFile[],
  buffers: Record<EditorFileId, string>,
) {
  for (const file of files) {
    const uri = monaco.Uri.parse(file.modelPath);
    const nextValue = buffers[file.id];
    const model = monaco.editor.getModel(uri);

    if (!model) {
      monaco.editor.createModel(nextValue, file.language, uri);
      continue;
    }

    if (model.getValue() !== nextValue) {
      model.setValue(nextValue);
    }
  }
}

function rawModuleSource(module: string | { default?: string }): string {
  if (typeof module === "string") {
    return module;
  }

  return typeof module.default === "string" ? module.default : "";
}

function syncVimMode(
  editor: Parameters<OnMount>[0],
  statusNode: HTMLDivElement | null,
  vimAdapterRef: MutableRefObject<VimAdapterInstance | null>,
  vimEnabled: boolean,
) {
  if (!vimEnabled) {
    vimAdapterRef.current?.dispose();
    vimAdapterRef.current = null;
    editor.updateOptions({ cursorStyle: "line" });
    return;
  }

  vimAdapterRef.current?.dispose();
  vimAdapterRef.current = initVimMode(editor, statusNode);
}

type PendingNavigation = {
  resource: string;
  selectionOrPosition: EditorSelectionLike | null;
};

type EditorSelectionLike = {
  startLineNumber: number;
  startColumn: number;
  endLineNumber?: number;
  endColumn?: number;
};

type TabContextMenuState = {
  fileId: EditorFileId;
  x: number;
  y: number;
};

function normalizeSelection(
  selectionOrPosition: EditorSelectionLike | null | undefined,
): EditorSelectionLike | null {
  if (!selectionOrPosition) {
    return null;
  }

  if (
    "endLineNumber" in selectionOrPosition &&
    typeof selectionOrPosition.endLineNumber === "number"
  ) {
    return selectionOrPosition;
  }

  return {
    startLineNumber: selectionOrPosition.startLineNumber,
    startColumn: selectionOrPosition.startColumn,
    endLineNumber: selectionOrPosition.startLineNumber,
    endColumn: selectionOrPosition.startColumn,
  };
}

function applyNavigation(
  editor: Parameters<OnMount>[0],
  selectionOrPosition: EditorSelectionLike | null,
) {
  if (!selectionOrPosition) {
    return;
  }

  editor.setPosition({
    lineNumber: selectionOrPosition.startLineNumber,
    column: selectionOrPosition.startColumn,
  });
  editor.revealPositionInCenter({
    lineNumber: selectionOrPosition.startLineNumber,
    column: selectionOrPosition.startColumn,
  });
  editor.focus();
}

function findFileByUri(files: EditorFile[], uri: string): EditorFile | null {
  return files.find((file) => file.modelPath === uri) ?? null;
}
