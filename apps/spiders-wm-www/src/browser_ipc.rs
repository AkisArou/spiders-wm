use std::cell::RefCell;
use std::rc::Rc;

use leptos::prelude::*;
use spiders_core::command::WmCommand;
use spiders_core::event::WmEvent;
use spiders_core::query::{QueryRequest, QueryResponse};
use spiders_ipc_browser::{
    BrowserIpcClient, BrowserIpcEventSource, BrowserIpcServer, BrowserIpcSubscription,
};
use spiders_ipc_core::{DebugDumpKind, DebugRequest, DebugResponse, IpcHandler};
use spiders_wm_runtime::PreviewRenderAction;

use crate::session::PreviewSessionState;

thread_local! {
    static IPC_RUNTIME: RefCell<Option<BrowserIpcRuntime>> = const { RefCell::new(None) };
}

struct BrowserIpcRuntime {
    server: Rc<BrowserIpcServer<BrowserPreviewIpcHandler>>,
    system_client: Rc<BrowserIpcClient>,
}

pub fn initialize(session: RwSignal<PreviewSessionState>) {
    IPC_RUNTIME.with(|runtime| {
        if runtime.borrow().is_some() {
            return;
        }

        let server = Rc::new(BrowserIpcServer::new(BrowserPreviewIpcHandler {
            session,
            pending_events: Vec::new(),
        }));
        let system_client =
            Rc::new(server.connect().expect("browser IPC client should initialize"));
        runtime.replace(Some(BrowserIpcRuntime { server, system_client }));
    });
}

pub fn system_client() -> Rc<BrowserIpcClient> {
    IPC_RUNTIME.with(|runtime| {
        Rc::clone(
            &runtime
                .borrow()
                .as_ref()
                .expect("browser IPC runtime should be initialized")
                .system_client,
        )
    })
}

pub fn broadcast_event(event: WmEvent) {
    IPC_RUNTIME.with(|runtime| {
        let _ = runtime
            .borrow()
            .as_ref()
            .expect("browser IPC runtime should be initialized")
            .server
            .broadcast_event(event);
    });
}

pub fn subscribe_system_events(
    handler: impl FnMut(spiders_ipc_core::IpcResponse) + 'static,
) -> BrowserIpcSubscription {
    system_client().on_event(handler)
}

pub struct BrowserPreviewIpcHandler {
    session: RwSignal<PreviewSessionState>,
    pending_events: Vec<WmEvent>,
}

impl IpcHandler for BrowserPreviewIpcHandler {
    type Error = std::convert::Infallible;

    fn handle_query(&mut self, query: QueryRequest) -> Result<QueryResponse, Self::Error> {
        Ok(self.session.get_untracked().query_response(query))
    }

    fn handle_command(&mut self, command: WmCommand) -> Result<(), Self::Error> {
        let mut session = self.session.get_untracked();
        let command_for_event = command.clone();
        let render_action = session.apply_command(command);
        let snapshot = session.state_snapshot();
        let event = session.event_for_command(&command_for_event);
        self.session.set(session);

        if let Some(event) = event {
            self.pending_events.push(event);
        }

        if matches!(
            render_action,
            PreviewRenderAction::RefreshFromLoadedLayout
                | PreviewRenderAction::RefreshFromLoadedLayoutAndReevaluate
        ) {
            self.pending_events.push(WmEvent::FocusChange {
                focused_window_id: snapshot.focused_window_id,
                current_output_id: snapshot.current_output_id,
                current_workspace_id: snapshot.current_workspace_id,
            });
        }

        Ok(())
    }

    fn handle_debug(&mut self, request: DebugRequest) -> Result<DebugResponse, Self::Error> {
        let DebugRequest::Dump { kind } = request;
        Ok(DebugResponse::DumpWritten {
            kind,
            path: Some(
                match kind {
                    DebugDumpKind::WmState => "browser://preview/state.json",
                    DebugDumpKind::DebugProfile => "browser://preview/profile.json",
                    DebugDumpKind::SceneSnapshot => "browser://preview/scene.json",
                    DebugDumpKind::FrameSync => "browser://preview/frame-sync.json",
                    DebugDumpKind::Seats => "browser://preview/seats.json",
                }
                .to_string(),
            ),
        })
    }
}

impl BrowserIpcEventSource for BrowserPreviewIpcHandler {
    fn drain_events(&mut self) -> Vec<WmEvent> {
        std::mem::take(&mut self.pending_events)
    }
}
