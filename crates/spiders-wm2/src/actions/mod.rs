pub mod floating;
pub mod focus;
pub mod fullscreen;
pub mod output;
pub mod window;
pub mod workspace;

pub use floating::{set_floating_rect, toggle_floating_focused_window};
pub use focus::{
    focus_next_window, focus_previous_window, focus_window, next_focus_in_active_workspace,
};
pub use fullscreen::toggle_fullscreen_focused_window;
pub use output::{register_output, sync_active_workspace_to_output, update_output_logical_size};
pub use window::{
    begin_window_removal, place_new_window_in_active_workspace, register_window, remove_window,
    swap_focused_window_with_next, swap_focused_window_with_previous, update_window_metadata,
};
pub use workspace::{
    active_workspace_windows, move_focused_window_to_workspace, switch_to_workspace,
};
