use std::collections::BTreeMap;

use crate::editor_files::{EditorFileId, WORKSPACE_ROOT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorFileTreeDirectory {
    pub name: &'static str,
    pub path: &'static str,
    pub default_open: bool,
    pub download_root_path: Option<&'static str>,
    pub children: Vec<EditorFileTreeNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorFileTreeNode {
    Directory(EditorFileTreeDirectory),
    File(EditorFileId),
}

pub fn initial_open_directories() -> BTreeMap<String, bool> {
    let mut directories = BTreeMap::new();
    collect_default_open_directories(&workspace_file_tree(), &mut directories);
    directories
}

pub fn workspace_file_tree() -> EditorFileTreeDirectory {
    EditorFileTreeDirectory {
        name: WORKSPACE_ROOT,
        path: WORKSPACE_ROOT,
        default_open: true,
        download_root_path: None,
        children: vec![
            EditorFileTreeNode::File(EditorFileId::Config),
            EditorFileTreeNode::File(EditorFileId::RootCss),
            EditorFileTreeNode::Directory(EditorFileTreeDirectory {
                name: "config",
                path: "~/.config/spiders-wm/config",
                default_open: true,
                download_root_path: None,
                children: vec![
                    EditorFileTreeNode::File(EditorFileId::ConfigBindings),
                    EditorFileTreeNode::File(EditorFileId::ConfigInputs),
                    EditorFileTreeNode::File(EditorFileId::ConfigLayouts),
                ],
            }),
            EditorFileTreeNode::Directory(EditorFileTreeDirectory {
                name: "layouts",
                path: "~/.config/spiders-wm/layouts",
                default_open: true,
                download_root_path: None,
                children: vec![
                    EditorFileTreeNode::Directory(EditorFileTreeDirectory {
                        name: "master-stack",
                        path: "~/.config/spiders-wm/layouts/master-stack",
                        default_open: true,
                        download_root_path: Some("~/.config/spiders-wm/layouts/master-stack"),
                        children: vec![
                            EditorFileTreeNode::File(EditorFileId::LayoutTsx),
                            EditorFileTreeNode::File(EditorFileId::LayoutCss),
                        ],
                    }),
                    EditorFileTreeNode::Directory(EditorFileTreeDirectory {
                        name: "focus-repro",
                        path: "~/.config/spiders-wm/layouts/focus-repro",
                        default_open: true,
                        download_root_path: Some("~/.config/spiders-wm/layouts/focus-repro"),
                        children: vec![
                            EditorFileTreeNode::File(EditorFileId::FocusReproLayoutTsx),
                            EditorFileTreeNode::File(EditorFileId::FocusReproLayoutCss),
                        ],
                    }),
                ],
            }),
        ],
    }
}

fn collect_default_open_directories(
    directory: &EditorFileTreeDirectory,
    out: &mut BTreeMap<String, bool>,
) {
    out.insert(directory.path.to_string(), directory.default_open);

    for child in &directory.children {
        if let EditorFileTreeNode::Directory(directory) = child {
            collect_default_open_directories(directory, out);
        }
    }
}
