use spiders_shared::api::{CompositorEvent, QueryRequest, QueryResponse, WmAction};

use crate::protocol::{
    normalize_topics, subscription_matches_event, IpcClientMessage, IpcEnvelope, IpcRequest,
    IpcResponse, IpcServerMessage, IpcSubscriptionTopic,
};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IpcSession {
    subscription_topics: Vec<IpcSubscriptionTopic>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IpcSessionHandleResult {
    Query {
        request_id: Option<String>,
        query: QueryRequest,
    },
    Action {
        request_id: Option<String>,
        action: WmAction,
    },
    Response(IpcResponse),
}

impl IpcSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscription_topics(&self) -> &[IpcSubscriptionTopic] {
        &self.subscription_topics
    }

    pub fn handle_request(&mut self, request: IpcRequest) -> IpcSessionHandleResult {
        match request.message {
            IpcClientMessage::Query(query) => IpcSessionHandleResult::Query {
                request_id: request.request_id,
                query,
            },
            IpcClientMessage::Action(action) => IpcSessionHandleResult::Action {
                request_id: request.request_id,
                action,
            },
            IpcClientMessage::Subscribe { topics } => {
                self.subscription_topics =
                    normalize_topics(self.subscription_topics.iter().copied().chain(topics));

                IpcSessionHandleResult::Response(IpcEnvelope {
                    request_id: request.request_id,
                    message: IpcServerMessage::subscribed(self.subscription_topics.iter().copied()),
                })
            }
            IpcClientMessage::Unsubscribe { topics } => {
                self.subscription_topics = unsubscribe_topics(&self.subscription_topics, &topics);

                IpcSessionHandleResult::Response(IpcEnvelope {
                    request_id: request.request_id,
                    message: IpcServerMessage::unsubscribed(
                        self.subscription_topics.iter().copied(),
                    ),
                })
            }
        }
    }

    pub fn query_response(
        &self,
        request_id: Option<String>,
        response: QueryResponse,
    ) -> IpcResponse {
        IpcEnvelope {
            request_id,
            message: IpcServerMessage::Query(response),
        }
    }

    pub fn action_accepted(&self, request_id: Option<String>) -> IpcResponse {
        IpcEnvelope {
            request_id,
            message: IpcServerMessage::ActionAccepted,
        }
    }

    pub fn error_response(
        &self,
        request_id: Option<String>,
        message: impl Into<String>,
    ) -> IpcResponse {
        IpcEnvelope {
            request_id,
            message: IpcServerMessage::error(message),
        }
    }

    pub fn event_response(&self, event: CompositorEvent) -> Option<IpcResponse> {
        if subscription_matches_event(&self.subscription_topics, &event) {
            Some(IpcEnvelope::new(IpcServerMessage::event(event)))
        } else {
            None
        }
    }
}

fn unsubscribe_topics(
    current_topics: &[IpcSubscriptionTopic],
    removed_topics: &[IpcSubscriptionTopic],
) -> Vec<IpcSubscriptionTopic> {
    let current = normalize_topics(current_topics.iter().copied());
    let removed = normalize_topics(removed_topics.iter().copied());

    if removed.is_empty() {
        return current;
    }

    if removed == [IpcSubscriptionTopic::All] {
        return Vec::new();
    }

    if current == [IpcSubscriptionTopic::All] {
        return current;
    }

    current
        .into_iter()
        .filter(|topic| !removed.contains(topic))
        .collect()
}

#[cfg(test)]
mod tests {
    use spiders_shared::api::QueryResponse;

    use super::*;

    #[test]
    fn session_forwards_query_requests_with_request_id() {
        let mut session = IpcSession::new();

        let result = session.handle_request(
            IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::State)).with_request_id("req-1"),
        );

        assert_eq!(
            result,
            IpcSessionHandleResult::Query {
                request_id: Some("req-1".into()),
                query: QueryRequest::State,
            }
        );
    }

    #[test]
    fn session_forwards_action_requests_with_request_id() {
        let mut session = IpcSession::new();

        let result = session.handle_request(
            IpcEnvelope::new(IpcClientMessage::Action(WmAction::ReloadConfig))
                .with_request_id("req-2"),
        );

        assert_eq!(
            result,
            IpcSessionHandleResult::Action {
                request_id: Some("req-2".into()),
                action: WmAction::ReloadConfig,
            }
        );
    }

    #[test]
    fn session_subscribe_updates_topics_and_returns_effective_state() {
        let mut session = IpcSession::new();

        let result = session.handle_request(IpcEnvelope::new(IpcClientMessage::subscribe([
            IpcSubscriptionTopic::Focus,
            IpcSubscriptionTopic::Focus,
            IpcSubscriptionTopic::Windows,
        ])));

        assert_eq!(
            result,
            IpcSessionHandleResult::Response(IpcEnvelope::new(IpcServerMessage::Subscribed {
                topics: vec![IpcSubscriptionTopic::Focus, IpcSubscriptionTopic::Windows],
            }))
        );
        assert_eq!(
            session.subscription_topics(),
            &[IpcSubscriptionTopic::Focus, IpcSubscriptionTopic::Windows]
        );
    }

    #[test]
    fn session_unsubscribe_removes_specific_topics() {
        let mut session = IpcSession::new();
        session.handle_request(IpcEnvelope::new(IpcClientMessage::subscribe([
            IpcSubscriptionTopic::Focus,
            IpcSubscriptionTopic::Layout,
        ])));

        let result = session.handle_request(IpcEnvelope::new(IpcClientMessage::unsubscribe([
            IpcSubscriptionTopic::Focus,
        ])));

        assert_eq!(
            result,
            IpcSessionHandleResult::Response(IpcEnvelope::new(IpcServerMessage::Unsubscribed {
                topics: vec![IpcSubscriptionTopic::Layout],
            }))
        );
        assert_eq!(
            session.subscription_topics(),
            &[IpcSubscriptionTopic::Layout]
        );
    }

    #[test]
    fn session_unsubscribe_all_clears_topics() {
        let mut session = IpcSession::new();
        session.handle_request(IpcEnvelope::new(IpcClientMessage::subscribe_all()));

        let result = session.handle_request(IpcEnvelope::new(IpcClientMessage::unsubscribe([
            IpcSubscriptionTopic::All,
        ])));

        assert_eq!(
            result,
            IpcSessionHandleResult::Response(IpcEnvelope::new(IpcServerMessage::Unsubscribed {
                topics: Vec::new(),
            }))
        );
        assert!(session.subscription_topics().is_empty());
    }

    #[test]
    fn session_unsubscribe_specific_topic_preserves_all_subscription() {
        let mut session = IpcSession::new();
        session.handle_request(IpcEnvelope::new(IpcClientMessage::subscribe_all()));

        let result = session.handle_request(IpcEnvelope::new(IpcClientMessage::unsubscribe([
            IpcSubscriptionTopic::Focus,
        ])));

        assert_eq!(
            result,
            IpcSessionHandleResult::Response(IpcEnvelope::new(IpcServerMessage::Unsubscribed {
                topics: vec![IpcSubscriptionTopic::All],
            }))
        );
        assert_eq!(session.subscription_topics(), &[IpcSubscriptionTopic::All]);
    }

    #[test]
    fn session_event_response_only_emits_matching_events() {
        let mut session = IpcSession::new();
        session.handle_request(IpcEnvelope::new(IpcClientMessage::subscribe([
            IpcSubscriptionTopic::Layout,
        ])));

        let matching = session.event_response(CompositorEvent::LayoutChange {
            workspace_id: None,
            layout: None,
        });
        let non_matching = session.event_response(CompositorEvent::ConfigReloaded);

        assert!(matches!(
            matching,
            Some(IpcEnvelope {
                message: IpcServerMessage::Event { .. },
                ..
            })
        ));
        assert!(non_matching.is_none());
    }

    #[test]
    fn session_builds_query_action_and_error_responses() {
        let session = IpcSession::new();

        assert_eq!(
            session.query_response(
                Some("req-3".into()),
                QueryResponse::TagNames(vec!["1".into()])
            ),
            IpcEnvelope::new(IpcServerMessage::Query(QueryResponse::TagNames(vec![
                "1".into()
            ])))
            .with_request_id("req-3")
        );
        assert_eq!(
            session.action_accepted(Some("req-4".into())),
            IpcEnvelope::new(IpcServerMessage::ActionAccepted).with_request_id("req-4")
        );
        assert_eq!(
            session.error_response(Some("req-5".into()), "nope"),
            IpcEnvelope::new(IpcServerMessage::error("nope")).with_request_id("req-5")
        );
    }
}
