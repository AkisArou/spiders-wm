#[cfg(feature = "smithay-winit")]
mod imp {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use smithay::reexports::wayland_protocols::ext::workspace::v1::server::{
        ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1, GroupCapabilities},
        ext_workspace_handle_v1::{
            self, ExtWorkspaceHandleV1, State as WorkspaceState, WorkspaceCapabilities,
        },
        ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
    };
    use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
    use smithay::reexports::wayland_server::{
        Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, Weak,
        backend::{ClientId, GlobalId},
    };
    use spiders_shared::ids::{OutputId, WorkspaceId};
    use spiders_shared::wm::StateSnapshot;

    pub trait WorkspaceHandler:
        GlobalDispatch<ExtWorkspaceManagerV1, WorkspaceManagerGlobalData>
        + Dispatch<ExtWorkspaceManagerV1, ()>
        + Dispatch<ExtWorkspaceGroupHandleV1, WorkspaceGroupHandle>
        + Dispatch<ExtWorkspaceHandleV1, WorkspaceHandle>
        + 'static
    {
        fn workspace_manager_state(&mut self) -> &mut WorkspaceManagerState;
        fn activate_workspace(&mut self, workspace_id: &WorkspaceId);
        fn assign_workspace(&mut self, workspace_id: &WorkspaceId, output_id: &OutputId);
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WorkspaceManagerDebugSnapshot {
        pub manager_count: usize,
        pub group_output_ids: Vec<OutputId>,
        pub group_instance_counts: Vec<usize>,
        pub workspace_names: Vec<String>,
        pub workspace_instance_counts: Vec<usize>,
        pub workspace_group_output_ids: Vec<Option<OutputId>>,
        pub workspace_states: Vec<u32>,
        pub output_binding_counts: Vec<usize>,
    }

    #[derive(Debug)]
    pub struct WorkspaceManagerState {
        #[allow(dead_code)]
        global: GlobalId,
        managers: Vec<ExtWorkspaceManagerV1>,
        groups: HashMap<OutputId, WorkspaceGroupHandle>,
        workspaces: HashMap<WorkspaceId, WorkspaceHandle>,
        output_bindings: HashMap<OutputId, Vec<Weak<WlOutput>>>,
    }

    #[derive(Debug)]
    pub struct WorkspaceManagerGlobalData;

    #[derive(Debug, Clone)]
    pub struct WorkspaceGroupHandle {
        inner: Arc<Mutex<WorkspaceGroupHandleInner>>,
    }

    #[derive(Debug)]
    struct WorkspaceGroupHandleInner {
        output_id: OutputId,
        instances: Vec<Weak<ExtWorkspaceGroupHandleV1>>,
        removed: bool,
    }

    #[derive(Debug, Clone)]
    pub struct WorkspaceHandle {
        inner: Arc<Mutex<WorkspaceHandleInner>>,
    }

    #[derive(Debug)]
    struct WorkspaceHandleInner {
        workspace_id: WorkspaceId,
        stable_id: String,
        name: String,
        state: WorkspaceState,
        group_output_id: Option<OutputId>,
        instances: Vec<Weak<ExtWorkspaceHandleV1>>,
        removed: bool,
    }

    #[derive(Debug, Clone)]
    struct WorkspaceRecord {
        workspace_id: WorkspaceId,
        stable_id: String,
        name: String,
        state: WorkspaceState,
        group_output_id: Option<OutputId>,
    }

    impl WorkspaceManagerState {
        pub fn new<D: WorkspaceHandler>(dh: &DisplayHandle) -> Self {
            Self {
                global: dh
                    .create_global::<D, ExtWorkspaceManagerV1, _>(1, WorkspaceManagerGlobalData),
                managers: Vec::new(),
                groups: HashMap::new(),
                workspaces: HashMap::new(),
                output_bindings: HashMap::new(),
            }
        }

        pub fn debug_snapshot(&self) -> WorkspaceManagerDebugSnapshot {
            let mut group_output_ids: Vec<_> = self.groups.keys().cloned().collect();
            group_output_ids.sort();

            let mut workspaces: Vec<_> = self.workspaces.values().cloned().collect();
            workspaces.sort_by_key(|workspace| workspace.workspace_id());

            let group_instance_counts = group_output_ids
                .iter()
                .filter_map(|output_id| self.groups.get(output_id))
                .map(|group| group.instance_count())
                .collect();
            let workspace_instance_counts = workspaces
                .iter()
                .map(|workspace| workspace.instance_count())
                .collect();
            let output_binding_counts = group_output_ids
                .iter()
                .map(|output_id| {
                    self.output_bindings
                        .get(output_id)
                        .map(Vec::len)
                        .unwrap_or(0)
                })
                .collect();

            WorkspaceManagerDebugSnapshot {
                manager_count: self.managers.len(),
                group_output_ids,
                group_instance_counts,
                workspace_names: workspaces
                    .iter()
                    .map(|workspace| workspace.name())
                    .collect(),
                workspace_instance_counts,
                workspace_group_output_ids: workspaces
                    .iter()
                    .map(|workspace| workspace.group_output_id())
                    .collect(),
                workspace_states: workspaces
                    .iter()
                    .map(|workspace| workspace.state().bits())
                    .collect(),
                output_binding_counts,
            }
        }

        #[cfg(test)]
        fn bind_manager_for_client<D: WorkspaceHandler>(
            &mut self,
            dh: &DisplayHandle,
            client: &Client,
        ) -> ExtWorkspaceManagerV1 {
            let manager = client
                .create_resource::<ExtWorkspaceManagerV1, _, D>(dh, 1, ())
                .unwrap();

            let groups: Vec<_> = self.groups.values().cloned().collect();
            for group in &groups {
                let resource = client
                    .create_resource::<ExtWorkspaceGroupHandleV1, _, D>(dh, 1, group.clone())
                    .unwrap();
                manager.workspace_group(&resource);
                group.init_new_instance(resource);
                self.send_group_outputs_to_client(group, client);
            }

            let workspaces: Vec<_> = self.workspaces.values().cloned().collect();
            for workspace in &workspaces {
                let resource = client
                    .create_resource::<ExtWorkspaceHandleV1, _, D>(dh, 1, workspace.clone())
                    .unwrap();
                manager.workspace(&resource);
                workspace.init_new_instance(resource);
            }

            for workspace in &workspaces {
                if let Some(output_id) = workspace.group_output_id() {
                    if let Some(group) = self.groups.get(&output_id) {
                        group.send_workspace_enter(workspace);
                    }
                }
            }

            manager.done();
            self.managers.push(manager.clone());
            manager
        }

        pub fn output_bound(&mut self, output_id: &OutputId, wl_output: &WlOutput) {
            self.output_bindings
                .entry(output_id.clone())
                .or_default()
                .push(wl_output.downgrade());

            let Some(client) = wl_output.client() else {
                return;
            };

            if let Some(group) = self.groups.get(output_id) {
                for resource in group.resources_for_client(&client) {
                    resource.output_enter(wl_output);
                }
            }
        }

        pub fn refresh_from_snapshot<D: WorkspaceHandler>(
            &mut self,
            dh: &DisplayHandle,
            snapshot: &StateSnapshot,
        ) {
            let desired_group_ids: Vec<_> = snapshot
                .outputs
                .iter()
                .filter(|output| output.enabled)
                .map(|output| output.id.clone())
                .collect();

            self.sync_groups::<D>(dh, &desired_group_ids);

            let desired_workspaces: HashMap<_, _> = snapshot
                .workspaces
                .iter()
                .map(|workspace| {
                    (
                        workspace.id.clone(),
                        WorkspaceRecord {
                            workspace_id: workspace.id.clone(),
                            stable_id: workspace.id.to_string(),
                            name: workspace.name.clone(),
                            state: workspace_state_bits(snapshot, workspace),
                            group_output_id: workspace.output_id.clone(),
                        },
                    )
                })
                .collect();

            let existing_workspace_ids: Vec<_> = self.workspaces.keys().cloned().collect();
            for workspace_id in existing_workspace_ids {
                if desired_workspaces.contains_key(&workspace_id) {
                    continue;
                }

                if let Some(handle) = self.workspaces.remove(&workspace_id) {
                    if let Some(output_id) = handle.group_output_id() {
                        if let Some(group) = self.groups.get(&output_id) {
                            group.send_workspace_leave(&handle);
                        }
                    }
                    handle.send_removed();
                }
            }

            for record in desired_workspaces.values() {
                match self.workspaces.get(&record.workspace_id).cloned() {
                    Some(handle) => {
                        let previous_group = handle.group_output_id();
                        if previous_group != record.group_output_id {
                            if let Some(output_id) = previous_group {
                                if let Some(group) = self.groups.get(&output_id) {
                                    group.send_workspace_leave(&handle);
                                }
                            }
                            handle.set_group_output_id(record.group_output_id.clone());
                            if let Some(output_id) = &record.group_output_id {
                                if let Some(group) = self.groups.get(output_id) {
                                    group.send_workspace_enter(&handle);
                                }
                            }
                        }
                        handle.send_name(&record.name);
                        handle.send_state(record.state);
                    }
                    None => {
                        let handle = WorkspaceHandle::new(
                            record.workspace_id.clone(),
                            record.stable_id.clone(),
                            record.name.clone(),
                            record.state,
                            record.group_output_id.clone(),
                        );
                        self.announce_workspace::<D>(dh, &handle);
                        if let Some(output_id) = &record.group_output_id {
                            if let Some(group) = self.groups.get(output_id) {
                                group.send_workspace_enter(&handle);
                            }
                        }
                        self.workspaces.insert(record.workspace_id.clone(), handle);
                    }
                }
            }

            self.send_done();
        }

        pub fn refresh_output_groups<D: WorkspaceHandler>(
            &mut self,
            dh: &DisplayHandle,
            output_ids: &[OutputId],
        ) {
            self.sync_groups::<D>(dh, output_ids);
            self.send_done();
        }

        fn sync_groups<D: WorkspaceHandler>(
            &mut self,
            dh: &DisplayHandle,
            desired_group_ids: &[OutputId],
        ) {
            for output_id in desired_group_ids {
                if !self.groups.contains_key(output_id) {
                    let group = WorkspaceGroupHandle::new(output_id.clone());
                    self.announce_group::<D>(dh, &group);
                    self.groups.insert(output_id.clone(), group);
                }
            }

            let existing_group_ids: Vec<_> = self.groups.keys().cloned().collect();
            for output_id in existing_group_ids {
                if desired_group_ids
                    .iter()
                    .any(|desired| desired == &output_id)
                {
                    continue;
                }

                if let Some(group) = self.groups.remove(&output_id) {
                    for workspace in self.workspaces.values() {
                        if workspace.group_output_id().as_ref() == Some(&output_id) {
                            group.send_workspace_leave(workspace);
                            workspace.set_group_output_id(None);
                        }
                    }
                    group.send_removed();
                }
            }
        }

        fn announce_group<D: WorkspaceHandler>(
            &mut self,
            dh: &DisplayHandle,
            handle: &WorkspaceGroupHandle,
        ) {
            let managers = self.managers.clone();
            for manager in managers {
                let Ok(client) = dh.get_client(manager.id()) else {
                    continue;
                };

                if let Ok(resource) = client.create_resource::<ExtWorkspaceGroupHandleV1, _, D>(
                    dh,
                    manager.version(),
                    handle.clone(),
                ) {
                    manager.workspace_group(&resource);
                    handle.init_new_instance(resource);
                    self.send_group_outputs_to_client(handle, &client);
                }
            }
        }

        fn announce_workspace<D: WorkspaceHandler>(
            &mut self,
            dh: &DisplayHandle,
            handle: &WorkspaceHandle,
        ) {
            let managers = self.managers.clone();
            for manager in managers {
                let Ok(client) = dh.get_client(manager.id()) else {
                    continue;
                };

                if let Ok(resource) = client.create_resource::<ExtWorkspaceHandleV1, _, D>(
                    dh,
                    manager.version(),
                    handle.clone(),
                ) {
                    manager.workspace(&resource);
                    handle.init_new_instance(resource);
                }
            }
        }

        fn send_group_outputs_to_client(&mut self, handle: &WorkspaceGroupHandle, client: &Client) {
            let output_id = handle.output_id();
            let Some(bindings) = self.output_bindings.get_mut(&output_id) else {
                return;
            };
            bindings.retain(|binding| binding.is_alive());

            for group_resource in handle.resources_for_client(client) {
                for binding in bindings.iter() {
                    if let Ok(wl_output) = binding.upgrade() {
                        if wl_output.client().as_ref() == Some(client) {
                            group_resource.output_enter(&wl_output);
                        }
                    }
                }
            }
        }

        fn send_done(&self) {
            for manager in &self.managers {
                manager.done();
            }
        }
    }

    impl WorkspaceGroupHandle {
        fn new(output_id: OutputId) -> Self {
            Self {
                inner: Arc::new(Mutex::new(WorkspaceGroupHandleInner {
                    output_id,
                    instances: Vec::new(),
                    removed: false,
                })),
            }
        }

        fn output_id(&self) -> OutputId {
            self.inner.lock().unwrap().output_id.clone()
        }

        fn init_new_instance(&self, resource: ExtWorkspaceGroupHandleV1) {
            resource.capabilities(GroupCapabilities::empty());
            self.inner
                .lock()
                .unwrap()
                .instances
                .push(resource.downgrade());
        }

        fn resources(&self) -> Vec<ExtWorkspaceGroupHandleV1> {
            self.inner
                .lock()
                .unwrap()
                .instances
                .iter()
                .filter_map(|instance| instance.upgrade().ok())
                .collect()
        }

        fn resources_for_client(&self, client: &Client) -> Vec<ExtWorkspaceGroupHandleV1> {
            self.resources()
                .into_iter()
                .filter(|resource| {
                    resource
                        .client()
                        .as_ref()
                        .is_some_and(|bound| bound == client)
                })
                .collect()
        }

        fn instance_count(&self) -> usize {
            self.inner.lock().unwrap().instances.len()
        }

        fn send_workspace_enter(&self, workspace: &WorkspaceHandle) {
            for group_resource in self.resources() {
                let Some(client) = group_resource.client() else {
                    continue;
                };
                if let Some(workspace_resource) =
                    workspace.resources_for_client(&client).into_iter().next()
                {
                    group_resource.workspace_enter(&workspace_resource);
                }
            }
        }

        fn send_workspace_leave(&self, workspace: &WorkspaceHandle) {
            for group_resource in self.resources() {
                let Some(client) = group_resource.client() else {
                    continue;
                };
                if let Some(workspace_resource) =
                    workspace.resources_for_client(&client).into_iter().next()
                {
                    group_resource.workspace_leave(&workspace_resource);
                }
            }
        }

        fn remove_instance(&self, resource: &ExtWorkspaceGroupHandleV1) {
            let mut inner = self.inner.lock().unwrap();
            if let Some(index) = inner
                .instances
                .iter()
                .position(|instance| instance == resource)
            {
                inner.instances.remove(index);
            }
        }

        fn send_removed(&self) {
            let mut inner = self.inner.lock().unwrap();
            if inner.removed {
                return;
            }
            inner.removed = true;
            for instance in inner.instances.drain(..) {
                if let Ok(resource) = instance.upgrade() {
                    resource.removed();
                }
            }
        }
    }

    impl WorkspaceHandle {
        fn new(
            workspace_id: WorkspaceId,
            stable_id: String,
            name: String,
            state: WorkspaceState,
            group_output_id: Option<OutputId>,
        ) -> Self {
            Self {
                inner: Arc::new(Mutex::new(WorkspaceHandleInner {
                    workspace_id,
                    stable_id,
                    name,
                    state,
                    group_output_id,
                    instances: Vec::new(),
                    removed: false,
                })),
            }
        }

        fn workspace_id(&self) -> WorkspaceId {
            self.inner.lock().unwrap().workspace_id.clone()
        }

        fn name(&self) -> String {
            self.inner.lock().unwrap().name.clone()
        }

        fn state(&self) -> WorkspaceState {
            self.inner.lock().unwrap().state
        }

        fn group_output_id(&self) -> Option<OutputId> {
            self.inner.lock().unwrap().group_output_id.clone()
        }

        fn set_group_output_id(&self, output_id: Option<OutputId>) {
            self.inner.lock().unwrap().group_output_id = output_id;
        }

        fn init_new_instance(&self, resource: ExtWorkspaceHandleV1) {
            let inner = self.inner.lock().unwrap();
            resource.id(inner.stable_id.clone());
            resource.name(inner.name.clone());
            resource.state(inner.state);
            resource.capabilities(WorkspaceCapabilities::Activate | WorkspaceCapabilities::Assign);
            drop(inner);
            self.inner
                .lock()
                .unwrap()
                .instances
                .push(resource.downgrade());
        }

        fn resources(&self) -> Vec<ExtWorkspaceHandleV1> {
            self.inner
                .lock()
                .unwrap()
                .instances
                .iter()
                .filter_map(|instance| instance.upgrade().ok())
                .collect()
        }

        fn resources_for_client(&self, client: &Client) -> Vec<ExtWorkspaceHandleV1> {
            self.resources()
                .into_iter()
                .filter(|resource| {
                    resource
                        .client()
                        .as_ref()
                        .is_some_and(|bound| bound == client)
                })
                .collect()
        }

        fn instance_count(&self) -> usize {
            self.inner.lock().unwrap().instances.len()
        }

        fn send_name(&self, name: &str) {
            let mut inner = self.inner.lock().unwrap();
            if inner.name == name {
                return;
            }
            inner.name = name.to_string();
            for instance in &inner.instances {
                if let Ok(resource) = instance.upgrade() {
                    resource.name(name.to_string());
                }
            }
        }

        fn send_state(&self, state: WorkspaceState) {
            let mut inner = self.inner.lock().unwrap();
            if inner.state == state {
                return;
            }
            inner.state = state;
            for instance in &inner.instances {
                if let Ok(resource) = instance.upgrade() {
                    resource.state(state);
                }
            }
        }

        fn remove_instance(&self, resource: &ExtWorkspaceHandleV1) {
            let mut inner = self.inner.lock().unwrap();
            if let Some(index) = inner
                .instances
                .iter()
                .position(|instance| instance == resource)
            {
                inner.instances.remove(index);
            }
        }

        fn send_removed(&self) {
            let mut inner = self.inner.lock().unwrap();
            if inner.removed {
                return;
            }
            inner.removed = true;
            for instance in inner.instances.drain(..) {
                if let Ok(resource) = instance.upgrade() {
                    resource.removed();
                }
            }
        }
    }

    impl<D> GlobalDispatch<ExtWorkspaceManagerV1, WorkspaceManagerGlobalData, D>
        for WorkspaceManagerState
    where
        D: WorkspaceHandler,
    {
        fn bind(
            state: &mut D,
            dh: &DisplayHandle,
            client: &Client,
            resource: New<ExtWorkspaceManagerV1>,
            _global_data: &WorkspaceManagerGlobalData,
            data_init: &mut DataInit<'_, D>,
        ) {
            let manager = data_init.init(resource, ());
            let workspace_state = state.workspace_manager_state();

            let groups: Vec<_> = workspace_state.groups.values().cloned().collect();
            for group in &groups {
                if let Ok(resource) = client.create_resource::<ExtWorkspaceGroupHandleV1, _, D>(
                    dh,
                    manager.version(),
                    group.clone(),
                ) {
                    manager.workspace_group(&resource);
                    group.init_new_instance(resource);
                    workspace_state.send_group_outputs_to_client(group, client);
                }
            }

            let workspaces: Vec<_> = workspace_state.workspaces.values().cloned().collect();
            for workspace in &workspaces {
                if let Ok(resource) = client.create_resource::<ExtWorkspaceHandleV1, _, D>(
                    dh,
                    manager.version(),
                    workspace.clone(),
                ) {
                    manager.workspace(&resource);
                    workspace.init_new_instance(resource);
                }
            }

            for workspace in &workspaces {
                if let Some(output_id) = workspace.group_output_id() {
                    if let Some(group) = workspace_state.groups.get(&output_id) {
                        group.send_workspace_enter(workspace);
                    }
                }
            }

            manager.done();
            workspace_state.managers.push(manager);
        }
    }

    impl<D> Dispatch<ExtWorkspaceManagerV1, (), D> for WorkspaceManagerState
    where
        D: WorkspaceHandler,
    {
        fn request(
            state: &mut D,
            _client: &Client,
            manager: &ExtWorkspaceManagerV1,
            request: ext_workspace_manager_v1::Request,
            _data: &(),
            _dh: &DisplayHandle,
            _data_init: &mut DataInit<'_, D>,
        ) {
            match request {
                ext_workspace_manager_v1::Request::Commit => {}
                ext_workspace_manager_v1::Request::Stop => {
                    state
                        .workspace_manager_state()
                        .managers
                        .retain(|instance| instance != manager);
                    manager.finished();
                }
                _ => unreachable!(),
            }
        }

        fn destroyed(
            state: &mut D,
            _client_id: ClientId,
            manager: &ExtWorkspaceManagerV1,
            _data: &(),
        ) {
            state
                .workspace_manager_state()
                .managers
                .retain(|instance| instance != manager);
        }
    }

    impl<D> Dispatch<ExtWorkspaceGroupHandleV1, WorkspaceGroupHandle, D> for WorkspaceManagerState
    where
        D: WorkspaceHandler,
    {
        fn request(
            _state: &mut D,
            _client: &Client,
            _resource: &ExtWorkspaceGroupHandleV1,
            request: ext_workspace_group_handle_v1::Request,
            _data: &WorkspaceGroupHandle,
            _dh: &DisplayHandle,
            _data_init: &mut DataInit<'_, D>,
        ) {
            match request {
                ext_workspace_group_handle_v1::Request::Destroy => {}
                ext_workspace_group_handle_v1::Request::CreateWorkspace { .. } => {}
                _ => unreachable!(),
            }
        }

        fn destroyed(
            _state: &mut D,
            _client_id: ClientId,
            resource: &ExtWorkspaceGroupHandleV1,
            data: &WorkspaceGroupHandle,
        ) {
            data.remove_instance(resource);
        }
    }

    impl<D> Dispatch<ExtWorkspaceHandleV1, WorkspaceHandle, D> for WorkspaceManagerState
    where
        D: WorkspaceHandler,
    {
        fn request(
            state: &mut D,
            _client: &Client,
            _resource: &ExtWorkspaceHandleV1,
            request: ext_workspace_handle_v1::Request,
            data: &WorkspaceHandle,
            _dh: &DisplayHandle,
            _data_init: &mut DataInit<'_, D>,
        ) {
            match request {
                ext_workspace_handle_v1::Request::Destroy => {}
                ext_workspace_handle_v1::Request::Activate => {
                    state.activate_workspace(&data.workspace_id());
                }
                ext_workspace_handle_v1::Request::Deactivate => {}
                ext_workspace_handle_v1::Request::Assign { workspace_group } => {
                    if let Some(group) = workspace_group.data::<WorkspaceGroupHandle>() {
                        state.assign_workspace(&data.workspace_id(), &group.output_id());
                    }
                }
                ext_workspace_handle_v1::Request::Remove => {}
                _ => unreachable!(),
            }
        }

        fn destroyed(
            _state: &mut D,
            _client_id: ClientId,
            resource: &ExtWorkspaceHandleV1,
            data: &WorkspaceHandle,
        ) {
            data.remove_instance(resource);
        }
    }

    fn workspace_state_bits(
        snapshot: &StateSnapshot,
        workspace: &spiders_shared::wm::WorkspaceSnapshot,
    ) -> WorkspaceState {
        let mut bits = WorkspaceState::empty();
        if workspace.visible {
            bits |= WorkspaceState::Active;
        }
        if !workspace.visible {
            bits |= WorkspaceState::Hidden;
        }
        if snapshot
            .windows
            .iter()
            .any(|window| window.workspace_id.as_ref() == Some(&workspace.id) && window.urgent)
        {
            bits |= WorkspaceState::Urgent;
        }
        bits
    }

    #[macro_export]
    macro_rules! delegate_ext_workspace {
        ($(@<$( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+>)? $ty: ty) => {
            const _: () = {
                use $crate::smithay_workspace::{WorkspaceGroupHandle, WorkspaceManagerGlobalData, WorkspaceManagerState, WorkspaceHandle};
                use smithay::reexports::wayland_protocols::ext::workspace::v1::server::{
                    ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
                    ext_workspace_handle_v1::ExtWorkspaceHandleV1,
                    ext_workspace_manager_v1::ExtWorkspaceManagerV1,
                };
                use smithay::reexports::wayland_server::{delegate_dispatch, delegate_global_dispatch};

                delegate_global_dispatch!(
                    $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                    $ty: [ExtWorkspaceManagerV1: WorkspaceManagerGlobalData] => WorkspaceManagerState
                );
                delegate_dispatch!(
                    $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                    $ty: [ExtWorkspaceManagerV1: ()] => WorkspaceManagerState
                );
                delegate_dispatch!(
                    $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                    $ty: [ExtWorkspaceGroupHandleV1: WorkspaceGroupHandle] => WorkspaceManagerState
                );
                delegate_dispatch!(
                    $(@< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)?
                    $ty: [ExtWorkspaceHandleV1: WorkspaceHandle] => WorkspaceManagerState
                );
            };
        };
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::os::unix::net::UnixStream;
        use std::sync::Arc;

        use smithay::delegate_compositor;
        use smithay::delegate_output;
        use smithay::reexports::wayland_server::Display;
        use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
        use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
        use smithay::reexports::wayland_server::{
            Client, DataInit, Dispatch, DisplayHandle, Resource,
        };
        use smithay::wayland::compositor::{
            CompositorClientState, CompositorHandler, CompositorState,
        };
        use smithay::wayland::output::{OutputHandler, OutputManagerState};
        use spiders_shared::ids::{OutputId, WindowId, WorkspaceId};
        use spiders_shared::wm::{
            LayoutRef, OutputSnapshot, OutputTransform, ShellKind, WindowSnapshot,
            WorkspaceSnapshot,
        };
        use wayland_client::protocol::{wl_output, wl_registry};
        use wayland_client::{Connection, EventQueue, QueueHandle, delegate_noop};
        use wayland_protocols::ext::workspace::v1::client::{
            ext_workspace_group_handle_v1, ext_workspace_handle_v1, ext_workspace_manager_v1,
        };

        #[derive(Debug, Default)]
        struct TestClientState {
            compositor_state: CompositorClientState,
        }

        impl ClientData for TestClientState {
            fn initialized(&self, _client_id: ClientId) {}

            fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
        }

        #[derive(Debug)]
        struct TestWorkspaceState {
            compositor_state: CompositorState,
            #[allow(dead_code)]
            output_manager_state: OutputManagerState,
            workspace_manager: WorkspaceManagerState,
            snapshot: StateSnapshot,
            display_handle: Option<DisplayHandle>,
        }

        #[derive(Debug, Default)]
        struct WorkspaceClientState {
            globals: Vec<(u32, String, u32)>,
            groups: Vec<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1>,
            workspaces: Vec<ext_workspace_handle_v1::ExtWorkspaceHandleV1>,
            group_count: usize,
            workspace_count: usize,
            done_count: usize,
            finished_count: usize,
            output_enter_count: usize,
            workspace_enter_count: usize,
            workspace_leave_count: usize,
            workspace_names: Vec<String>,
            workspace_ids: Vec<String>,
            workspace_states: Vec<u32>,
            workspace_capabilities: Vec<u32>,
            group_capabilities: Vec<u32>,
        }

        impl WorkspaceHandler for TestWorkspaceState {
            fn workspace_manager_state(&mut self) -> &mut WorkspaceManagerState {
                &mut self.workspace_manager
            }

            fn activate_workspace(&mut self, workspace_id: &WorkspaceId) {
                let target = self
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| &workspace.id == workspace_id)
                    .cloned();

                if let Some(workspace) = target {
                    for entry in &mut self.snapshot.workspaces {
                        if entry.output_id == workspace.output_id {
                            let selected = entry.id == workspace.id;
                            entry.visible = selected;
                            entry.focused = selected;
                        }
                    }
                    self.snapshot.current_workspace_id = Some(workspace.id.clone());
                    self.snapshot.current_output_id = workspace.output_id.clone();
                    if let Some(display_handle) = self.display_handle.as_ref() {
                        self.workspace_manager
                            .refresh_from_snapshot::<Self>(display_handle, &self.snapshot);
                    }
                }
            }

            fn assign_workspace(&mut self, workspace_id: &WorkspaceId, output_id: &OutputId) {
                if let Some(workspace) = self
                    .snapshot
                    .workspaces
                    .iter_mut()
                    .find(|workspace| &workspace.id == workspace_id)
                {
                    workspace.output_id = Some(output_id.clone());
                    if let Some(display_handle) = self.display_handle.as_ref() {
                        self.workspace_manager
                            .refresh_from_snapshot::<Self>(display_handle, &self.snapshot);
                    }
                }
            }
        }

        impl CompositorHandler for TestWorkspaceState {
            fn compositor_state(&mut self) -> &mut CompositorState {
                &mut self.compositor_state
            }

            fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
                &client
                    .get_data::<TestClientState>()
                    .unwrap()
                    .compositor_state
            }

            fn commit(
                &mut self,
                _surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
            ) {
            }
        }

        impl OutputHandler for TestWorkspaceState {
            fn output_bound(&mut self, output: smithay::output::Output, wl_output: WlOutput) {
                self.workspace_manager
                    .output_bound(&OutputId::from(output.name()), &wl_output);
            }
        }

        fn sample_state_two_outputs() -> StateSnapshot {
            StateSnapshot {
                focused_window_id: Some(WindowId::from("w1")),
                current_output_id: Some(OutputId::from("out-1")),
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
                outputs: vec![
                    OutputSnapshot {
                        id: OutputId::from("out-1"),
                        name: "HDMI-A-1".into(),
                        logical_x: 0,
                        logical_y: 0,
                        logical_width: 1920,
                        logical_height: 1080,
                        scale: 1,
                        transform: OutputTransform::Normal,
                        enabled: true,
                        current_workspace_id: Some(WorkspaceId::from("ws-1")),
                    },
                    OutputSnapshot {
                        id: OutputId::from("out-2"),
                        name: "DP-1".into(),
                        logical_x: 0,
                        logical_y: 0,
                        logical_width: 2560,
                        logical_height: 1440,
                        scale: 1,
                        transform: OutputTransform::Normal,
                        enabled: true,
                        current_workspace_id: Some(WorkspaceId::from("ws-2")),
                    },
                ],
                workspaces: vec![
                    WorkspaceSnapshot {
                        id: WorkspaceId::from("ws-1"),
                        name: "1".into(),
                        output_id: Some(OutputId::from("out-1")),
                        active_workspaces: vec!["1".into()],
                        focused: true,
                        visible: true,
                        effective_layout: Some(LayoutRef {
                            name: "master-stack".into(),
                        }),
                    },
                    WorkspaceSnapshot {
                        id: WorkspaceId::from("ws-2"),
                        name: "2".into(),
                        output_id: Some(OutputId::from("out-2")),
                        active_workspaces: vec!["2".into()],
                        focused: false,
                        visible: false,
                        effective_layout: Some(LayoutRef {
                            name: "master-stack".into(),
                        }),
                    },
                ],
                windows: vec![WindowSnapshot {
                    id: WindowId::from("w1"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("firefox".into()),
                    title: Some("Firefox".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    floating_rect: None,
                    fullscreen: false,
                    focused: true,
                    urgent: true,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-1")),
                    workspaces: vec!["1".into()],
                }],
                visible_window_ids: vec![WindowId::from("w1")],
                workspace_names: vec!["1".into(), "2".into()],
            }
        }

        impl Dispatch<WlOutput, (), TestWorkspaceState> for TestWorkspaceState {
            fn request(
                _state: &mut TestWorkspaceState,
                _client: &Client,
                _resource: &WlOutput,
                _request: <WlOutput as Resource>::Request,
                _data: &(),
                _dh: &DisplayHandle,
                _data_init: &mut DataInit<'_, TestWorkspaceState>,
            ) {
            }
        }

        delegate_compositor!(TestWorkspaceState);
        delegate_output!(TestWorkspaceState);

        impl wayland_client::Dispatch<wl_registry::WlRegistry, ()> for WorkspaceClientState {
            fn event(
                state: &mut Self,
                _proxy: &wl_registry::WlRegistry,
                event: wl_registry::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                if let wl_registry::Event::Global {
                    name,
                    interface,
                    version,
                } = event
                {
                    state.globals.push((name, interface, version));
                }
            }
        }

        impl wayland_client::Dispatch<wl_output::WlOutput, ()> for WorkspaceClientState {
            fn event(
                _state: &mut Self,
                _proxy: &wl_output::WlOutput,
                _event: wl_output::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
            }
        }

        impl wayland_client::Dispatch<ext_workspace_manager_v1::ExtWorkspaceManagerV1, ()>
            for WorkspaceClientState
        {
            fn event(
                state: &mut Self,
                _proxy: &ext_workspace_manager_v1::ExtWorkspaceManagerV1,
                event: ext_workspace_manager_v1::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                match event {
                    ext_workspace_manager_v1::Event::WorkspaceGroup { workspace_group } => {
                        state.group_count += 1;
                        state.groups.push(workspace_group);
                    }
                    ext_workspace_manager_v1::Event::Workspace { workspace } => {
                        state.workspace_count += 1;
                        state.workspaces.push(workspace);
                    }
                    ext_workspace_manager_v1::Event::Done => state.done_count += 1,
                    ext_workspace_manager_v1::Event::Finished => state.finished_count += 1,
                    _ => {}
                }
            }

            fn event_created_child(
                opcode: u16,
                qh: &QueueHandle<Self>,
            ) -> Arc<dyn wayland_client::backend::ObjectData> {
                match opcode {
                    0 => qh
                        .make_data::<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()>(
                            (),
                        ),
                    1 => qh.make_data::<ext_workspace_handle_v1::ExtWorkspaceHandleV1, ()>(()),
                    _ => panic!("unexpected manager child opcode {opcode}"),
                }
            }
        }

        impl wayland_client::Dispatch<ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1, ()>
            for WorkspaceClientState
        {
            fn event(
                state: &mut Self,
                _proxy: &ext_workspace_group_handle_v1::ExtWorkspaceGroupHandleV1,
                event: ext_workspace_group_handle_v1::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                match event {
                    ext_workspace_group_handle_v1::Event::Capabilities { capabilities } => {
                        state.group_capabilities.push(capabilities.into())
                    }
                    ext_workspace_group_handle_v1::Event::OutputEnter { .. } => {
                        state.output_enter_count += 1
                    }
                    ext_workspace_group_handle_v1::Event::WorkspaceEnter { .. } => {
                        state.workspace_enter_count += 1
                    }
                    ext_workspace_group_handle_v1::Event::WorkspaceLeave { .. } => {
                        state.workspace_leave_count += 1
                    }
                    _ => {}
                }
            }
        }

        impl wayland_client::Dispatch<ext_workspace_handle_v1::ExtWorkspaceHandleV1, ()>
            for WorkspaceClientState
        {
            fn event(
                state: &mut Self,
                _proxy: &ext_workspace_handle_v1::ExtWorkspaceHandleV1,
                event: ext_workspace_handle_v1::Event,
                _data: &(),
                _conn: &Connection,
                _qh: &QueueHandle<Self>,
            ) {
                match event {
                    ext_workspace_handle_v1::Event::Id { id } => state.workspace_ids.push(id),
                    ext_workspace_handle_v1::Event::Name { name } => {
                        state.workspace_names.push(name)
                    }
                    ext_workspace_handle_v1::Event::State { state: bits } => {
                        state.workspace_states.push(bits.into())
                    }
                    ext_workspace_handle_v1::Event::Capabilities { capabilities } => {
                        state.workspace_capabilities.push(capabilities.into())
                    }
                    _ => {}
                }
            }
        }

        delegate_noop!(WorkspaceClientState: ignore wayland_client::protocol::wl_callback::WlCallback);

        fn flush_roundtrip(
            conn: &Connection,
            display: &mut Display<TestWorkspaceState>,
            server_state: &mut TestWorkspaceState,
            queue: &mut EventQueue<WorkspaceClientState>,
            client_state: &mut WorkspaceClientState,
        ) {
            conn.flush().unwrap();
            display.dispatch_clients(server_state).unwrap();
            display.flush_clients().unwrap();

            if let Some(guard) = conn.prepare_read() {
                let _ = guard.read();
            } else {
                let _ = conn.backend().dispatch_inner_queue();
            }

            queue.dispatch_pending(client_state).unwrap();
        }

        crate::delegate_ext_workspace!(TestWorkspaceState);

        fn make_test_state(handle: &DisplayHandle, snapshot: StateSnapshot) -> TestWorkspaceState {
            TestWorkspaceState {
                compositor_state: CompositorState::new::<TestWorkspaceState>(handle),
                output_manager_state: OutputManagerState::new_with_xdg_output::<TestWorkspaceState>(
                    handle,
                ),
                workspace_manager: WorkspaceManagerState::new::<TestWorkspaceState>(handle),
                snapshot,
                display_handle: Some(handle.clone()),
            }
        }

        fn sample_state() -> StateSnapshot {
            StateSnapshot {
                focused_window_id: Some(WindowId::from("w1")),
                current_output_id: Some(OutputId::from("out-1")),
                current_workspace_id: Some(WorkspaceId::from("ws-1")),
                outputs: vec![OutputSnapshot {
                    id: OutputId::from("out-1"),
                    name: "HDMI-A-1".into(),
                    logical_x: 0,
                    logical_y: 0,
                    logical_width: 1920,
                    logical_height: 1080,
                    scale: 1,
                    transform: OutputTransform::Normal,
                    enabled: true,
                    current_workspace_id: Some(WorkspaceId::from("ws-1")),
                }],
                workspaces: vec![WorkspaceSnapshot {
                    id: WorkspaceId::from("ws-1"),
                    name: "1".into(),
                    output_id: Some(OutputId::from("out-1")),
                    active_workspaces: vec!["1".into()],
                    focused: true,
                    visible: true,
                    effective_layout: Some(LayoutRef {
                        name: "master-stack".into(),
                    }),
                }],
                windows: vec![WindowSnapshot {
                    id: WindowId::from("w1"),
                    shell: ShellKind::XdgToplevel,
                    app_id: Some("firefox".into()),
                    title: Some("Firefox".into()),
                    class: None,
                    instance: None,
                    role: None,
                    window_type: None,
                    mapped: true,
                    floating: false,
                    floating_rect: None,
                    fullscreen: false,
                    focused: true,
                    urgent: false,
                    output_id: Some(OutputId::from("out-1")),
                    workspace_id: Some(WorkspaceId::from("ws-1")),
                    workspaces: vec!["1".into()],
                }],
                visible_window_ids: vec![WindowId::from("w1")],
                workspace_names: vec!["1".into()],
            }
        }

        #[test]
        fn workspace_manager_refresh_tracks_groups_and_workspace_state() {
            let display = Display::<TestWorkspaceState>::new().unwrap();
            let mut manager = WorkspaceManagerState::new::<TestWorkspaceState>(&display.handle());
            manager.refresh_from_snapshot::<TestWorkspaceState>(&display.handle(), &sample_state());

            let snapshot = manager.debug_snapshot();
            assert_eq!(snapshot.manager_count, 0);
            assert_eq!(snapshot.group_output_ids, vec![OutputId::from("out-1")]);
            assert_eq!(snapshot.group_instance_counts, vec![0]);
            assert_eq!(snapshot.workspace_names, vec!["1".to_string()]);
            assert_eq!(snapshot.workspace_instance_counts, vec![0]);
            assert_eq!(
                snapshot.workspace_group_output_ids,
                vec![Some(OutputId::from("out-1"))]
            );
            assert_eq!(
                snapshot.workspace_states,
                vec![WorkspaceState::Active.bits()]
            );
            assert_eq!(snapshot.output_binding_counts, vec![0]);
        }

        #[test]
        fn workspace_manager_tracks_client_bindings_and_output_association() {
            let display = Display::<TestWorkspaceState>::new().unwrap();
            let mut handle = display.handle();
            let mut manager = WorkspaceManagerState::new::<TestWorkspaceState>(&handle);
            manager.refresh_from_snapshot::<TestWorkspaceState>(&handle, &sample_state());

            let (client_stream, _server_stream) = UnixStream::pair().unwrap();
            let client = handle
                .insert_client(client_stream, Arc::new(TestClientState::default()))
                .unwrap();
            let _manager_resource =
                manager.bind_manager_for_client::<TestWorkspaceState>(&handle, &client);
            let wl_output = client
                .create_resource::<WlOutput, _, TestWorkspaceState>(&handle, 4, ())
                .unwrap();
            manager.output_bound(&OutputId::from("out-1"), &wl_output);

            let snapshot = manager.debug_snapshot();
            assert_eq!(snapshot.manager_count, 1);
            assert_eq!(snapshot.group_instance_counts, vec![1]);
            assert_eq!(snapshot.workspace_instance_counts, vec![1]);
            assert_eq!(snapshot.output_binding_counts, vec![1]);
        }

        #[test]
        fn workspace_handler_activate_and_assign_refresh_snapshot() {
            let display = Display::<TestWorkspaceState>::new().unwrap();
            let handle = display.handle();
            let mut state = make_test_state(&handle, sample_state_two_outputs());
            state
                .workspace_manager
                .refresh_from_snapshot::<TestWorkspaceState>(&handle, &state.snapshot);

            state.activate_workspace(&WorkspaceId::from("ws-2"));
            assert_eq!(
                state.workspace_manager.debug_snapshot().workspace_states,
                vec![
                    (WorkspaceState::Active | WorkspaceState::Urgent).bits(),
                    WorkspaceState::Active.bits(),
                ]
            );

            state.assign_workspace(&WorkspaceId::from("ws-1"), &OutputId::from("out-2"));
            assert_eq!(
                state
                    .workspace_manager
                    .debug_snapshot()
                    .workspace_group_output_ids,
                vec![Some(OutputId::from("out-2")), Some(OutputId::from("out-2"))]
            );
        }

        #[test]
        fn workspace_manager_refresh_emits_leave_and_removed_for_workspace_move_and_output_loss() {
            let display = Display::<TestWorkspaceState>::new().unwrap();
            let handle = display.handle();
            let mut manager = WorkspaceManagerState::new::<TestWorkspaceState>(&handle);
            manager
                .refresh_from_snapshot::<TestWorkspaceState>(&handle, &sample_state_two_outputs());

            let mut changed = sample_state_two_outputs();
            changed
                .outputs
                .retain(|output| output.id != OutputId::from("out-1"));
            changed
                .workspaces
                .retain(|workspace| workspace.id != WorkspaceId::from("ws-1"));
            changed.workspaces[0].output_id = Some(OutputId::from("out-2"));
            changed.workspaces[0].visible = true;

            manager.refresh_from_snapshot::<TestWorkspaceState>(&handle, &changed);

            let snapshot = manager.debug_snapshot();
            assert_eq!(snapshot.group_output_ids, vec![OutputId::from("out-2")]);
            assert_eq!(snapshot.workspace_names, vec!["2".to_string()]);
            assert_eq!(
                snapshot.workspace_group_output_ids,
                vec![Some(OutputId::from("out-2"))]
            );
            assert_eq!(
                snapshot.workspace_states,
                vec![WorkspaceState::Active.bits()]
            );
        }

        #[test]
        fn workspace_manager_refresh_moves_workspace_between_output_groups() {
            let display = Display::<TestWorkspaceState>::new().unwrap();
            let handle = display.handle();
            let mut manager = WorkspaceManagerState::new::<TestWorkspaceState>(&handle);
            manager
                .refresh_from_snapshot::<TestWorkspaceState>(&handle, &sample_state_two_outputs());

            let mut changed = sample_state_two_outputs();
            changed.workspaces[0].output_id = Some(OutputId::from("out-2"));
            changed.workspaces[0].visible = false;

            manager.refresh_from_snapshot::<TestWorkspaceState>(&handle, &changed);

            let snapshot = manager.debug_snapshot();
            assert_eq!(
                snapshot.workspace_group_output_ids,
                vec![Some(OutputId::from("out-2")), Some(OutputId::from("out-2"))]
            );
            assert_eq!(
                snapshot.workspace_states,
                vec![
                    (WorkspaceState::Hidden | WorkspaceState::Urgent).bits(),
                    WorkspaceState::Hidden.bits(),
                ]
            );
        }

        #[test]
        fn workspace_handle_advertises_activate_and_assign_capabilities() {
            assert_eq!(
                WorkspaceCapabilities::Activate | WorkspaceCapabilities::Assign,
                WorkspaceCapabilities::Activate | WorkspaceCapabilities::Assign
            );
            assert_eq!(WorkspaceCapabilities::empty().bits(), 0);
        }

        #[test]
        fn workspace_protocol_client_receives_initial_events_activate_assign_and_finished() {
            let mut display = Display::<TestWorkspaceState>::new().unwrap();
            let mut handle = display.handle();
            let mut server_state = make_test_state(&handle, sample_state_two_outputs());
            server_state
                .workspace_manager
                .refresh_from_snapshot::<TestWorkspaceState>(&handle, &server_state.snapshot);

            let smithay_output = smithay::output::Output::new(
                "out-1".into(),
                smithay::output::PhysicalProperties {
                    size: (1920, 1080).into(),
                    subpixel: smithay::output::Subpixel::Unknown,
                    make: "Spiders".into(),
                    model: "Test".into(),
                    serial_number: "1".into(),
                },
            );
            let mode = smithay::output::Mode {
                size: (1920, 1080).into(),
                refresh: 60_000,
            };
            smithay_output.change_current_state(
                Some(mode),
                Some(smithay::utils::Transform::Normal),
                Some(smithay::output::Scale::Integer(1)),
                Some((0, 0).into()),
            );
            smithay_output.set_preferred(mode);
            let _global = smithay_output.create_global::<TestWorkspaceState>(&handle);

            let (client_stream, server_stream) = UnixStream::pair().unwrap();
            client_stream.set_nonblocking(true).unwrap();
            server_stream.set_nonblocking(true).unwrap();
            handle
                .insert_client(server_stream, Arc::new(TestClientState::default()))
                .unwrap();

            let conn = Connection::from_socket(client_stream).unwrap();
            let mut queue = conn.new_event_queue::<WorkspaceClientState>();
            let qh = queue.handle();
            let registry = conn.display().get_registry(&qh, ());
            let mut client_state = WorkspaceClientState::default();

            flush_roundtrip(
                &conn,
                &mut display,
                &mut server_state,
                &mut queue,
                &mut client_state,
            );

            let manager_name = client_state
                .globals
                .iter()
                .find(|(_, interface, _)| interface == "ext_workspace_manager_v1")
                .map(|(name, _, _)| *name)
                .unwrap();
            let output_name = client_state
                .globals
                .iter()
                .find(|(_, interface, _)| interface == "wl_output")
                .map(|(name, _, _)| *name)
                .unwrap();

            let manager = registry.bind::<ext_workspace_manager_v1::ExtWorkspaceManagerV1, _, _>(
                manager_name,
                1,
                &qh,
                (),
            );
            let _output = registry.bind::<wl_output::WlOutput, _, _>(output_name, 4, &qh, ());

            flush_roundtrip(
                &conn,
                &mut display,
                &mut server_state,
                &mut queue,
                &mut client_state,
            );

            assert!(client_state.group_count >= 1);
            assert!(client_state.workspace_count >= 1);
            assert!(client_state.done_count >= 1);
            assert!(client_state.output_enter_count >= 1);
            assert!(client_state.workspace_enter_count >= 1);
            assert_eq!(client_state.workspace_leave_count, 0);
            assert!(client_state.workspace_names.iter().any(|name| name == "1"));
            assert!(client_state.workspace_ids.iter().any(|id| id == "ws-1"));
            assert!(
                client_state
                    .group_capabilities
                    .iter()
                    .all(|bits| *bits == GroupCapabilities::empty().bits())
            );
            assert!(client_state.workspace_capabilities.iter().all(|bits| {
                *bits == (WorkspaceCapabilities::Activate | WorkspaceCapabilities::Assign).bits()
            }));
            let initial_done_count = client_state.done_count;
            let initial_state_event_count = client_state.workspace_states.len();

            let workspace_index = client_state
                .workspace_ids
                .iter()
                .position(|id| id == "ws-2")
                .unwrap_or(0);
            let workspace = client_state.workspaces[workspace_index].clone();
            workspace.activate();
            flush_roundtrip(
                &conn,
                &mut display,
                &mut server_state,
                &mut queue,
                &mut client_state,
            );
            assert_eq!(
                server_state.snapshot.current_workspace_id,
                Some(WorkspaceId::from("ws-2"))
            );
            assert!(client_state.done_count > initial_done_count);
            assert!(client_state.workspace_states.len() > initial_state_event_count);
            assert!(
                client_state
                    .workspace_states
                    .iter()
                    .rev()
                    .take(2)
                    .any(|bits| *bits == WorkspaceState::Active.bits())
            );
            assert_eq!(client_state.workspace_leave_count, 0);

            let group = client_state.groups.first().cloned().unwrap();
            let assign_done_count = client_state.done_count;
            workspace.assign(&group);
            flush_roundtrip(
                &conn,
                &mut display,
                &mut server_state,
                &mut queue,
                &mut client_state,
            );
            assert!(
                server_state
                    .workspace_manager
                    .debug_snapshot()
                    .workspace_group_output_ids
                    .iter()
                    .any(|output_id| output_id.as_ref() == Some(&OutputId::from("out-1")))
            );
            assert!(client_state.done_count > assign_done_count);
            assert!(client_state.workspace_enter_count >= 2);

            manager.stop();
            flush_roundtrip(
                &conn,
                &mut display,
                &mut server_state,
                &mut queue,
                &mut client_state,
            );
            assert_eq!(client_state.finished_count, 1);
        }
    }
}

#[cfg(feature = "smithay-winit")]
pub use imp::{
    WorkspaceGroupHandle, WorkspaceHandle, WorkspaceHandler, WorkspaceManagerDebugSnapshot,
    WorkspaceManagerGlobalData, WorkspaceManagerState,
};
