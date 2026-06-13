use anyhow::{Context, Result, bail};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::{sync::mpsc, time::timeout};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        Bytes, Message,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue, Request, header::SEC_WEBSOCKET_PROTOCOL},
    },
};

const DEFAULT_WEBSOCKET_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSocketExchange {
    pub url: String,
    pub sent: String,
    pub received: Vec<WebSocketMessage>,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub kind: WebSocketMessageKind,
    pub data: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebSocketMessageKind {
    Text,
    Binary,
    Ping,
    Pong,
    Close,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WebSocketSessionOptions {
    pub headers: Vec<(String, String)>,
    pub protocols: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSocketSessionCommand {
    SendText(String),
    SendBinary(Vec<u8>),
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WebSocketSessionEvent {
    Connected { url: String },
    Sent(WebSocketMessage),
    Received(WebSocketMessage),
    Closed(String),
    Error(String),
}

pub async fn send_websocket_message(url: &str, message: &str) -> Result<WebSocketExchange> {
    send_websocket_message_with_timeout(url, message, DEFAULT_WEBSOCKET_TIMEOUT).await
}

pub async fn send_websocket_message_with_timeout(
    url: &str,
    message: &str,
    request_timeout: Duration,
) -> Result<WebSocketExchange> {
    let url = url.trim();
    if !url.starts_with("ws://") && !url.starts_with("wss://") {
        bail!("WebSocket URL must start with ws:// or wss://");
    }

    let started = Instant::now();
    let (mut socket, _) = timeout(request_timeout, connect_async(url))
        .await
        .context("WebSocket connection timed out")?
        .with_context(|| format!("failed to connect WebSocket {url}"))?;

    timeout(
        request_timeout,
        socket.send(Message::Text(message.to_string().into())),
    )
    .await
    .context("WebSocket send timed out")?
    .context("failed to send WebSocket message")?;

    let mut received = Vec::new();
    loop {
        let Some(message) = timeout(request_timeout, socket.next())
            .await
            .context("WebSocket receive timed out")?
        else {
            bail!("WebSocket closed before receiving a response");
        };
        let message = message.context("failed to receive WebSocket message")?;

        match message {
            Message::Text(text) => {
                received.push(WebSocketMessage {
                    kind: WebSocketMessageKind::Text,
                    data: text.to_string(),
                });
                break;
            }
            Message::Binary(bytes) => {
                received.push(WebSocketMessage {
                    kind: WebSocketMessageKind::Binary,
                    data: websocket_bytes_summary(&bytes),
                });
                break;
            }
            Message::Ping(bytes) => {
                socket
                    .send(Message::Pong(bytes.clone()))
                    .await
                    .context("failed to answer WebSocket ping")?;
                received.push(WebSocketMessage {
                    kind: WebSocketMessageKind::Ping,
                    data: websocket_bytes_summary(&bytes),
                });
            }
            Message::Pong(bytes) => {
                received.push(WebSocketMessage {
                    kind: WebSocketMessageKind::Pong,
                    data: websocket_bytes_summary(&bytes),
                });
            }
            Message::Close(frame) => {
                received.push(WebSocketMessage {
                    kind: WebSocketMessageKind::Close,
                    data: frame
                        .map(|frame| frame.reason.to_string())
                        .filter(|reason| !reason.is_empty())
                        .unwrap_or_else(|| "closed".to_string()),
                });
                break;
            }
            Message::Frame(_) => {}
        }
    }

    let _ = socket.close(None).await;

    Ok(WebSocketExchange {
        url: url.to_string(),
        sent: message.to_string(),
        received,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

pub async fn run_websocket_session(
    url: String,
    commands: mpsc::UnboundedReceiver<WebSocketSessionCommand>,
    events: mpsc::UnboundedSender<WebSocketSessionEvent>,
) {
    run_websocket_session_with_options(url, WebSocketSessionOptions::default(), commands, events)
        .await;
}

pub async fn run_websocket_session_with_options(
    url: String,
    options: WebSocketSessionOptions,
    mut commands: mpsc::UnboundedReceiver<WebSocketSessionCommand>,
    events: mpsc::UnboundedSender<WebSocketSessionEvent>,
) {
    let url = url.trim().to_string();
    if !url.starts_with("ws://") && !url.starts_with("wss://") {
        send_websocket_event(
            &events,
            WebSocketSessionEvent::Error("WebSocket URL must start with ws:// or wss://".into()),
        );
        return;
    }

    let request = match websocket_client_request(&url, &options) {
        Ok(request) => request,
        Err(error) => {
            send_websocket_event(
                &events,
                WebSocketSessionEvent::Error(format!(
                    "invalid WebSocket connection options: {error}"
                )),
            );
            return;
        }
    };

    let (mut socket, _) = match timeout(DEFAULT_WEBSOCKET_TIMEOUT, connect_async(request)).await {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => {
            send_websocket_event(
                &events,
                WebSocketSessionEvent::Error(format!("failed to connect WebSocket {url}: {error}")),
            );
            return;
        }
        Err(_) => {
            send_websocket_event(
                &events,
                WebSocketSessionEvent::Error("WebSocket connection timed out".into()),
            );
            return;
        }
    };

    send_websocket_event(&events, WebSocketSessionEvent::Connected { url });

    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(WebSocketSessionCommand::SendText(text)) => {
                        match socket.send(Message::Text(text.clone().into())).await {
                            Ok(()) => send_websocket_event(
                                &events,
                                WebSocketSessionEvent::Sent(WebSocketMessage {
                                    kind: WebSocketMessageKind::Text,
                                    data: text,
                                }),
                            ),
                            Err(error) => {
                                send_websocket_event(
                                    &events,
                                    WebSocketSessionEvent::Error(format!("failed to send WebSocket message: {error}")),
                                );
                                break;
                            }
                        }
                    }
                    Some(WebSocketSessionCommand::SendBinary(bytes)) => {
                        let summary = websocket_bytes_summary(&Bytes::from(bytes.clone()));
                        match socket.send(Message::Binary(bytes.into())).await {
                            Ok(()) => send_websocket_event(
                                &events,
                                WebSocketSessionEvent::Sent(WebSocketMessage {
                                    kind: WebSocketMessageKind::Binary,
                                    data: summary,
                                }),
                            ),
                            Err(error) => {
                                send_websocket_event(
                                    &events,
                                    WebSocketSessionEvent::Error(format!("failed to send WebSocket message: {error}")),
                                );
                                break;
                            }
                        }
                    }
                    Some(WebSocketSessionCommand::Close) => {
                        let _ = socket.close(None).await;
                        send_websocket_event(
                            &events,
                            WebSocketSessionEvent::Closed("client closed".to_string()),
                        );
                        break;
                    }
                    None => {
                        let _ = socket.close(None).await;
                        send_websocket_event(
                            &events,
                            WebSocketSessionEvent::Closed("command channel closed".to_string()),
                        );
                        break;
                    }
                }
            }
            message = socket.next() => {
                let Some(message) = message else {
                    send_websocket_event(
                        &events,
                        WebSocketSessionEvent::Closed("server closed".to_string()),
                    );
                    break;
                };

                match message {
                    Ok(Message::Ping(bytes)) => {
                        let summary = websocket_bytes_summary(&bytes);
                        if let Err(error) = socket.send(Message::Pong(bytes)).await {
                            send_websocket_event(
                                &events,
                                WebSocketSessionEvent::Error(format!("failed to answer WebSocket ping: {error}")),
                            );
                            break;
                        }
                        send_websocket_event(
                            &events,
                            WebSocketSessionEvent::Received(WebSocketMessage {
                                kind: WebSocketMessageKind::Ping,
                                data: summary,
                            }),
                        );
                    }
                    Ok(Message::Text(text)) => send_websocket_event(
                        &events,
                        WebSocketSessionEvent::Received(WebSocketMessage {
                            kind: WebSocketMessageKind::Text,
                            data: text.to_string(),
                        }),
                    ),
                    Ok(Message::Binary(bytes)) => send_websocket_event(
                        &events,
                        WebSocketSessionEvent::Received(WebSocketMessage {
                            kind: WebSocketMessageKind::Binary,
                            data: websocket_bytes_summary(&bytes),
                        }),
                    ),
                    Ok(Message::Pong(bytes)) => send_websocket_event(
                        &events,
                        WebSocketSessionEvent::Received(WebSocketMessage {
                            kind: WebSocketMessageKind::Pong,
                            data: websocket_bytes_summary(&bytes),
                        }),
                    ),
                    Ok(Message::Close(frame)) => {
                        send_websocket_event(
                            &events,
                            WebSocketSessionEvent::Closed(
                                frame
                                    .map(|frame| frame.reason.to_string())
                                    .filter(|reason| !reason.is_empty())
                                    .unwrap_or_else(|| "server closed".to_string()),
                            ),
                        );
                        break;
                    }
                    Ok(Message::Frame(_)) => {}
                    Err(error) => {
                        send_websocket_event(
                            &events,
                            WebSocketSessionEvent::Error(format!("failed to receive WebSocket message: {error}")),
                        );
                        break;
                    }
                }
            }
        }
    }
}

fn websocket_client_request(url: &str, options: &WebSocketSessionOptions) -> Result<Request<()>> {
    let mut request = url
        .into_client_request()
        .with_context(|| format!("invalid WebSocket URL: {url}"))?;

    for (name, value) in &options.headers {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("invalid WebSocket header name: {name}"))?;
        let header_value = HeaderValue::from_str(value.trim())
            .with_context(|| format!("invalid WebSocket header value for {name}"))?;
        request.headers_mut().insert(header_name, header_value);
    }

    let protocols = websocket_protocol_header(&options.protocols);
    if let Some(protocols) = protocols {
        let header_value =
            HeaderValue::from_str(&protocols).context("invalid WebSocket subprotocol header")?;
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, header_value);
    }

    Ok(request)
}

fn websocket_protocol_header(protocols: &[String]) -> Option<String> {
    let protocols = protocols
        .iter()
        .map(|protocol| protocol.trim())
        .filter(|protocol| !protocol.is_empty())
        .collect::<Vec<_>>();

    (!protocols.is_empty()).then(|| protocols.join(", "))
}

fn send_websocket_event(
    events: &mpsc::UnboundedSender<WebSocketSessionEvent>,
    event: WebSocketSessionEvent,
) {
    let _ = events.send(event);
}

fn websocket_bytes_summary(bytes: &Bytes) -> String {
    format!("{} bytes", bytes.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio_tungstenite::{accept_async, accept_hdr_async};

    async fn next_session_event(
        events: &mut mpsc::UnboundedReceiver<WebSocketSessionEvent>,
    ) -> WebSocketSessionEvent {
        timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("session event timeout")
            .expect("session event")
    }

    #[tokio::test]
    async fn sends_text_message_and_records_echo_response() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("websocket handshake");
            if let Some(Ok(Message::Text(text))) = socket.next().await {
                socket
                    .send(Message::Text(format!("echo:{text}").into()))
                    .await
                    .expect("send echo");
            }
        });

        let exchange = send_websocket_message_with_timeout(
            &format!("ws://{addr}"),
            "hello",
            Duration::from_secs(2),
        )
        .await?;

        assert_eq!(exchange.url, format!("ws://{addr}"));
        assert_eq!(exchange.sent, "hello");
        assert_eq!(
            exchange.received,
            vec![WebSocketMessage {
                kind: WebSocketMessageKind::Text,
                data: "echo:hello".to_string(),
            }]
        );

        server.await?;
        Ok(())
    }

    #[tokio::test]
    async fn rejects_non_websocket_urls() {
        let error = send_websocket_message_with_timeout(
            "http://localhost",
            "hello",
            Duration::from_secs(1),
        )
        .await
        .expect_err("invalid scheme");

        assert!(error.to_string().contains("ws:// or wss://"));
    }

    #[tokio::test]
    async fn keeps_websocket_session_open_for_multiple_messages_and_close() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("websocket handshake");
            while let Some(message) = socket.next().await {
                match message.expect("message") {
                    Message::Text(text) => socket
                        .send(Message::Text(format!("echo:{text}").into()))
                        .await
                        .expect("send echo"),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let session = tokio::spawn(run_websocket_session(
            format!("ws://{addr}"),
            command_rx,
            event_tx,
        ));

        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Connected {
                url: format!("ws://{addr}"),
            }
        );

        command_tx.send(WebSocketSessionCommand::SendText("one".to_string()))?;
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Sent(WebSocketMessage {
                kind: WebSocketMessageKind::Text,
                data: "one".to_string(),
            })
        );
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Received(WebSocketMessage {
                kind: WebSocketMessageKind::Text,
                data: "echo:one".to_string(),
            })
        );

        command_tx.send(WebSocketSessionCommand::SendText("two".to_string()))?;
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Sent(WebSocketMessage {
                kind: WebSocketMessageKind::Text,
                data: "two".to_string(),
            })
        );
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Received(WebSocketMessage {
                kind: WebSocketMessageKind::Text,
                data: "echo:two".to_string(),
            })
        );

        command_tx.send(WebSocketSessionCommand::Close)?;
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Closed("client closed".to_string())
        );

        session.await?;
        server.await?;
        Ok(())
    }

    #[tokio::test]
    async fn sends_binary_messages_in_websocket_session() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let mut socket = accept_async(stream).await.expect("websocket handshake");
            while let Some(message) = socket.next().await {
                match message.expect("message") {
                    Message::Binary(bytes) => socket
                        .send(Message::Binary(bytes))
                        .await
                        .expect("send binary echo"),
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let session = tokio::spawn(run_websocket_session(
            format!("ws://{addr}"),
            command_rx,
            event_tx,
        ));

        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Connected {
                url: format!("ws://{addr}"),
            }
        );

        command_tx.send(WebSocketSessionCommand::SendBinary(vec![0, 1, 255]))?;
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Sent(WebSocketMessage {
                kind: WebSocketMessageKind::Binary,
                data: "3 bytes".to_string(),
            })
        );
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Received(WebSocketMessage {
                kind: WebSocketMessageKind::Binary,
                data: "3 bytes".to_string(),
            })
        );

        command_tx.send(WebSocketSessionCommand::Close)?;
        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Closed("client closed".to_string())
        );

        session.await?;
        server.await?;
        Ok(())
    }

    #[tokio::test]
    async fn sends_websocket_session_headers_and_subprotocols() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let seen = Arc::new(Mutex::new((None::<String>, None::<String>)));
        let seen_for_server = seen.clone();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let seen_for_callback = seen_for_server.clone();
            let mut socket = accept_hdr_async(stream, move |request, response| {
                *seen_for_callback.lock().expect("seen lock") = (
                    request
                        .headers()
                        .get("x-token")
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                    request
                        .headers()
                        .get(SEC_WEBSOCKET_PROTOCOL)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string),
                );
                Ok(response)
            })
            .await
            .expect("websocket handshake");
            let _ = socket.close(None).await;
        });

        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let session = tokio::spawn(run_websocket_session_with_options(
            format!("ws://{addr}"),
            WebSocketSessionOptions {
                headers: vec![("X-Token".to_string(), "secret".to_string())],
                protocols: vec!["chat".to_string(), "superchat".to_string()],
            },
            command_rx,
            event_tx,
        ));

        assert_eq!(
            next_session_event(&mut event_rx).await,
            WebSocketSessionEvent::Connected {
                url: format!("ws://{addr}"),
            }
        );
        command_tx.send(WebSocketSessionCommand::Close)?;
        let _ = next_session_event(&mut event_rx).await;

        session.await?;
        server.await?;
        assert_eq!(
            *seen.lock().expect("seen lock"),
            (
                Some("secret".to_string()),
                Some("chat, superchat".to_string())
            )
        );
        Ok(())
    }
}
