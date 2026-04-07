use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EditorFileId {
    Config,
    RootCss,
    ConfigBindings,
    ConfigInputs,
    ConfigLayouts,
    LayoutTsx,
    LayoutCss,
    FocusReproLayoutTsx,
    FocusReproLayoutCss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorFile {
    pub id: EditorFileId,
    pub label: &'static str,
    pub path: &'static str,
    pub language: &'static str,
}

pub const WORKSPACE_ROOT: &str = "~/.config/spiders-wm";
pub const WORKSPACE_FS_ROOT: &str = "/home/demo/.config/spiders-wm";

pub const EDITOR_FILES: [EditorFile; 9] = [
    EditorFile {
        id: EditorFileId::Config,
        label: "config.tsx",
        path: "~/.config/spiders-wm/config.tsx",
        language: "typescriptreact",
    },
    EditorFile {
        id: EditorFileId::RootCss,
        label: "index.css",
        path: "~/.config/spiders-wm/index.css",
        language: "css",
    },
    EditorFile {
        id: EditorFileId::ConfigBindings,
        label: "bindings.ts",
        path: "~/.config/spiders-wm/config/bindings.ts",
        language: "typescript",
    },
    EditorFile {
        id: EditorFileId::ConfigInputs,
        label: "inputs.ts",
        path: "~/.config/spiders-wm/config/inputs.ts",
        language: "typescript",
    },
    EditorFile {
        id: EditorFileId::ConfigLayouts,
        label: "layouts.ts",
        path: "~/.config/spiders-wm/config/layouts.ts",
        language: "typescript",
    },
    EditorFile {
        id: EditorFileId::LayoutTsx,
        label: "index.tsx",
        path: "~/.config/spiders-wm/layouts/master-stack/index.tsx",
        language: "typescriptreact",
    },
    EditorFile {
        id: EditorFileId::LayoutCss,
        label: "index.css",
        path: "~/.config/spiders-wm/layouts/master-stack/index.css",
        language: "css",
    },
    EditorFile {
        id: EditorFileId::FocusReproLayoutTsx,
        label: "index.tsx",
        path: "~/.config/spiders-wm/layouts/focus-repro/index.tsx",
        language: "typescriptreact",
    },
    EditorFile {
        id: EditorFileId::FocusReproLayoutCss,
        label: "index.css",
        path: "~/.config/spiders-wm/layouts/focus-repro/index.css",
        language: "css",
    },
];

pub fn initial_editor_buffers() -> BTreeMap<EditorFileId, String> {
    EDITOR_FILES.iter().map(|file| (file.id, initial_content(file.id).to_string())).collect()
}

pub fn initial_open_editor_files() -> Vec<EditorFileId> {
    vec![EditorFileId::LayoutTsx]
}

pub fn initial_content(file_id: EditorFileId) -> &'static str {
    match file_id {
        EditorFileId::Config => include_str!("../fixtures/spiders-wm/config.tsx"),
        EditorFileId::RootCss => include_str!("../fixtures/spiders-wm/index.css"),
        EditorFileId::ConfigBindings => include_str!("../fixtures/spiders-wm/config/bindings.ts"),
        EditorFileId::ConfigInputs => include_str!("../fixtures/spiders-wm/config/inputs.ts"),
        EditorFileId::ConfigLayouts => include_str!("../fixtures/spiders-wm/config/layouts.ts"),
        EditorFileId::LayoutTsx => {
            include_str!("../fixtures/spiders-wm/layouts/master-stack/index.tsx")
        }
        EditorFileId::LayoutCss => {
            include_str!("../fixtures/spiders-wm/layouts/master-stack/index.css")
        }
        EditorFileId::FocusReproLayoutTsx => {
            include_str!("../fixtures/spiders-wm/layouts/focus-repro/index.tsx")
        }
        EditorFileId::FocusReproLayoutCss => {
            include_str!("../fixtures/spiders-wm/layouts/focus-repro/index.css")
        }
    }
}

pub fn file_by_id(file_id: EditorFileId) -> &'static EditorFile {
    EDITOR_FILES.iter().find(|file| file.id == file_id).expect("editor file id should exist")
}

pub fn runtime_path(file_id: EditorFileId) -> &'static str {
    match file_id {
        EditorFileId::Config => "/home/demo/.config/spiders-wm/config.tsx",
        EditorFileId::RootCss => "/home/demo/.config/spiders-wm/index.css",
        EditorFileId::ConfigBindings => "/home/demo/.config/spiders-wm/config/bindings.ts",
        EditorFileId::ConfigInputs => "/home/demo/.config/spiders-wm/config/inputs.ts",
        EditorFileId::ConfigLayouts => "/home/demo/.config/spiders-wm/config/layouts.ts",
        EditorFileId::LayoutTsx => "/home/demo/.config/spiders-wm/layouts/master-stack/index.tsx",
        EditorFileId::LayoutCss => "/home/demo/.config/spiders-wm/layouts/master-stack/index.css",
        EditorFileId::FocusReproLayoutTsx => {
            "/home/demo/.config/spiders-wm/layouts/focus-repro/index.tsx"
        }
        EditorFileId::FocusReproLayoutCss => {
            "/home/demo/.config/spiders-wm/layouts/focus-repro/index.css"
        }
    }
}

pub fn model_path(file_id: EditorFileId) -> &'static str {
    match file_id {
        EditorFileId::Config => "file:///home/demo/.config/spiders-wm/config.tsx",
        EditorFileId::RootCss => "file:///home/demo/.config/spiders-wm/index.css",
        EditorFileId::ConfigBindings => "file:///home/demo/.config/spiders-wm/config/bindings.ts",
        EditorFileId::ConfigInputs => "file:///home/demo/.config/spiders-wm/config/inputs.ts",
        EditorFileId::ConfigLayouts => "file:///home/demo/.config/spiders-wm/config/layouts.ts",
        EditorFileId::LayoutTsx => {
            "file:///home/demo/.config/spiders-wm/layouts/master-stack/index.tsx"
        }
        EditorFileId::LayoutCss => {
            "file:///home/demo/.config/spiders-wm/layouts/master-stack/index.css"
        }
        EditorFileId::FocusReproLayoutTsx => {
            "file:///home/demo/.config/spiders-wm/layouts/focus-repro/index.tsx"
        }
        EditorFileId::FocusReproLayoutCss => {
            "file:///home/demo/.config/spiders-wm/layouts/focus-repro/index.css"
        }
    }
}

pub fn file_id_by_model_path(path: &str) -> Option<EditorFileId> {
    EDITOR_FILES.iter().find(|file| model_path(file.id) == path).map(|file| file.id)
}
