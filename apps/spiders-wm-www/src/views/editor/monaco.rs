use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use leptos::html;
use leptos::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::app_state::AppState;
use crate::editor_files::{
    EDITOR_FILES, EditorFileId, WORKSPACE_FS_ROOT, initial_content, model_path,
};

use super::buffers::active_buffer_text;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MonacoModel {
    path: String,
    language: String,
    value: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MonacoExtraLib {
  file_path: String,
    content: &'static str,
}

fn monaco_models(buffers: &BTreeMap<EditorFileId, String>) -> Vec<MonacoModel> {
    EDITOR_FILES
        .iter()
        .map(|file| MonacoModel {
            path: model_path(file.id).to_string(),
            language: file.language.to_string(),
            value: buffers
                .get(&file.id)
                .cloned()
                .unwrap_or_else(|| initial_content(file.id).to_string()),
        })
        .collect()
}

fn sdk_type_libs() -> Vec<MonacoExtraLib> {
  let workspace_node_modules = format!("{WORKSPACE_FS_ROOT}/node_modules/@spiders-wm/sdk");

    vec![
        MonacoExtraLib {
      file_path: format!("{workspace_node_modules}/index.d.ts"),
            content: concat!(
                "export * from \"./api\";\n",
                "export * from \"./commands\";\n",
                "export * from \"./config\";\n",
                "export * from \"./css\";\n",
                "export * from \"./jsx-dev-runtime\";\n",
                "export * from \"./jsx-runtime\";\n",
                "export * from \"./layout\";\n",
            ),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/api.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/api.d.ts"),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/commands.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/commands.d.ts"),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/config.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/config.d.ts"),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/css.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/css.d.ts"),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/jsx-dev-runtime.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/jsx-dev-runtime.d.ts"),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/jsx-runtime.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/jsx-runtime.d.ts"),
        },
        MonacoExtraLib {
          file_path: format!("{workspace_node_modules}/layout.d.ts"),
          content: include_str!("../../../../../packages/spiders-wm-sdk/src/layout.d.ts"),
        },
    ]
}

mod wasm {
    use js_sys::{Function, Promise};
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::JsFuture;

    use super::{MonacoModel, sdk_type_libs};

    #[wasm_bindgen(inline_js = r##"
let monacoPromise;
let monacoConfigured = false;
const monacoTheme = "spiders-terminal";
const monacoCdnRoot = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/+esm";
const workspaceRootUri = "file:///home/demo/.config/spiders-wm";
const monacoWorkerUrls = {
  default: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/editor/editor.worker.js/+esm",
  css: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/css/css.worker.js/+esm",
  html: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/html/html.worker.js/+esm",
  handlebars: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/html/html.worker.js/+esm",
  javascript: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/typescript/ts.worker.js/+esm",
  json: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/json/json.worker.js/+esm",
  less: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/css/css.worker.js/+esm",
  razor: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/html/html.worker.js/+esm",
  scss: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/css/css.worker.js/+esm",
  typescript: "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/esm/vs/language/typescript/ts.worker.js/+esm",
};
const monacoWorkerCache = new Map();

function rawExtraLibs(extraLibs) {
  return Array.isArray(extraLibs) ? extraLibs : [];
}

function ensureMonacoEnvironment() {
  if (globalThis.MonacoEnvironment?.getWorker) {
    return;
  }

  globalThis.MonacoEnvironment = {
    getWorker(_workerId, label) {
      const workerUrl = monacoWorkerUrls[label] ?? monacoWorkerUrls.default;
      let blobUrl = monacoWorkerCache.get(workerUrl);

      if (!blobUrl) {
        const blob = new Blob([`import "${workerUrl}";`], {
          type: "text/javascript",
        });
        blobUrl = URL.createObjectURL(blob);
        monacoWorkerCache.set(workerUrl, blobUrl);
      }

      return new Worker(blobUrl, { type: "module" });
    },
  };
}

async function loadMonaco() {
  if (!monacoPromise) {
    ensureMonacoEnvironment();
    monacoPromise = import(monacoCdnRoot);
  }

  return monacoPromise;
}

function ensureConfigured(monaco, extraLibs) {
  if (!monacoConfigured) {
    const moduleResolutionKind =
      monaco.languages.typescript.ModuleResolutionKind.Bundler ??
      monaco.languages.typescript.ModuleResolutionKind.NodeJs;

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

    for (const lib of rawExtraLibs(extraLibs)) {
      monaco.languages.typescript.typescriptDefaults.addExtraLib(
        lib.content,
        lib.filePath,
      );
    }

    monacoConfigured = true;
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
}

function syncModels(handle, models) {
  const nextModels = Array.isArray(models) ? models : [];
  handle.modelPaths = nextModels.map((model) => model.path);

  for (const model of nextModels) {
    const uri = handle.monaco.Uri.parse(model.path);
    const existingModel = handle.monaco.editor.getModel(uri);

    if (!existingModel) {
      handle.monaco.editor.createModel(model.value, model.language, uri);
      continue;
    }

    if (existingModel.getValue() !== model.value) {
      existingModel.setValue(model.value);
    }
  }
}

function setActiveModel(handle, activePath) {
  if (!activePath) {
    return;
  }

  const uri = handle.monaco.Uri.parse(activePath);
  const model = handle.monaco.editor.getModel(uri);

  if (!model) {
    return;
  }

  if (handle.editor.getModel() !== model) {
    handle.editor.setModel(model);
  }
}

export async function createMonacoEditor(host, activePath, models, extraLibs, onChange, onOpen) {
  const monaco = await loadMonaco();
  ensureConfigured(monaco, extraLibs);

  const handle = {
    monaco,
    modelPaths: [],
  };

  syncModels(handle, models);

  handle.editor = monaco.editor.create(host, {
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
    theme: monacoTheme,
    wordWrap: "off",
  });

  monaco.editor.setTheme(monacoTheme);
  setActiveModel(handle, activePath);

  handle.changeDisposable = handle.editor.onDidChangeModelContent(() => {
    const model = handle.editor.getModel();

    if (!model) {
      return;
    }

    onChange(model.uri.toString(), model.getValue());
  });

  handle.openerDisposable = monaco.editor.registerEditorOpener({
    openCodeEditor(_source, resource) {
      onOpen(resource.toString());
      return true;
    },
  });

  return handle;
}

export function updateMonacoEditor(handle, activePath, models) {
  if (!handle) {
    return;
  }

  syncModels(handle, models);
  setActiveModel(handle, activePath);
}

export function disposeMonacoEditor(handle) {
  if (!handle) {
    return;
  }

  handle.changeDisposable?.dispose();
  handle.openerDisposable?.dispose();
  handle.editor?.dispose();

  for (const path of handle.modelPaths ?? []) {
    const uri = handle.monaco.Uri.parse(path);
    handle.monaco.editor.getModel(uri)?.dispose();
  }
}
"##)]
    extern "C" {
        #[wasm_bindgen(catch, js_name = createMonacoEditor)]
        fn create_monaco_editor_js(
            host: &web_sys::HtmlElement,
            active_path: &str,
            models: JsValue,
            extra_libs: JsValue,
            on_change: &Function,
            on_open: &Function,
        ) -> Result<Promise, JsValue>;

        #[wasm_bindgen(catch, js_name = updateMonacoEditor)]
        fn update_monaco_editor_js(
            handle: &JsValue,
            active_path: &str,
            models: JsValue,
        ) -> Result<(), JsValue>;

        #[wasm_bindgen(catch, js_name = disposeMonacoEditor)]
        fn dispose_monaco_editor_js(handle: &JsValue) -> Result<(), JsValue>;
    }

    pub struct MonacoEditorHandle {
        handle: JsValue,
        _change_callback: Closure<dyn Fn(String, String)>,
        _open_callback: Closure<dyn Fn(String)>,
    }

    impl MonacoEditorHandle {
      pub(super) fn sync(
        &self,
        active_path: Option<&str>,
        models: &[MonacoModel],
      ) -> Result<(), String> {
            let models = serde_wasm_bindgen::to_value(models).map_err(|error| error.to_string())?;
            update_monaco_editor_js(&self.handle, active_path.unwrap_or_default(), models)
                .map_err(js_error_message)
        }
    }

    impl Drop for MonacoEditorHandle {
        fn drop(&mut self) {
            let _ = dispose_monaco_editor_js(&self.handle);
        }
    }

    pub async fn mount_monaco_editor(
        host: web_sys::HtmlElement,
        active_path: Option<&str>,
        models: &[MonacoModel],
        on_change: impl Fn(String, String) + 'static,
        on_open: impl Fn(String) + 'static,
    ) -> Result<MonacoEditorHandle, String> {
        let models = serde_wasm_bindgen::to_value(models).map_err(|error| error.to_string())?;
        let extra_libs = serde_wasm_bindgen::to_value(&sdk_type_libs())
            .map_err(|error| error.to_string())?;
        let change_callback = Closure::wrap(Box::new(on_change) as Box<dyn Fn(String, String)>);
        let open_callback = Closure::wrap(Box::new(on_open) as Box<dyn Fn(String)>);
        let promise = create_monaco_editor_js(
            &host,
            active_path.unwrap_or_default(),
            models,
            extra_libs,
            change_callback.as_ref().unchecked_ref(),
            open_callback.as_ref().unchecked_ref(),
        )
        .map_err(js_error_message)?;
        let handle = JsFuture::from(promise).await.map_err(js_error_message)?;

        Ok(MonacoEditorHandle {
            handle,
            _change_callback: change_callback,
            _open_callback: open_callback,
        })
    }

    fn js_error_message(error: JsValue) -> String {
        error
            .as_string()
            .unwrap_or_else(|| "monaco editor bridge failed".to_string())
    }
}

pub use wasm::MonacoEditorHandle;
use wasm::mount_monaco_editor;

#[component]
pub fn MonacoEditorPane() -> impl IntoView {
    let app_state = expect_context::<AppState>();
    let editor_mount = NodeRef::<html::Div>::new();
    let monaco_error = RwSignal::new(None::<String>);
    let monaco_loading = RwSignal::new(false);
    let _monaco_handle = Rc::new(RefCell::new(None::<MonacoEditorHandle>));

    {
        let editor_mount = editor_mount.clone();
      let monaco_handle = Rc::clone(&_monaco_handle);
        let app_state_for_mount = app_state;
        Effect::new(move |_| {
            let Some(host) = editor_mount.get() else {
                return;
            };

            if monaco_handle.borrow().is_some() || monaco_loading.get() {
                return;
            }

            let models = monaco_models(&app_state_for_mount.editor_buffers.get_untracked());
            let active_path = app_state_for_mount
                .active_file_id
                .get_untracked()
                .map(|file_id| model_path(file_id).to_string());
            let monaco_handle = Rc::clone(&monaco_handle);
            let callback_state = app_state_for_mount;

            monaco_loading.set(true);
            monaco_error.set(None);

            spawn_local(async move {
                let result = mount_monaco_editor(
                  host.into(),
                    active_path.as_deref(),
                    &models,
                    move |path, value| {
                        let Some(file_id) = crate::editor_files::file_id_by_model_path(&path) else {
                            return;
                        };

                        let current_value = callback_state
                            .editor_buffers
                            .get_untracked()
                            .get(&file_id)
                            .cloned()
                            .unwrap_or_else(|| initial_content(file_id).to_string());

                        if current_value != value {
                            callback_state.update_buffer(file_id, value);
                        }
                    },
                    move |path| {
                        if let Some(file_id) = crate::editor_files::file_id_by_model_path(&path) {
                            callback_state.select_editor_file(file_id);
                        }
                    },
                )
                .await;

                match result {
                    Ok(handle) => {
                        *monaco_handle.borrow_mut() = Some(handle);
                    monaco_error.set(None);
                    }
                    Err(error) => {
                        monaco_error.set(Some(error));
                    }
                }

                monaco_loading.set(false);
            });
        });

        let monaco_handle = Rc::clone(&_monaco_handle);
        Effect::new(move |_| {
            let active_path = app_state
                .active_file_id
                .get()
                .map(|file_id| model_path(file_id).to_string());
            let models = monaco_models(&app_state.editor_buffers.get());

            if let Some(handle) = monaco_handle.borrow().as_ref() {
                if let Err(error) = handle.sync(active_path.as_deref(), &models) {
                    monaco_error.set(Some(error));
                }
            }
        });
    }

    if cfg!(target_arch = "wasm32") {
        view! {
            <div class="relative flex-1 min-h-120 bg-[linear-gradient(180deg,rgba(255,255,255,0.015),transparent)]">
                <Show
                    when=move || monaco_error.get().is_none()
                    fallback=move || {
                        view! {
                            <textarea
                                class="flex py-4 px-4 w-full h-full font-mono text-white bg-transparent outline-none resize-none min-h-120 text-[0.94rem] leading-[1.55]"
                                prop:value=move || active_buffer_text(app_state)
                                prop:spellcheck=false
                                on:input=move |event| {
                                    let Some(file_id) = app_state.active_file_id.get_untracked()
                                    else {
                                        return;
                                    };
                                    app_state.update_buffer(file_id, event_target_value(&event));
                                }
                            />
                        }
                    }
                >
                    <div node_ref=editor_mount class="w-full h-full min-h-120" />
                    <Show when=move || monaco_loading.get()>
                        <div class="grid absolute inset-0 place-items-center uppercase pointer-events-none bg-[linear-gradient(180deg,rgba(4,8,12,0.82),rgba(9,17,26,0.78))] text-[0.72rem] tracking-[0.18em] text-slate-400">
                            "loading monaco..."
                        </div>
                    </Show>
                </Show>
            </div>
        }
            .into_any()
    } else {
        view! {
            <textarea
                class="flex-1 py-4 px-4 font-mono text-white outline-none resize-none min-h-120 bg-[linear-gradient(180deg,rgba(255,255,255,0.015),transparent)] text-[0.94rem] leading-[1.55]"
                prop:value=move || active_buffer_text(app_state)
                prop:spellcheck=false
                on:input=move |event| {
                    let Some(file_id) = app_state.active_file_id.get_untracked() else {
                        return;
                    };
                    app_state.update_buffer(file_id, event_target_value(&event));
                }
            />
        }
            .into_any()
    }
}
