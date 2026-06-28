use anyhow::{Result, anyhow, bail};
use zenapi::client;

pub(in crate::app) const MAX_SSE_STREAM_EVENTS: usize = 200;

pub(in crate::app) fn parse_positive_usize(input: &str, field_name: &str) -> Result<usize> {
    let value = input.trim();
    let parsed = value
        .parse::<usize>()
        .map_err(|_| anyhow!("{field_name} must be a positive integer"))?;
    if parsed == 0 {
        bail!("{field_name} must be greater than zero");
    }
    Ok(parsed)
}

pub(in crate::app) fn push_bounded_sse_stream_event(
    events: &mut Vec<client::SseStreamEvent>,
    event: client::SseStreamEvent,
) {
    events.push(event);
    if events.len() > MAX_SSE_STREAM_EVENTS {
        let overflow = events.len() - MAX_SSE_STREAM_EVENTS;
        drop(events.drain(..overflow));
    }
}

pub(in crate::app) fn sse_stream_meta(event_count: usize) -> String {
    if event_count == MAX_SSE_STREAM_EVENTS {
        format!("latest {event_count} events")
    } else {
        format!("{event_count} events")
    }
}

pub(in crate::app) fn sse_stream_event_done(event: &client::SseStreamEvent) -> bool {
    matches!(
        event,
        client::SseStreamEvent::Closed(_) | client::SseStreamEvent::Error(_)
    )
}

pub(in crate::app) fn sse_stream_event_last_id(event: &client::SseStreamEvent) -> Option<&str> {
    match event {
        client::SseStreamEvent::Event(event) => event
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty()),
        _ => None,
    }
}

pub(in crate::app) fn sse_stream_status(event: &client::SseStreamEvent) -> &'static str {
    match event {
        client::SseStreamEvent::Connected { .. } => "SSE stream open",
        client::SseStreamEvent::Event(_) => "SSE event",
        client::SseStreamEvent::Reconnecting { .. } => "SSE reconnecting",
        client::SseStreamEvent::Closed(_) => "SSE closed",
        client::SseStreamEvent::Error(_) => "SSE failed",
    }
}

pub(in crate::app) fn sse_stream_tone(event: &client::SseStreamEvent) -> &'static str {
    match event {
        client::SseStreamEvent::Error(_) => "error",
        client::SseStreamEvent::Closed(_) => "neutral",
        client::SseStreamEvent::Reconnecting { .. } => "busy",
        _ => "success",
    }
}

pub(in crate::app) fn format_sse_stream_events(events: &[client::SseStreamEvent]) -> String {
    events
        .iter()
        .enumerate()
        .map(|(index, event)| format_sse_stream_event(index + 1, event))
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_sse_stream_event(index: usize, event: &client::SseStreamEvent) -> String {
    match event {
        client::SseStreamEvent::Connected { url } => format!("{index}. connected {url}"),
        client::SseStreamEvent::Event(event) => format_sse_event(index, event),
        client::SseStreamEvent::Reconnecting {
            attempt,
            delay_ms,
            reason,
        } => format!("{index}. reconnecting attempt {attempt} in {delay_ms} ms\n{reason}"),
        client::SseStreamEvent::Closed(reason) => format!("{index}. closed\n{reason}"),
        client::SseStreamEvent::Error(error) => format!("{index}. error\n{error}"),
    }
}

pub(in crate::app) fn latest_sse_event_id(events: &[client::SseEvent]) -> Option<&str> {
    events.iter().rev().find_map(|event| {
        event
            .id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
    })
}

pub(in crate::app) fn format_sse_exchange(exchange: &client::SseExchange) -> String {
    let mut lines = vec![format!("URL: {}", exchange.url)];
    for (index, event) in exchange.events.iter().enumerate() {
        lines.push(format_sse_event(index + 1, event));
    }
    lines.join("\n")
}

fn format_sse_event(index: usize, event: &client::SseEvent) -> String {
    let event_name = event.event.as_deref().unwrap_or("message");
    let id = event
        .id
        .as_deref()
        .map(|id| format!(" / id {id}"))
        .unwrap_or_default();
    let retry = event
        .retry
        .map(|retry| format!(" / retry {retry}"))
        .unwrap_or_default();
    format!("{index}. {event_name}{id}{retry}\n{}", event.data)
}
