use xcb::x;

xcb::atoms_struct! {
    #[derive(Debug, Clone, Copy)]
    pub(crate) struct Atoms {
        pub wm_protocols => b"WM_PROTOCOLS" only_if_exists = false,
        pub wm_delete_window => b"WM_DELETE_WINDOW" only_if_exists = false,
        pub wm_state => b"WM_STATE" only_if_exists = false,
        pub wm_name => b"WM_NAME" only_if_exists = false,
        pub wm_class => b"WM_CLASS" only_if_exists = false,
        pub utf8_string => b"UTF8_STRING" only_if_exists = false,
        pub net_wm_name => b"_NET_WM_NAME" only_if_exists = false,
        pub net_supported => b"_NET_SUPPORTED" only_if_exists = false,
        pub net_supporting_wm_check => b"_NET_SUPPORTING_WM_CHECK" only_if_exists = false,
        pub net_active_window => b"_NET_ACTIVE_WINDOW" only_if_exists = false,
        pub net_client_list => b"_NET_CLIENT_LIST" only_if_exists = false,
        pub net_client_list_stacking => b"_NET_CLIENT_LIST_STACKING" only_if_exists = false,
        pub net_current_desktop => b"_NET_CURRENT_DESKTOP" only_if_exists = false,
        pub net_number_of_desktops => b"_NET_NUMBER_OF_DESKTOPS" only_if_exists = false,
        pub net_desktop_names => b"_NET_DESKTOP_NAMES" only_if_exists = false,
        pub net_wm_desktop => b"_NET_WM_DESKTOP" only_if_exists = false,
        pub net_wm_state => b"_NET_WM_STATE" only_if_exists = false,
        pub net_wm_state_fullscreen => b"_NET_WM_STATE_FULLSCREEN" only_if_exists = false,
        pub net_wm_state_hidden => b"_NET_WM_STATE_HIDDEN" only_if_exists = false,
        pub net_wm_state_focused => b"_NET_WM_STATE_FOCUSED" only_if_exists = false,
        pub net_close_window => b"_NET_CLOSE_WINDOW" only_if_exists = false,
        pub net_moveresize_window => b"_NET_MOVERESIZE_WINDOW" only_if_exists = false,
        pub net_restack_window => b"_NET_RESTACK_WINDOW" only_if_exists = false,
        pub net_workarea => b"_NET_WORKAREA" only_if_exists = false,
    }
}

impl Atoms {
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

        atoms.into_iter().filter(|atom| *atom != x::ATOM_NONE).count()
    }
}
