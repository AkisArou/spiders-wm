// oxlint-disable import/default
import "monaco-editor/min/vs/editor/editor.main.css";
import "monaco-editor/esm/vs/editor/editor.main.js";
import * as monaco from "monaco-editor/esm/vs/editor/editor.api.js";
import * as monacoTypescript from "monaco-editor/esm/vs/language/typescript/monaco.contribution.js";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import CssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import TsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

interface MonacoModel {
  path: string;
  language: string;
  value: string;
}

interface MonacoExtraLib {
  filePath: string;
  content: string;
}

interface MonacoHostHandle {
  host: HTMLElement;
  monaco: typeof monaco;
  editor: monaco.editor.IStandaloneCodeEditor;
  modelPaths: string[];
  activePath: string | null;
  sourceLibs: Map<string, { content: string; dispose: monaco.IDisposable }>;
  changeDisposable?: monaco.IDisposable;
  openerDisposable?: monaco.IDisposable;
}

const monacoTheme = "spiders-terminal";
const workspaceRootUri = "file:///home/demo/.config/spiders-wm";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string) {
    switch (label) {
      case "css":
        return new CssWorker();
      case "typescript":
      case "javascript":
        return new TsWorker();
      default:
        return new EditorWorker();
    }
  },
};

let configured = false;

function ensureMonacoStyles() {
  const styleId = "spiders-wm-monaco-host-css";
  if (document.getElementById(styleId)) return;

  const link = document.createElement("link");
  link.id = styleId;
  link.rel = "stylesheet";
  link.href = "/monaco/spiders-wm-www-monaco-host.css";
  document.head.appendChild(link);
}

function ensureConfigured(extraLibs: MonacoExtraLib[]) {
  ensureMonacoStyles();

  if (!configured) {
    monacoTypescript.javascriptDefaults.setEagerModelSync(true);

    monacoTypescript.typescriptDefaults.setCompilerOptions({
      allowJs: true,
      allowImportingTsExtensions: true,
      allowNonTsExtensions: true,
      allowSyntheticDefaultImports: true,
      baseUrl: workspaceRootUri,
      esModuleInterop: true,
      jsx: monacoTypescript.JsxEmit.ReactJSX,
      jsxImportSource: "@spiders-wm/sdk",
      module: monacoTypescript.ModuleKind.ESNext,
      moduleResolution: monacoTypescript.ModuleResolutionKind.NodeJs,
      paths: {
        "@spiders-wm/sdk": ["./node_modules/@spiders-wm/sdk/index.d.ts"],
        "@spiders-wm/sdk/*": ["./node_modules/@spiders-wm/sdk/*"],
      },
      target: monacoTypescript.ScriptTarget.ESNext,
    });

    monacoTypescript.typescriptDefaults.setDiagnosticsOptions({
      noSemanticValidation: false,
      noSyntaxValidation: false,
    });
    monacoTypescript.typescriptDefaults.setEagerModelSync(true);

    for (const lib of extraLibs) {
      monacoTypescript.typescriptDefaults.addExtraLib(
        lib.content,
        lib.filePath,
      );
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
      },
    });

    configured = true;
  }
}

function syncModels(handle: MonacoHostHandle, models: MonacoModel[]) {
  handle.modelPaths = models.map((model) => model.path);
  const nextTypeScriptPaths = new Set<string>();

  for (const model of models) {
    if (model.language === "typescript") {
      nextTypeScriptPaths.add(model.path);
      const existingSourceLib = handle.sourceLibs.get(model.path);
      if (!existingSourceLib || existingSourceLib.content !== model.value) {
        existingSourceLib?.dispose.dispose();
        handle.sourceLibs.set(model.path, {
          content: model.value,
          dispose: monacoTypescript.typescriptDefaults.addExtraLib(
            model.value,
            model.path,
          ),
        });
      }
    }

    const uri = monaco.Uri.parse(model.path);
    const existingModel = monaco.editor.getModel(uri);
    if (existingModel && existingModel.getValue() !== model.value) {
      existingModel.setValue(model.value);
    }
  }

  for (const [path, sourceLib] of handle.sourceLibs.entries()) {
    if (!nextTypeScriptPaths.has(path)) {
      sourceLib.dispose.dispose();
      handle.sourceLibs.delete(path);
    }
  }
}

function setActiveModel(handle: MonacoHostHandle, activePath: string | null) {
  if (!activePath) return;
  const uri = monaco.Uri.parse(activePath);
  const model = monaco.editor.getModel(uri);
  if (model && handle.editor.getModel() !== model) {
    handle.editor.setModel(model);
  }
}

export async function createMonacoEditor(
  host: HTMLElement,
  activePath: string,
  models: MonacoModel[],
  extraLibs: MonacoExtraLib[],
  onChange: (path: string, value: string) => void,
  onOpen: (payload: string) => void,
) {
  ensureConfigured(extraLibs);

  const activeModel = models.find((model) => model.path === activePath) ?? null;
  const initialValue = activeModel?.value ?? "";
  const initialLanguage = activeModel?.language ?? "typescript";
  const initialUri = activePath ? monaco.Uri.parse(activePath) : null;
  let fileBackedModel = initialUri ? monaco.editor.getModel(initialUri) : null;
  if (!fileBackedModel && initialUri) {
    fileBackedModel = monaco.editor.createModel(
      initialValue,
      initialLanguage,
      initialUri,
    );
  }

  const editor = monaco.editor.create(host, {
    automaticLayout: true,
    contextmenu: true,
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
    smoothScrolling: false,
    tabSize: 2,
    theme: monacoTheme,
    wordWrap: "off",
  });
  editor.updateOptions({ editContext: false });
  if (fileBackedModel) {
    editor.setModel(fileBackedModel);
  }

  const handle: MonacoHostHandle = {
    host,
    monaco,
    editor,
    modelPaths: [],
    activePath: activePath || null,
    sourceLibs: new Map(),
  };

  syncModels(handle, models);
  setActiveModel(handle, activePath || null);

  handle.changeDisposable = editor.onDidChangeModelContent(() => {
    const model = editor.getModel();
    if (model) onChange(model.uri.toString(), model.getValue());
  });
  handle.openerDisposable = monaco.editor.registerEditorOpener({
    openCodeEditor(_source, resource, selectionOrPosition) {
      onOpen(
        JSON.stringify({
          path: resource.toString(),
          selectionOrPosition: selectionOrPosition ?? null,
        }),
      );
      return true;
    },
  });

  (globalThis as any).__spidersMonacoDebug = handle;
  return handle;
}

export function updateMonacoEditor(
  handle: MonacoHostHandle,
  activePath: string,
  models: MonacoModel[],
) {
  handle.activePath = activePath || null;
  syncModels(handle, models);
  setActiveModel(handle, activePath || null);
}

export function revealMonacoPosition(
  handle: MonacoHostHandle,
  lineNumber: number,
  column: number,
) {
  const position = { lineNumber, column };
  handle.editor.setPosition(position);
  handle.editor.revealPositionInCenter(position);
  handle.editor.focus();
}

export function monacoMarkerCount(handle: MonacoHostHandle) {
  const model = handle.editor.getModel();
  if (!model) return 0;
  return handle.monaco.editor.getModelMarkers({ resource: model.uri }).length;
}

export function disposeMonacoEditor(handle: MonacoHostHandle) {
  handle.changeDisposable?.dispose();
  handle.openerDisposable?.dispose();
  handle.editor.dispose();
  for (const path of handle.modelPaths) {
    monaco.editor.getModel(monaco.Uri.parse(path))?.dispose();
  }
  for (const sourceLib of handle.sourceLibs.values()) {
    sourceLib.dispose.dispose();
  }
}
