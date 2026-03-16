use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

use crate::{
    IpcCodecError, IpcRequest, IpcResponse, decode_request_line, decode_response_line,
    encode_request_line, encode_response_line,
};

#[derive(Debug, thiserror::Error)]
pub enum IpcTransportError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Codec(#[from] IpcCodecError),
}

pub fn bind_listener(path: impl AsRef<Path>) -> Result<UnixListener, IpcTransportError> {
    let path = path.as_ref();

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(UnixListener::bind(path)?)
}

pub fn connect(path: impl AsRef<Path>) -> Result<UnixStream, IpcTransportError> {
    Ok(UnixStream::connect(path)?)
}

pub fn send_request(
    stream: &mut UnixStream,
    request: &IpcRequest,
) -> Result<(), IpcTransportError> {
    let line = encode_request_line(request)?;
    stream.write_all(line.as_bytes())?;
    stream.flush()?;
    Ok(())
}

pub fn recv_request(stream: &UnixStream) -> Result<IpcRequest, IpcTransportError> {
    recv_line(stream).and_then(|line| decode_request_line(&line).map_err(Into::into))
}

pub fn send_response(
    stream: &mut UnixStream,
    response: &IpcResponse,
) -> Result<(), IpcTransportError> {
    let line = encode_response_line(response)?;
    stream.write_all(line.as_bytes())?;
    stream.flush()?;
    Ok(())
}

pub fn recv_response(stream: &UnixStream) -> Result<IpcResponse, IpcTransportError> {
    recv_line(stream).and_then(|line| decode_response_line(&line).map_err(Into::into))
}

fn recv_line(stream: &UnixStream) -> Result<String, IpcTransportError> {
    let mut line = String::new();
    let mut reader = BufReader::new(stream.try_clone()?);
    reader.read_line(&mut line)?;
    Ok(line)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use spiders_shared::api::QueryRequest;

    use crate::{IpcClientMessage, IpcEnvelope, IpcServerMessage, IpcSubscriptionTopic};

    use super::*;

    #[test]
    fn listener_bind_replaces_stale_socket_path() {
        let path = unique_socket_path("bind-replaces-stale");

        std::fs::write(&path, b"stale").unwrap();
        let listener = bind_listener(&path).unwrap();

        assert!(path.exists());
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn request_round_trips_over_unix_socket() {
        let path = unique_socket_path("request-round-trip");
        let listener = bind_listener(&path).unwrap();
        let mut client = connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();

        let request = IpcEnvelope::new(IpcClientMessage::subscribe([
            IpcSubscriptionTopic::Focus,
            IpcSubscriptionTopic::Layout,
        ]))
        .with_request_id("req-1");

        send_request(&mut client, &request).unwrap();
        let decoded = recv_request(&server).unwrap();

        assert_eq!(decoded, request);

        drop(server);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn response_round_trips_over_unix_socket() {
        let path = unique_socket_path("response-round-trip");
        let listener = bind_listener(&path).unwrap();
        let client = connect(&path).unwrap();
        let (mut server, _) = listener.accept().unwrap();

        let response = IpcEnvelope::new(IpcServerMessage::Subscribed {
            topics: vec![IpcSubscriptionTopic::All],
        })
        .with_request_id("sub-1");

        send_response(&mut server, &response).unwrap();
        let decoded = recv_response(&client).unwrap();

        assert_eq!(decoded, response);

        drop(server);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn recv_request_surfaces_codec_errors() {
        let path = unique_socket_path("request-codec-error");
        let listener = bind_listener(&path).unwrap();
        let mut client = connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();

        client.write_all(b"{not-json}\n").unwrap();
        client.flush().unwrap();

        let error = recv_request(&server).unwrap_err();

        assert!(matches!(error, IpcTransportError::Codec(_)));

        drop(server);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn recv_response_reads_query_envelope() {
        let path = unique_socket_path("response-query-envelope");
        let listener = bind_listener(&path).unwrap();
        let mut client = connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();

        let response = IpcEnvelope::new(IpcServerMessage::Query(
            spiders_shared::api::QueryResponse::TagNames(vec!["1".into()]),
        ));

        let line = encode_response_line(&response).unwrap();
        client.write_all(line.as_bytes()).unwrap();
        client.flush().unwrap();

        let decoded = recv_response(&server).unwrap();

        assert_eq!(decoded, response);

        drop(server);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }

    fn unique_socket_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("spiders-wm-{label}-{nanos}.sock"))
    }

    #[test]
    fn send_request_writes_query_frame() {
        let path = unique_socket_path("request-query-frame");
        let listener = bind_listener(&path).unwrap();
        let mut client = connect(&path).unwrap();
        let (server, _) = listener.accept().unwrap();

        let request = IpcEnvelope::new(IpcClientMessage::Query(QueryRequest::State));

        send_request(&mut client, &request).unwrap();
        let decoded = recv_request(&server).unwrap();

        assert_eq!(decoded, request);

        drop(server);
        drop(client);
        drop(listener);
        std::fs::remove_file(path).unwrap();
    }
}
