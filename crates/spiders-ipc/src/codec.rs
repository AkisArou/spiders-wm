use serde::{Serialize, de::DeserializeOwned};

use crate::{IpcRequest, IpcResponse};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IpcCodecError {
    #[error("empty IPC frame")]
    EmptyFrame,
    #[error("invalid IPC json: {0}")]
    InvalidJson(String),
}

pub fn encode_request_line(request: &IpcRequest) -> Result<String, IpcCodecError> {
    encode_json_line(request)
}

pub fn decode_request_line(line: &str) -> Result<IpcRequest, IpcCodecError> {
    decode_json_line(line)
}

pub fn encode_response_line(response: &IpcResponse) -> Result<String, IpcCodecError> {
    encode_json_line(response)
}

pub fn decode_response_line(line: &str) -> Result<IpcResponse, IpcCodecError> {
    decode_json_line(line)
}

fn encode_json_line<T>(value: &T) -> Result<String, IpcCodecError>
where
    T: Serialize,
{
    let mut line = serde_json::to_string(value)
        .map_err(|error| IpcCodecError::InvalidJson(error.to_string()))?;
    line.push('\n');
    Ok(line)
}

fn decode_json_line<T>(line: &str) -> Result<T, IpcCodecError>
where
    T: DeserializeOwned,
{
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return Err(IpcCodecError::EmptyFrame);
    }

    serde_json::from_str(trimmed).map_err(|error| IpcCodecError::InvalidJson(error.to_string()))
}

#[cfg(test)]
mod tests {
    use spiders_shared::api::{CompositorEvent, QueryRequest};
    use spiders_tree::WindowId;

    use crate::{IpcClientMessage, IpcEnvelope, IpcServerMessage, IpcSubscriptionTopic};

    use super::*;

    #[test]
    fn request_line_round_trips() {
        let request = IpcEnvelope::new(IpcClientMessage::subscribe([
            IpcSubscriptionTopic::Focus,
            IpcSubscriptionTopic::Layout,
        ]))
        .with_request_id("req-1");

        let line = encode_request_line(&request).unwrap();
        let decoded = decode_request_line(&line).unwrap();

        assert!(line.ends_with('\n'));
        assert_eq!(decoded, request);
    }

    #[test]
    fn response_line_round_trips() {
        let response =
            IpcEnvelope::new(IpcServerMessage::event(CompositorEvent::WindowDestroyed {
                window_id: WindowId::from("w1"),
            }))
            .with_request_id("sub-1");

        let line = encode_response_line(&response).unwrap();
        let decoded = decode_response_line(&line).unwrap();

        assert!(line.ends_with('\n'));
        assert_eq!(decoded, response);
    }

    #[test]
    fn decode_request_rejects_empty_frames() {
        let error = decode_request_line("   \n\t").unwrap_err();

        assert_eq!(error, IpcCodecError::EmptyFrame);
    }

    #[test]
    fn decode_response_rejects_invalid_json() {
        let error = decode_response_line("{not-json}\n").unwrap_err();

        assert!(matches!(error, IpcCodecError::InvalidJson(_)));
    }

    #[test]
    fn decode_request_tolerates_missing_trailing_newline() {
        let request = IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::State));
        let line = serde_json::to_string(&request).unwrap();

        let decoded = decode_request_line(&line).unwrap();

        assert_eq!(decoded, request);
    }
}
