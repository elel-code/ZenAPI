use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::{sync::mpsc, time::timeout};

const DEFAULT_SSE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SseExchange {
    pub url: String,
    pub events: Vec<SseEvent>,
    pub elapsed_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
    pub retry: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseStreamEvent {
    Connected { url: String },
    Event(SseEvent),
    Closed(String),
    Error(String),
}

pub async fn collect_sse_events(url: &str, max_events: usize) -> Result<SseExchange> {
    collect_sse_events_with_timeout(url, max_events, DEFAULT_SSE_TIMEOUT).await
}

pub async fn collect_sse_events_with_timeout(
    url: &str,
    max_events: usize,
    request_timeout: Duration,
) -> Result<SseExchange> {
    let url = url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        bail!("SSE URL must start with http:// or https://");
    }
    if max_events == 0 {
        bail!("SSE max events must be greater than zero");
    }

    let started = Instant::now();
    let client = reqwest::Client::builder()
        .user_agent("ZenAPI/0.1")
        .timeout(request_timeout)
        .build()
        .context("failed to build SSE HTTP client")?;
    let response = client
        .get(url)
        .header(ACCEPT, "text/event-stream")
        .send()
        .await
        .with_context(|| format!("failed to connect SSE stream {url}"))?
        .error_for_status()
        .context("SSE stream returned an error status")?;

    if !response_is_sse(&response) {
        bail!("SSE response must use text/event-stream");
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut events = Vec::new();

    while events.len() < max_events {
        let Some(chunk) = timeout(request_timeout, stream.next())
            .await
            .context("SSE receive timed out")?
        else {
            break;
        };
        let chunk = chunk.context("failed to read SSE stream chunk")?;
        buffer.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
        events.extend(drain_sse_events(&mut buffer));
    }

    events.truncate(max_events);
    if events.is_empty() {
        bail!("SSE stream ended without events");
    }

    Ok(SseExchange {
        url: url.to_string(),
        events,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

pub async fn run_sse_subscription(
    url: String,
    last_event_id: Option<String>,
    events: mpsc::UnboundedSender<SseStreamEvent>,
) {
    let url = url.trim().to_string();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        send_sse_stream_event(
            &events,
            SseStreamEvent::Error("SSE URL must start with http:// or https://".into()),
        );
        return;
    }

    let client = match reqwest::Client::builder()
        .user_agent("ZenAPI/0.1")
        .connect_timeout(DEFAULT_SSE_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            send_sse_stream_event(
                &events,
                SseStreamEvent::Error(format!("failed to build SSE HTTP client: {error}")),
            );
            return;
        }
    };

    let mut request = client.get(&url).header(ACCEPT, "text/event-stream");
    if let Some(last_event_id) = last_event_id.filter(|value| !value.trim().is_empty()) {
        request = request.header("last-event-id", last_event_id);
    }

    let response = match request.send().await {
        Ok(response) => match response.error_for_status() {
            Ok(response) => response,
            Err(error) => {
                send_sse_stream_event(
                    &events,
                    SseStreamEvent::Error(format!("SSE stream returned an error status: {error}")),
                );
                return;
            }
        },
        Err(error) => {
            send_sse_stream_event(
                &events,
                SseStreamEvent::Error(format!("failed to connect SSE stream {url}: {error}")),
            );
            return;
        }
    };

    if !response_is_sse(&response) {
        send_sse_stream_event(
            &events,
            SseStreamEvent::Error("SSE response must use text/event-stream".to_string()),
        );
        return;
    }

    if !send_sse_stream_event(&events, SseStreamEvent::Connected { url }) {
        return;
    }

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                send_sse_stream_event(
                    &events,
                    SseStreamEvent::Error(format!("failed to read SSE stream chunk: {error}")),
                );
                return;
            }
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk).replace("\r\n", "\n"));
        for event in drain_sse_events(&mut buffer) {
            if !send_sse_stream_event(&events, SseStreamEvent::Event(event)) {
                return;
            }
        }
    }

    send_sse_stream_event(&events, SseStreamEvent::Closed("stream ended".to_string()));
}

fn send_sse_stream_event(
    events: &mpsc::UnboundedSender<SseStreamEvent>,
    event: SseStreamEvent,
) -> bool {
    events.send(event).is_ok()
}

fn response_is_sse(response: &reqwest::Response) -> bool {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("text/event-stream"))
}

fn drain_sse_events(buffer: &mut String) -> Vec<SseEvent> {
    let mut events = Vec::new();

    while let Some(index) = buffer.find("\n\n") {
        let frame = buffer[..index].to_string();
        buffer.drain(..index + 2);
        if let Some(event) = parse_sse_frame(&frame) {
            events.push(event);
        }
    }

    events
}

fn parse_sse_frame(frame: &str) -> Option<SseEvent> {
    let mut event = None;
    let mut data = Vec::new();
    let mut id = None;
    let mut retry = None;

    for line in frame.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }

        let (field, value) = line.split_once(':').map_or((line, ""), |(field, value)| {
            (field, value.strip_prefix(' ').unwrap_or(value))
        });
        match field {
            "event" => event = Some(value.to_string()),
            "data" => data.push(value.to_string()),
            "id" => id = Some(value.to_string()),
            "retry" => retry = value.parse::<u64>().ok(),
            _ => {}
        }
    }

    let data = data.join("\n");
    if event.is_none() && data.is_empty() && id.is_none() && retry.is_none() {
        return None;
    }

    Some(SseEvent {
        event,
        data,
        id,
        retry,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::State,
        http::HeaderMap,
        response::Sse,
        response::sse::{Event, KeepAlive},
        routing::get,
    };
    use futures_util::stream;
    use std::convert::Infallible;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    async fn next_sse_stream_event(
        events: &mut mpsc::UnboundedReceiver<SseStreamEvent>,
    ) -> SseStreamEvent {
        timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("SSE stream event timeout")
            .expect("SSE stream event")
    }

    #[test]
    fn parses_sse_frames_with_multiline_data_and_metadata() {
        let mut buffer = concat!(
            ": comment\n",
            "id: 7\n",
            "event: patch\n",
            "data: one\n",
            "data: two\n",
            "retry: 3000\n",
            "\n",
            "data: plain\n",
            "\n"
        )
        .to_string();

        assert_eq!(
            drain_sse_events(&mut buffer),
            vec![
                SseEvent {
                    event: Some("patch".to_string()),
                    data: "one\ntwo".to_string(),
                    id: Some("7".to_string()),
                    retry: Some(3000),
                },
                SseEvent {
                    event: None,
                    data: "plain".to_string(),
                    id: None,
                    retry: None,
                },
            ]
        );
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn collects_sse_events_from_local_stream() -> Result<()> {
        async fn events() -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
            let events = vec![
                Ok(Event::default().id("1").event("ready").data("connected")),
                Ok(Event::default().event("message").data("hello")),
            ];
            Sse::new(stream::iter(events)).keep_alive(KeepAlive::default())
        }

        let app = Router::new().route("/events", get(events));
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        let exchange = collect_sse_events_with_timeout(
            &format!("http://{addr}/events"),
            2,
            Duration::from_secs(2),
        )
        .await?;

        assert_eq!(exchange.url, format!("http://{addr}/events"));
        assert_eq!(
            exchange.events,
            vec![
                SseEvent {
                    event: Some("ready".to_string()),
                    data: "connected".to_string(),
                    id: Some("1".to_string()),
                    retry: None,
                },
                SseEvent {
                    event: Some("message".to_string()),
                    data: "hello".to_string(),
                    id: None,
                    retry: None,
                },
            ]
        );

        server.abort();
        Ok(())
    }

    #[tokio::test]
    async fn rejects_non_http_sse_urls() {
        let error = collect_sse_events_with_timeout("ws://localhost", 1, Duration::from_secs(1))
            .await
            .expect_err("invalid scheme");

        assert!(error.to_string().contains("http:// or https://"));
    }

    #[tokio::test]
    async fn subscribes_to_sse_stream_and_sends_last_event_id() -> Result<()> {
        #[derive(Clone)]
        struct TestState {
            last_event_id: Arc<Mutex<Option<String>>>,
        }

        async fn events(
            State(state): State<TestState>,
            headers: HeaderMap,
        ) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
            let last_event_id = headers
                .get("last-event-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            *state.last_event_id.lock().expect("last id lock") = last_event_id;

            let events = vec![
                Ok(Event::default().id("8").event("ready").data("connected")),
                Ok(Event::default().event("message").data("hello")),
            ];
            Sse::new(stream::iter(events)).keep_alive(KeepAlive::default())
        }

        let state = TestState {
            last_event_id: Arc::new(Mutex::new(None)),
        };
        let app = Router::new()
            .route("/events", get(events))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve");
        });

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let subscription = tokio::spawn(run_sse_subscription(
            format!("http://{addr}/events"),
            Some("7".to_string()),
            event_tx,
        ));

        assert_eq!(
            next_sse_stream_event(&mut event_rx).await,
            SseStreamEvent::Connected {
                url: format!("http://{addr}/events"),
            }
        );
        assert_eq!(
            next_sse_stream_event(&mut event_rx).await,
            SseStreamEvent::Event(SseEvent {
                event: Some("ready".to_string()),
                data: "connected".to_string(),
                id: Some("8".to_string()),
                retry: None,
            })
        );
        assert_eq!(
            next_sse_stream_event(&mut event_rx).await,
            SseStreamEvent::Event(SseEvent {
                event: Some("message".to_string()),
                data: "hello".to_string(),
                id: None,
                retry: None,
            })
        );
        assert_eq!(
            next_sse_stream_event(&mut event_rx).await,
            SseStreamEvent::Closed("stream ended".to_string())
        );
        assert_eq!(
            state.last_event_id.lock().expect("last id lock").as_deref(),
            Some("7")
        );

        subscription.await?;
        server.abort();
        Ok(())
    }
}
