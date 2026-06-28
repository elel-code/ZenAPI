use anyhow::{Result, anyhow, bail};
use zenapi::client;

pub(in crate::app) fn parse_websocket_protocols(input: &str) -> Vec<String> {
    input
        .split([',', '\n'])
        .map(str::trim)
        .filter(|protocol| !protocol.is_empty())
        .map(str::to_string)
        .collect()
}

pub(in crate::app) fn normalize_websocket_message_mode(mode: &str) -> &'static str {
    if mode.trim().eq_ignore_ascii_case("binary") {
        "binary"
    } else {
        "text"
    }
}

pub(in crate::app) fn websocket_message_mode_label(mode: &str) -> &'static str {
    match normalize_websocket_message_mode(mode) {
        "binary" => "binary",
        _ => "text",
    }
}

pub(in crate::app) fn websocket_session_command(
    mode: &str,
    message: &str,
) -> Result<client::WebSocketSessionCommand> {
    match normalize_websocket_message_mode(mode) {
        "binary" => Ok(client::WebSocketSessionCommand::SendBinary(
            parse_websocket_binary_message(message)?,
        )),
        _ => Ok(client::WebSocketSessionCommand::SendText(
            message.to_string(),
        )),
    }
}

pub(in crate::app) fn parse_websocket_binary_message(input: &str) -> Result<Vec<u8>> {
    let tokens = input
        .split(|character: char| character.is_whitespace() || character == ',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        bail!("WebSocket binary message is empty");
    }

    tokens
        .into_iter()
        .map(|token| {
            let token = token
                .strip_prefix("0x")
                .or_else(|| token.strip_prefix("0X"))
                .unwrap_or(token);
            if token.is_empty() || token.len() > 2 {
                bail!("invalid WebSocket binary byte: {token}");
            }
            u8::from_str_radix(token, 16)
                .map_err(|_| anyhow!("invalid WebSocket binary byte: {token}"))
        })
        .collect()
}

pub(in crate::app) fn format_websocket_exchange(exchange: &client::WebSocketExchange) -> String {
    let mut lines = vec![
        format!("URL: {}", exchange.url),
        format!("Sent: {}", exchange.sent),
    ];
    for (index, message) in exchange.received.iter().enumerate() {
        lines.push(format!(
            "Received {} [{}]: {}",
            index + 1,
            websocket_message_kind(&message.kind),
            message.data
        ));
    }
    lines.join("\n")
}

pub(in crate::app) fn format_websocket_session_events(
    events: &[client::WebSocketSessionEvent],
) -> String {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| format_websocket_session_event(index + 1, event))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_websocket_session_event(index: usize, event: &client::WebSocketSessionEvent) -> String {
    match event {
        client::WebSocketSessionEvent::Connected { url } => format!("{index}. connected {url}"),
        client::WebSocketSessionEvent::Sent(message) => {
            format!(
                "{index}. sent [{}]: {}",
                websocket_message_kind(&message.kind),
                message.data
            )
        }
        client::WebSocketSessionEvent::Received(message) => {
            format!(
                "{index}. received [{}]: {}",
                websocket_message_kind(&message.kind),
                message.data
            )
        }
        client::WebSocketSessionEvent::Closed(reason) => format!("{index}. closed {reason}"),
        client::WebSocketSessionEvent::Error(error) => format!("{index}. error {error}"),
    }
}

pub(in crate::app) fn websocket_session_event_done(event: &client::WebSocketSessionEvent) -> bool {
    matches!(
        event,
        client::WebSocketSessionEvent::Closed(_) | client::WebSocketSessionEvent::Error(_)
    )
}

pub(in crate::app) fn websocket_session_status(
    event: &client::WebSocketSessionEvent,
) -> &'static str {
    match event {
        client::WebSocketSessionEvent::Connected { .. } => "WebSocket open",
        client::WebSocketSessionEvent::Sent(_) => "WebSocket sent",
        client::WebSocketSessionEvent::Received(_) => "WebSocket received",
        client::WebSocketSessionEvent::Closed(_) => "WebSocket closed",
        client::WebSocketSessionEvent::Error(_) => "WebSocket failed",
    }
}

pub(in crate::app) fn websocket_session_tone(
    event: &client::WebSocketSessionEvent,
) -> &'static str {
    match event {
        client::WebSocketSessionEvent::Error(_) => "error",
        client::WebSocketSessionEvent::Closed(_) => "neutral",
        _ => "success",
    }
}

fn websocket_message_kind(kind: &client::WebSocketMessageKind) -> &'static str {
    match kind {
        client::WebSocketMessageKind::Text => "text",
        client::WebSocketMessageKind::Binary => "binary",
        client::WebSocketMessageKind::Ping => "ping",
        client::WebSocketMessageKind::Pong => "pong",
        client::WebSocketMessageKind::Close => "close",
    }
}
