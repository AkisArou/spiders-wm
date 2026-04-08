use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use serde_wasm_bindgen::{from_value, to_value};
use spiders_core::event::WmEvent;
use spiders_ipc_core::{
    IpcClientId, IpcHandler, IpcRequest, IpcResponse, IpcServerState, ResolveIpcRequestError,
    UnknownClientError, resolve_ipc_request,
};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::{MessageChannel, MessageEvent, MessagePort};

pub trait BrowserIpcEventSource {
    fn drain_events(&mut self) -> Vec<WmEvent>;
}

#[derive(Debug)]
pub enum BrowserIpcError<E> {
    Js(JsValue),
    Protocol(String),
    UnknownClient(UnknownClientError),
    Handler(E),
    Closed,
}

impl<E> From<JsValue> for BrowserIpcError<E> {
    fn from(value: JsValue) -> Self {
        Self::Js(value)
    }
}

impl<E> From<serde_wasm_bindgen::Error> for BrowserIpcError<E> {
    fn from(value: serde_wasm_bindgen::Error) -> Self {
        Self::Protocol(value.to_string())
    }
}

impl<E> From<UnknownClientError> for BrowserIpcError<E> {
    fn from(value: UnknownClientError) -> Self {
        Self::UnknownClient(value)
    }
}

pub struct BrowserIpcServer<H> {
    inner: Rc<RefCell<BrowserIpcServerInner<H>>>,
}

pub struct BrowserIpcClient {
    inner: Rc<BrowserIpcClientInner>,
}

pub struct BrowserIpcSubscription {
    inner: Rc<BrowserIpcClientInner>,
    subscription_id: u64,
}

struct BrowserIpcServerInner<H> {
    server: IpcServerState,
    handler: H,
    clients: BTreeMap<IpcClientId, BrowserServerClient>,
}

struct BrowserServerClient {
    port: MessagePort,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
}

struct BrowserIpcClientInner {
    port: MessagePort,
    next_request_id: Cell<u64>,
    next_subscription_id: Cell<u64>,
    pending: Rc<RefCell<BTreeMap<String, Rc<RefCell<PendingResponse>>>>>,
    event_handlers: Rc<RefCell<BTreeMap<u64, Box<dyn FnMut(IpcResponse)>>>>,
    closed: Cell<bool>,
    _on_message: Closure<dyn FnMut(MessageEvent)>,
}

struct PendingResponse {
    response: Option<Result<IpcResponse, BrowserIpcError<JsValue>>>,
    waker: Option<Waker>,
}

pub struct BrowserIpcRequestFuture {
    pending: Rc<RefCell<PendingResponse>>,
}

impl<H> BrowserIpcServer<H>
where
    H: IpcHandler + BrowserIpcEventSource + 'static,
{
    pub fn new(handler: H) -> Self {
        Self {
            inner: Rc::new(RefCell::new(BrowserIpcServerInner {
                server: IpcServerState::new(),
                handler,
                clients: BTreeMap::new(),
            })),
        }
    }

    pub fn connect(&self) -> Result<BrowserIpcClient, BrowserIpcError<H::Error>> {
        let channel = MessageChannel::new()?;
        let server_port = channel.port1();
        let client_port = channel.port2();

        let client_id = {
            let mut inner = self.inner.borrow_mut();
            inner.server.add_client()
        };

        let inner = Rc::clone(&self.inner);
        let server_port_for_handler = server_port.clone();
        let on_message = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event: MessageEvent| {
            if let Ok(response) = handle_server_message(&inner, client_id, event.data())
                && let Ok(payload) = to_value(&response)
            {
                let _ = server_port_for_handler.post_message(&payload);
            }
        }));

        server_port.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        server_port.start();
        client_port.start();

        self.inner.borrow_mut().clients.insert(
            client_id,
            BrowserServerClient {
                port: server_port,
                _on_message: on_message,
            },
        );

        Ok(BrowserIpcClient::new(client_port))
    }

    pub fn broadcast_event(&self, event: WmEvent) -> Result<(), BrowserIpcError<H::Error>> {
        let responses = {
            let inner = self.inner.borrow();
            inner.server.broadcast_event(event)
        };

        let inner = self.inner.borrow();
        for (client_id, response) in responses {
            let Some(client) = inner.clients.get(&client_id) else {
                continue;
            };
            let payload = to_value(&response).map_err(BrowserIpcError::from)?;
            client.port.post_message(&payload)?;
        }

        Ok(())
    }

    pub fn with_handler<T>(&self, update: impl FnOnce(&mut H) -> T) -> T {
        let mut inner = self.inner.borrow_mut();
        update(&mut inner.handler)
    }

    pub fn client_count(&self) -> usize {
        self.inner.borrow().server.client_count()
    }

    pub fn remove_client(
        &self,
        client_id: IpcClientId,
    ) -> Result<(), BrowserIpcError<H::Error>> {
        let mut inner = self.inner.borrow_mut();
        if let Some(client) = inner.clients.remove(&client_id) {
            client.port.close();
            drop(client);
        }
        inner.server.remove_client(client_id).ok_or(UnknownClientError { client_id })?;
        Ok(())
    }
}

impl BrowserIpcClient {
    fn new(port: MessagePort) -> Self {
        let pending = Rc::new(RefCell::new(BTreeMap::<String, Rc<RefCell<PendingResponse>>>::new()));
        let event_handlers = Rc::new(RefCell::new(BTreeMap::<u64, Box<dyn FnMut(IpcResponse)>>::new()));

        let pending_for_handler = Rc::clone(&pending);
        let handlers_for_handler = Rc::clone(&event_handlers);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::wrap(Box::new(move |event: MessageEvent| {
            let response = from_value::<IpcResponse>(event.data())
                .map_err(BrowserIpcError::<JsValue>::from);

            match response {
                Ok(response) => {
                    if let Some(request_id) = response.request_id.clone()
                        && let Some(pending) = pending_for_handler.borrow_mut().remove(&request_id)
                    {
                        let mut pending = pending.borrow_mut();
                        pending.response = Some(Ok(response));
                        if let Some(waker) = pending.waker.take() {
                            waker.wake();
                        }
                        return;
                    }

                    for handler in handlers_for_handler.borrow_mut().values_mut() {
                        handler(response.clone());
                    }
                }
                Err(error) => {
                    let pending_values: Vec<_> =
                        pending_for_handler.borrow_mut().values().cloned().collect();
                    pending_for_handler.borrow_mut().clear();
                    for pending in pending_values {
                        let mut pending = pending.borrow_mut();
                        pending.response = Some(Err(match &error {
                            BrowserIpcError::Js(value) => BrowserIpcError::Js(value.clone()),
                            BrowserIpcError::Protocol(message) => {
                                BrowserIpcError::Protocol(message.clone())
                            }
                            BrowserIpcError::Closed => BrowserIpcError::Closed,
                            BrowserIpcError::UnknownClient(err) => {
                                BrowserIpcError::UnknownClient(err.clone())
                            }
                            BrowserIpcError::Handler(_) => BrowserIpcError::Protocol(
                                "handler errors do not cross the browser client boundary"
                                    .to_string(),
                            ),
                        }));
                        if let Some(waker) = pending.waker.take() {
                            waker.wake();
                        }
                    }
                }
            }
        }));

        port.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        port.start();

        Self {
            inner: Rc::new(BrowserIpcClientInner {
                port,
                next_request_id: Cell::new(1),
                next_subscription_id: Cell::new(1),
                pending,
                event_handlers,
                closed: Cell::new(false),
                _on_message: on_message,
            }),
        }
    }

    pub fn request(
        &self,
        request: IpcRequest,
    ) -> Result<BrowserIpcRequestFuture, BrowserIpcError<JsValue>> {
        if self.inner.closed.get() {
            return Err(BrowserIpcError::Closed);
        }

        let request_id = request.request_id.clone().unwrap_or_else(|| {
            let request_id = format!("browser-ipc-{}", self.inner.next_request_id.get());
            self.inner.next_request_id.set(self.inner.next_request_id.get() + 1);
            request_id
        });

        let pending = Rc::new(RefCell::new(PendingResponse { response: None, waker: None }));
        self.inner.pending.borrow_mut().insert(request_id.clone(), Rc::clone(&pending));

        let mut request = request;
        request.request_id = Some(request_id);
        let payload = to_value(&request).map_err(BrowserIpcError::from)?;
        self.inner.port.post_message(&payload)?;

        Ok(BrowserIpcRequestFuture { pending })
    }

    pub fn on_event(
        &self,
        handler: impl FnMut(IpcResponse) + 'static,
    ) -> BrowserIpcSubscription {
        let subscription_id = self.inner.next_subscription_id.get();
        self.inner.next_subscription_id.set(subscription_id + 1);
        self.inner.event_handlers.borrow_mut().insert(subscription_id, Box::new(handler));
        BrowserIpcSubscription { inner: Rc::clone(&self.inner), subscription_id }
    }

    pub fn close(&self) {
        if self.inner.closed.replace(true) {
            return;
        }

        self.inner.port.close();
        let pending_values: Vec<_> = self.inner.pending.borrow_mut().values().cloned().collect();
        self.inner.pending.borrow_mut().clear();
        for pending in pending_values {
            let mut pending = pending.borrow_mut();
            pending.response = Some(Err(BrowserIpcError::Closed));
            if let Some(waker) = pending.waker.take() {
                waker.wake();
            }
        }
    }
}

impl Clone for BrowserIpcClient {
    fn clone(&self) -> Self {
        Self { inner: Rc::clone(&self.inner) }
    }
}

impl Drop for BrowserIpcSubscription {
    fn drop(&mut self) {
        self.inner.event_handlers.borrow_mut().remove(&self.subscription_id);
    }
}

impl Future for BrowserIpcRequestFuture {
    type Output = Result<IpcResponse, BrowserIpcError<JsValue>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut pending = self.pending.borrow_mut();
        if let Some(response) = pending.response.take() {
            Poll::Ready(response)
        } else {
            pending.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

fn handle_server_message<H>(
    inner: &Rc<RefCell<BrowserIpcServerInner<H>>>,
    client_id: IpcClientId,
    payload: JsValue,
) -> Result<IpcResponse, BrowserIpcError<H::Error>>
where
    H: IpcHandler + BrowserIpcEventSource,
{
    let request = from_value::<IpcRequest>(payload)
        .map_err(|error| BrowserIpcError::Protocol(error.to_string()))?;

    let mut inner = inner.borrow_mut();
    let BrowserIpcServerInner { server, handler, clients } = &mut *inner;

    match resolve_ipc_request(server, client_id, request.clone(), handler) {
        Ok(response) => {
            let events = handler.drain_events();
            for event in events {
                for (target_client_id, event_response) in server.broadcast_event(event) {
                    let Some(client) = clients.get(&target_client_id) else {
                        continue;
                    };
                    let payload = to_value(&event_response).map_err(BrowserIpcError::from)?;
                    client.port.post_message(&payload)?;
                }
            }
            Ok(response)
        }
        Err(ResolveIpcRequestError::UnknownClient(error)) => Err(BrowserIpcError::UnknownClient(error)),
        Err(ResolveIpcRequestError::Handler(error)) => {
            let response =
                server.error_response(client_id, request.request_id, "browser ipc handler error")?;
            let _ = error;
            Ok(response)
        }
    }
}
