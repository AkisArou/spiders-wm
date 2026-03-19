use smithay::utils::{Logical, Point, Rectangle, Size};
use spiders_shared::{
    layout::LayoutRect,
    wm::{WindowMode as SharedWindowMode, WindowSnapshot},
};

use crate::model::{OutputId, WorkspaceId};

pub fn layout_rect_from_rectangle(rect: Rectangle<i32, Logical>) -> LayoutRect {
    LayoutRect {
        x: rect.loc.x as f32,
        y: rect.loc.y as f32,
        width: rect.size.w as f32,
        height: rect.size.h as f32,
    }
}

pub fn rectangle_from_layout_rect(rect: LayoutRect) -> Rectangle<i32, Logical> {
    Rectangle::new(
        Point::from((rect.x.round() as i32, rect.y.round() as i32)),
        Size::from((rect.width.round() as i32, rect.height.round() as i32)),
    )
}

#[derive(Debug, Clone)]
pub struct ManagedWindowState {
    pub id: crate::model::WindowId,
    pub workspace: WorkspaceId,
    pub output: Option<OutputId>,
    pub mode: WindowMode,
    pub mapped: bool,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowMode {
    Tiled,
    Floating { rect: Rectangle<i32, Logical> },
    Fullscreen,
}

impl ManagedWindowState {
    pub fn tiled(
        id: crate::model::WindowId,
        workspace: WorkspaceId,
        output: Option<OutputId>,
    ) -> Self {
        Self {
            id,
            workspace,
            output,
            mode: WindowMode::Tiled,
            mapped: true,
            app_id: None,
            title: None,
        }
    }

    pub fn mode(&self) -> WindowMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: WindowMode) {
        self.mode = mode;
    }

    pub fn snapshot(&self, focused: bool) -> WindowSnapshot {
        WindowSnapshot {
            id: self.id.clone(),
            shell: spiders_shared::wm::ShellKind::XdgToplevel,
            app_id: self.app_id.clone(),
            title: self.title.clone(),
            class: None,
            instance: None,
            role: None,
            window_type: None,
            mapped: self.mapped,
            mode: self.mode.into(),
            focused,
            urgent: false,
            output_id: self.output.clone(),
            workspace_id: Some(self.workspace.clone()),
            workspaces: vec![],
        }
    }

    pub fn rect(
        &self,
        output_rect: Option<Rectangle<i32, Logical>>,
        tiled_rect: Rectangle<i32, Logical>,
    ) -> Option<Rectangle<i32, Logical>> {
        match self.mode {
            WindowMode::Tiled => Some(tiled_rect),
            WindowMode::Floating { rect } => Some(rect),
            WindowMode::Fullscreen => output_rect,
        }
    }
}

impl From<WindowMode> for SharedWindowMode {
    fn from(value: WindowMode) -> Self {
        match value {
            WindowMode::Tiled => Self::Tiled,
            WindowMode::Floating { rect } => Self::Floating {
                rect: Some(layout_rect_from_rectangle(rect)),
            },
            WindowMode::Fullscreen => Self::Fullscreen,
        }
    }
}

impl From<SharedWindowMode> for WindowMode {
    fn from(value: SharedWindowMode) -> Self {
        match value {
            SharedWindowMode::Tiled => Self::Tiled,
            SharedWindowMode::Floating { rect } => Self::Floating {
                rect: rect
                    .map(rectangle_from_layout_rect)
                    .unwrap_or_else(default_floating_rect),
            },
            SharedWindowMode::Fullscreen => Self::Fullscreen,
        }
    }
}

fn default_floating_rect() -> Rectangle<i32, Logical> {
    Rectangle::new(Point::from((80, 80)), Size::from((960, 640)))
}
