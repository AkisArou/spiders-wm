use serde::{Deserialize, Serialize};

use spiders_tree::LayoutRect;

/// Which Wayland/X11 shell protocol a window was created through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellKind {
    XdgToplevel,
    X11,
    Unknown,
}

/// Display-level transform (rotation / flip) reported by an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

/// Reference to a named layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayoutRef {
    pub name: String,
}

/// Tiling mode of a window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WindowMode {
    Tiled,
    Floating {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rect: Option<LayoutRect>,
    },
    Fullscreen,
}

impl Default for WindowMode {
    fn default() -> Self {
        Self::Tiled
    }
}

impl WindowMode {
    pub fn is_floating(self) -> bool {
        matches!(self, Self::Floating { .. })
    }

    pub fn is_fullscreen(self) -> bool {
        matches!(self, Self::Fullscreen)
    }

    pub fn floating_rect(self) -> Option<LayoutRect> {
        match self {
            Self::Floating { rect } => rect,
            Self::Tiled | Self::Fullscreen => None,
        }
    }
}
