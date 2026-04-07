use anyhow::Result;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt as _};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Atoms {
    pub(crate) wm_protocols: Atom,
    pub(crate) wm_delete_window: Atom,
    pub(crate) wm_state: Atom,
    pub(crate) wm_name: Atom,
    pub(crate) wm_class: Atom,
    pub(crate) utf8_string: Atom,
    pub(crate) net_wm_name: Atom,
    pub(crate) net_supported: Atom,
    pub(crate) net_supporting_wm_check: Atom,
    pub(crate) net_active_window: Atom,
    pub(crate) net_client_list: Atom,
    pub(crate) net_client_list_stacking: Atom,
    pub(crate) net_current_desktop: Atom,
    pub(crate) net_number_of_desktops: Atom,
    pub(crate) net_desktop_names: Atom,
    pub(crate) net_wm_desktop: Atom,
    pub(crate) net_wm_state: Atom,
    pub(crate) net_wm_state_fullscreen: Atom,
    pub(crate) net_wm_state_hidden: Atom,
    pub(crate) net_wm_state_focused: Atom,
    pub(crate) net_close_window: Atom,
    pub(crate) net_moveresize_window: Atom,
    pub(crate) net_restack_window: Atom,
    pub(crate) net_workarea: Atom,
}

impl Atoms {
    pub(crate) fn intern_all<C: Connection>(connection: &C) -> Result<Self> {
        Ok(Self {
            wm_protocols: intern_atom(connection, b"WM_PROTOCOLS")?,
            wm_delete_window: intern_atom(connection, b"WM_DELETE_WINDOW")?,
            wm_state: intern_atom(connection, b"WM_STATE")?,
            wm_name: intern_atom(connection, b"WM_NAME")?,
            wm_class: intern_atom(connection, b"WM_CLASS")?,
            utf8_string: intern_atom(connection, b"UTF8_STRING")?,
            net_wm_name: intern_atom(connection, b"_NET_WM_NAME")?,
            net_supported: intern_atom(connection, b"_NET_SUPPORTED")?,
            net_supporting_wm_check: intern_atom(connection, b"_NET_SUPPORTING_WM_CHECK")?,
            net_active_window: intern_atom(connection, b"_NET_ACTIVE_WINDOW")?,
            net_client_list: intern_atom(connection, b"_NET_CLIENT_LIST")?,
            net_client_list_stacking: intern_atom(connection, b"_NET_CLIENT_LIST_STACKING")?,
            net_current_desktop: intern_atom(connection, b"_NET_CURRENT_DESKTOP")?,
            net_number_of_desktops: intern_atom(connection, b"_NET_NUMBER_OF_DESKTOPS")?,
            net_desktop_names: intern_atom(connection, b"_NET_DESKTOP_NAMES")?,
            net_wm_desktop: intern_atom(connection, b"_NET_WM_DESKTOP")?,
            net_wm_state: intern_atom(connection, b"_NET_WM_STATE")?,
            net_wm_state_fullscreen: intern_atom(connection, b"_NET_WM_STATE_FULLSCREEN")?,
            net_wm_state_hidden: intern_atom(connection, b"_NET_WM_STATE_HIDDEN")?,
            net_wm_state_focused: intern_atom(connection, b"_NET_WM_STATE_FOCUSED")?,
            net_close_window: intern_atom(connection, b"_NET_CLOSE_WINDOW")?,
            net_moveresize_window: intern_atom(connection, b"_NET_MOVERESIZE_WINDOW")?,
            net_restack_window: intern_atom(connection, b"_NET_RESTACK_WINDOW")?,
            net_workarea: intern_atom(connection, b"_NET_WORKAREA")?,
        })
    }

    pub(crate) fn known_atom_count(&self) -> usize {
        let atoms = [
            self.wm_protocols,
            self.wm_delete_window,
            self.wm_state,
            self.wm_name,
            self.wm_class,
            self.utf8_string,
            self.net_wm_name,
            self.net_supported,
            self.net_supporting_wm_check,
            self.net_active_window,
            self.net_client_list,
            self.net_client_list_stacking,
            self.net_current_desktop,
            self.net_number_of_desktops,
            self.net_desktop_names,
            self.net_wm_desktop,
            self.net_wm_state,
            self.net_wm_state_fullscreen,
            self.net_wm_state_hidden,
            self.net_wm_state_focused,
            self.net_close_window,
            self.net_moveresize_window,
            self.net_restack_window,
            self.net_workarea,
        ];

        atoms.into_iter().filter(|atom| *atom != u32::from(AtomEnum::NONE)).count()
    }
}

fn intern_atom<C: Connection>(connection: &C, name: &[u8]) -> Result<Atom> {
    Ok(connection.intern_atom(false, name)?.reply()?.atom)
}
