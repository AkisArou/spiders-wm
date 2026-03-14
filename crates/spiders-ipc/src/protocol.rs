use serde::{Deserialize, Serialize};

use spiders_shared::api::{CompositorEvent, QueryRequest, QueryResponse, WmAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IpcSubscriptionTopic {
    All,
    Focus,
    Windows,
    Tags,
    Layout,
    Config,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IpcEnvelope<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub message: T,
}

impl<T> IpcEnvelope<T> {
    pub fn new(message: T) -> Self {
        Self {
            request_id: None,
            message,
        }
    }

    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = Some(request_id.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum IpcClientMessage {
    Query(QueryRequest),
    Action(WmAction),
    Subscribe {
        #[serde(default)]
        topics: Vec<IpcSubscriptionTopic>,
    },
    Unsubscribe {
        #[serde(default)]
        topics: Vec<IpcSubscriptionTopic>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "kebab-case")]
pub enum IpcServerMessage {
    Query(QueryResponse),
    Event {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        topics: Vec<IpcSubscriptionTopic>,
        event: CompositorEvent,
    },
    ActionAccepted,
    Subscribed {
        #[serde(default)]
        topics: Vec<IpcSubscriptionTopic>,
    },
    Unsubscribed {
        #[serde(default)]
        topics: Vec<IpcSubscriptionTopic>,
    },
    Error {
        message: String,
    },
}

pub type IpcRequest = IpcEnvelope<IpcClientMessage>;
pub type IpcResponse = IpcEnvelope<IpcServerMessage>;

impl IpcClientMessage {
    pub fn subscribe(topics: impl IntoIterator<Item = IpcSubscriptionTopic>) -> Self {
        Self::Subscribe {
            topics: normalize_topics(topics),
        }
    }

    pub fn unsubscribe(topics: impl IntoIterator<Item = IpcSubscriptionTopic>) -> Self {
        Self::Unsubscribe {
            topics: normalize_topics(topics),
        }
    }

    pub fn subscribe_all() -> Self {
        Self::subscribe([IpcSubscriptionTopic::All])
    }
}

impl IpcServerMessage {
    pub fn event(event: CompositorEvent) -> Self {
        Self::Event {
            topics: infer_topics(&event),
            event,
        }
    }

    pub fn subscribed(topics: impl IntoIterator<Item = IpcSubscriptionTopic>) -> Self {
        Self::Subscribed {
            topics: normalize_topics(topics),
        }
    }

    pub fn unsubscribed(topics: impl IntoIterator<Item = IpcSubscriptionTopic>) -> Self {
        Self::Unsubscribed {
            topics: normalize_topics(topics),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }
}

pub fn infer_topics(event: &CompositorEvent) -> Vec<IpcSubscriptionTopic> {
    match event {
        CompositorEvent::FocusChange { .. } => vec![IpcSubscriptionTopic::Focus],
        CompositorEvent::WindowCreated { .. }
        | CompositorEvent::WindowDestroyed { .. }
        | CompositorEvent::WindowTagChange { .. }
        | CompositorEvent::WindowFloatingChange { .. }
        | CompositorEvent::WindowGeometryChange { .. }
        | CompositorEvent::WindowFullscreenChange { .. } => vec![IpcSubscriptionTopic::Windows],
        CompositorEvent::TagChange { .. } => vec![IpcSubscriptionTopic::Tags],
        CompositorEvent::LayoutChange { .. } => vec![IpcSubscriptionTopic::Layout],
        CompositorEvent::ConfigReloaded => vec![IpcSubscriptionTopic::Config],
    }
}

pub fn normalize_topics(
    topics: impl IntoIterator<Item = IpcSubscriptionTopic>,
) -> Vec<IpcSubscriptionTopic> {
    let mut normalized = Vec::new();

    for topic in topics {
        if topic == IpcSubscriptionTopic::All {
            return vec![IpcSubscriptionTopic::All];
        }

        if !normalized.contains(&topic) {
            normalized.push(topic);
        }
    }

    normalized
}

pub fn subscription_matches_topics(
    subscription_topics: &[IpcSubscriptionTopic],
    event_topics: &[IpcSubscriptionTopic],
) -> bool {
    let normalized = normalize_topics(subscription_topics.iter().copied());

    if normalized.is_empty() {
        return false;
    }

    if normalized == [IpcSubscriptionTopic::All] {
        return true;
    }

    event_topics
        .iter()
        .any(|topic| normalized.iter().any(|candidate| candidate == topic))
}

pub fn subscription_matches_event(
    subscription_topics: &[IpcSubscriptionTopic],
    event: &CompositorEvent,
) -> bool {
    subscription_matches_topics(subscription_topics, &infer_topics(event))
}

#[cfg(test)]
mod tests {
    use spiders_shared::ids::{OutputId, WindowId};
    use spiders_shared::wm::OutputSnapshot;

    use super::*;

    #[test]
    fn client_request_round_trips_with_request_id() {
        let request =
            IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::State)).with_request_id("req-1");

        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, request);
    }

    #[test]
    fn subscribe_all_helper_uses_all_topic() {
        assert_eq!(
            IpcClientMessage::subscribe_all(),
            IpcClientMessage::Subscribe {
                topics: vec![IpcSubscriptionTopic::All],
            }
        );
    }

    #[test]
    fn subscribe_helper_normalizes_duplicates_and_all() {
        assert_eq!(
            IpcClientMessage::subscribe([
                IpcSubscriptionTopic::Focus,
                IpcSubscriptionTopic::Focus,
                IpcSubscriptionTopic::All,
                IpcSubscriptionTopic::Layout,
            ]),
            IpcClientMessage::Subscribe {
                topics: vec![IpcSubscriptionTopic::All],
            }
        );
    }

    #[test]
    fn event_response_infers_topics_from_compositor_event() {
        let message = IpcServerMessage::event(CompositorEvent::FocusChange {
            focused_window_id: Some(WindowId::from("w1")),
            current_output_id: Some(OutputId::from("out-1")),
            current_workspace_id: None,
        });

        assert!(matches!(
            message,
            IpcServerMessage::Event { topics, .. }
                if topics == vec![IpcSubscriptionTopic::Focus]
        ));
    }

    #[test]
    fn server_event_round_trips_with_topics() {
        let response = IpcEnvelope::new(IpcServerMessage::event(CompositorEvent::WindowCreated {
            window: spiders_shared::wm::WindowSnapshot {
                id: WindowId::from("w1"),
                shell: spiders_shared::wm::ShellKind::XdgToplevel,
                title: Some("Terminal".into()),
                app_id: Some("foot".into()),
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
                workspace_id: None,
                output_id: Some(OutputId::from("out-1")),
                tags: vec!["1".into()],
            },
        }))
        .with_request_id("sub-1");

        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, response);
    }

    #[test]
    fn subscribed_helper_normalizes_topics() {
        assert_eq!(
            IpcServerMessage::subscribed([
                IpcSubscriptionTopic::Focus,
                IpcSubscriptionTopic::Focus,
                IpcSubscriptionTopic::All,
            ]),
            IpcServerMessage::Subscribed {
                topics: vec![IpcSubscriptionTopic::All],
            }
        );
    }

    #[test]
    fn unsubscribe_round_trips_empty_topic_list() {
        let request = IpcEnvelope::new(IpcClientMessage::Unsubscribe { topics: Vec::new() });

        let json = serde_json::to_string(&request).unwrap();
        let parsed: IpcRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, request);
    }

    #[test]
    fn subscription_matches_event_topics() {
        let event = CompositorEvent::LayoutChange {
            workspace_id: None,
            layout: None,
        };

        assert!(subscription_matches_event(
            &[IpcSubscriptionTopic::Layout],
            &event,
        ));
        assert!(subscription_matches_event(
            &[IpcSubscriptionTopic::All],
            &event
        ));
        assert!(!subscription_matches_event(
            &[IpcSubscriptionTopic::Windows],
            &event,
        ));
        assert!(!subscription_matches_event(&[], &event));
    }

    #[test]
    fn query_response_round_trips_in_server_envelope() {
        let response = IpcEnvelope::new(IpcServerMessage::Query(QueryResponse::CurrentOutput(
            Some(OutputSnapshot {
                id: OutputId::from("out-1"),
                name: "HDMI-A-1".into(),
                logical_width: 1920,
                logical_height: 1080,
                scale: 1,
                transform: spiders_shared::wm::OutputTransform::Normal,
                enabled: true,
                current_workspace_id: None,
            }),
        )));

        let json = serde_json::to_string(&response).unwrap();
        let parsed: IpcResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, response);
    }

    #[test]
    fn error_helper_builds_error_message() {
        assert_eq!(
            IpcServerMessage::error("bad request"),
            IpcServerMessage::Error {
                message: "bad request".into(),
            }
        );
    }
}
