mod input;

use std::{path::Path, sync::Arc};

use anyhow::Result;
use gpui::prelude::*;
use gpui::{
    App, Bounds, Context, Entity, FontWeight, Hsla, MouseButton, MouseUpEvent, Render,
    SharedString, Window, WindowBounds, WindowOptions, div, px, rgb, size,
};
use tokio::{runtime::Runtime, sync::oneshot};
use zenapi::{
    client,
    mock_server::MockServer,
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
};

use self::input::{TextChanged, TextInput, bind_text_input_keys};

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);

    gpui_platform::application().run(move |cx: &mut App| {
        bind_text_input_keys(cx);

        let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            {
                let runtime = runtime.clone();
                move |_, cx| cx.new(|cx| ZenApiApp::new(runtime, cx))
            },
        )
        .expect("open ZenAPI window");
        cx.activate(true);
    });

    Ok(())
}

struct ZenApiApp {
    runtime: Arc<Runtime>,
    import_path: Entity<TextInput>,
    route_filter: Entity<TextInput>,
    url: Entity<TextInput>,
    request_body: Entity<TextInput>,
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
    selected_route: Option<usize>,
    method: String,
    spec_label: String,
    response_status: String,
    response_meta: String,
    response_tone: ResponseTone,
    response_body: String,
    server: Option<MockServer>,
    server_running: bool,
    server_status: String,
    busy: bool,
}

impl ZenApiApp {
    fn new(runtime: Arc<Runtime>, cx: &mut Context<Self>) -> Self {
        let import_path = cx.new(|cx| TextInput::new(cx, "OpenAPI / Swagger file path", true));
        let route_filter =
            cx.new(|cx| TextInput::new(cx, "Filter method, path, or summary", false));
        let url = cx.new(|cx| TextInput::new(cx, "Request URL", true));
        let request_body = cx.new(|cx| TextInput::new(cx, "JSON body", true));

        cx.subscribe(&route_filter, |app, _input, event: &TextChanged, cx| {
            app.apply_route_filter(&event.text);
            cx.notify();
        })
        .detach();

        Self {
            runtime,
            import_path,
            route_filter,
            url,
            request_body,
            routes: Vec::new(),
            visible_routes: Vec::new(),
            selected_route: None,
            method: "GET".to_string(),
            spec_label: "No spec loaded".to_string(),
            response_status: "Idle".to_string(),
            response_meta: String::new(),
            response_tone: ResponseTone::Neutral,
            response_body: "Import an OpenAPI or Swagger document to begin.".to_string(),
            server: None,
            server_running: false,
            server_status: "Mock stopped".to_string(),
            busy: false,
        }
    }

    fn import_openapi(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let path = self.import_path.read(cx).text();
        let path = path.trim();
        if path.is_empty() {
            self.set_response(
                "Import needs a file path",
                "",
                ResponseTone::Error,
                "Enter a local OpenAPI or Swagger JSON/YAML file path.",
            );
            cx.notify();
            return;
        }

        match load_openapi_file(path) {
            Ok(spec) => {
                if let Some(server) = self.server.take() {
                    let runtime = self.runtime.clone();
                    runtime.spawn(async move {
                        server.stop().await;
                    });
                }

                let spec_name = display_spec_name(&spec);
                let routes = spec.routes;
                self.visible_routes = routes.clone();
                self.routes = routes;
                self.selected_route = None;
                self.spec_label = display_spec_label(path);
                self.server_running = false;
                self.server_status = if self.routes.is_empty() {
                    "No mock routes".to_string()
                } else {
                    "Mock ready".to_string()
                };
                self.route_filter
                    .update(cx, |input, cx| input.set_text("", cx));
                self.set_response(
                    format!("Imported {spec_name}"),
                    format!("{} routes", self.routes.len()),
                    ResponseTone::Success,
                    format!("Ready: {} routes parsed.", self.routes.len()),
                );
            }
            Err(error) => {
                self.set_response("Import failed", "", ResponseTone::Error, error.to_string());
            }
        }

        cx.notify();
    }

    fn apply_route_filter(&mut self, query: &str) {
        self.visible_routes = filter_routes(&self.routes, query);
        self.selected_route = None;
    }

    fn select_route(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(route) = self.visible_routes.get(index).cloned() else {
            return;
        };

        self.selected_route = Some(index);
        self.method = route.method.clone();
        self.url.update(cx, |input, cx| {
            input.set_text(format!("http://localhost:8080{}", route.path), cx)
        });
        self.request_body.update(cx, |input, cx| {
            input.set_text(default_request_body(&route.method), cx)
        });
        self.set_response(
            "Route selected",
            route.summary,
            ResponseTone::Neutral,
            pretty_json(&route.mock_body),
        );
        cx.notify();
    }

    fn send_request(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let url = self.url.read(cx).text();
        let url = url.trim().to_string();
        if url.is_empty() {
            self.set_response(
                "Request needs a URL",
                "",
                ResponseTone::Error,
                "Enter a request URL or select an imported route first.",
            );
            cx.notify();
            return;
        }

        let method = self.method.clone();
        let body = self.request_body.read(cx).text();
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();

        self.busy = true;
        self.set_response(
            "Sending",
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(client::send_request(&method, &url, &body).await);
        });

        cx.spawn(async move |app, cx| {
            if let Ok(result) = rx.await {
                app.update(cx, |app, cx| {
                    match result {
                        Ok(response) => {
                            app.set_response(
                                format!("HTTP {}", response.status),
                                format!("{} ms", response.elapsed_ms),
                                response_tone(response.status),
                                response.body,
                            );
                        }
                        Err(error) => {
                            app.set_response(
                                "Request failed",
                                "",
                                ResponseTone::Error,
                                error.to_string(),
                            );
                        }
                    }
                    app.busy = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn toggle_mock_server(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if let Some(server) = self.server.take() {
            self.busy = true;
            self.server_running = false;
            self.server_status = "Stopping mock".to_string();
            let runtime = self.runtime.clone();
            let (tx, rx) = oneshot::channel();

            runtime.spawn(async move {
                server.stop().await;
                let _ = tx.send(());
            });

            cx.spawn(async move |app, cx| {
                if rx.await.is_ok() {
                    app.update(cx, |app, cx| {
                        app.busy = false;
                        app.server_running = false;
                        app.server_status = "Mock stopped".to_string();
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
            cx.notify();
            return;
        }

        if self.routes.is_empty() {
            self.set_response(
                "Mock needs routes",
                "",
                ResponseTone::Error,
                "Import an OpenAPI file before starting the mock server.",
            );
            self.server_status = "Import routes first".to_string();
            cx.notify();
            return;
        }

        let routes = self.routes.clone();
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();
        self.busy = true;
        self.server_status = "Starting mock".to_string();
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(MockServer::start(routes, 8080).await);
        });

        cx.spawn(async move |app, cx| {
            if let Ok(result) = rx.await {
                app.update(cx, |app, cx| {
                    match result {
                        Ok(server) => {
                            app.server_status = server.addr().to_string();
                            app.server_running = true;
                            app.server = Some(server);
                        }
                        Err(error) => {
                            app.server_running = false;
                            app.server_status = "Mock failed".to_string();
                            app.set_response(
                                "Mock server failed",
                                "",
                                ResponseTone::Error,
                                error.to_string(),
                            );
                        }
                    }
                    app.busy = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn set_response(
        &mut self,
        status: impl Into<String>,
        meta: impl Into<String>,
        tone: ResponseTone,
        body: impl Into<String>,
    ) {
        self.response_status = status.into();
        self.response_meta = meta.into();
        self.response_tone = tone;
        self.response_body = body.into();
    }

    fn method_button(&self, method: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.method == method;
        let enabled = !self.busy;
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(32.))
            .w(px(74.))
            .rounded(px(5.))
            .border_1()
            .border_color(if active {
                method_color(method)
            } else {
                rgb(0xd1d5db).into()
            })
            .bg(if active { rgb(0xf9fafb) } else { rgb(0xffffff) })
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(method_color(method))
            .opacity(if enabled { 1.0 } else { 0.55 })
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if !app.busy {
                        app.method = method.to_string();
                        cx.notify();
                    }
                }),
            )
            .child(method)
    }

    fn action_button(
        &self,
        label: impl Into<SharedString>,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(34.))
            .w(px(112.))
            .rounded(px(6.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(13.))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(if enabled { 1.0 } else { 0.62 })
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if enabled {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(label.into())
    }

    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_toggle_mock = self.server_running || !self.routes.is_empty();
        div()
            .flex()
            .items_center()
            .h(px(48.))
            .w_full()
            .border_b_1()
            .border_color(rgb(0xe5e7eb))
            .bg(rgb(0xf9fafb))
            .px_3()
            .gap_3()
            .child(
                div()
                    .w(px(230.))
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(15.))
                    .text_color(rgb(0x111827))
                    .child("ZenAPI"),
            )
            .child(div().flex_1().child(self.import_path.clone()))
            .child(self.action_button(
                "Import",
                true,
                ButtonTone::Neutral,
                |app, _event, _window, cx| app.import_openapi(cx),
                cx,
            ))
            .child(
                div()
                    .w(px(124.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(rgb(0x6b7280))
                    .child(self.spec_label.clone()),
            )
            .child(
                div()
                    .w(px(132.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(if self.server_running {
                        ResponseTone::Success.color()
                    } else {
                        rgb(0x6b7280).into()
                    })
                    .child(self.server_status.clone()),
            )
            .child(self.action_button(
                if self.server_running {
                    "Stop Mock"
                } else {
                    "Start Mock"
                },
                can_toggle_mock,
                if self.server_running {
                    ButtonTone::Warning
                } else {
                    ButtonTone::Primary
                },
                |app, _event, _window, cx| app.toggle_mock_server(cx),
                cx,
            ))
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .visible_routes
            .iter()
            .enumerate()
            .map(|(index, route)| {
                self.render_route_row(
                    index,
                    route.method.clone(),
                    route.path.clone(),
                    route.summary.clone(),
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .w(px(320.))
            .h_full()
            .border_r_1()
            .border_color(rgb(0xe5e7eb))
            .bg(rgb(0xf9fafb))
            .p_3()
            .gap_3()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .h(px(24.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x111827))
                            .child("Endpoints"),
                    )
                    .child(
                        div()
                            .text_color(rgb(0x6b7280))
                            .child(if self.routes.is_empty() {
                                String::new()
                            } else {
                                format!("{}/{}", self.visible_routes.len(), self.routes.len())
                            }),
                    ),
            )
            .child(self.route_filter.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .gap_1()
                    .children(rows)
                    .when(self.visible_routes.is_empty(), |list| {
                        list.child(
                            div()
                                .h(px(34.))
                                .flex()
                                .items_center()
                                .text_color(rgb(0x9ca3af))
                                .text_size(px(13.))
                                .child(if self.routes.is_empty() {
                                    "No imported routes"
                                } else {
                                    "No matching routes"
                                }),
                        )
                    }),
            )
    }

    fn render_route_row(
        &self,
        index: usize,
        method: String,
        path: String,
        summary: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + 'static + use<> {
        let selected = self.selected_route == Some(index);
        div()
            .id(("route", index))
            .flex()
            .flex_col()
            .h(px(48.))
            .rounded(px(4.))
            .border_l_2()
            .border_color(if selected {
                rgb(0x2563eb)
            } else {
                rgb(0xf9fafb)
            })
            .bg(if selected {
                rgb(0xeff6ff)
            } else {
                rgb(0xf9fafb)
            })
            .px_2()
            .py_1()
            .cursor_pointer()
            .hover(|row| if selected { row } else { row.bg(rgb(0xf3f4f6)) })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.select_route(index, cx);
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(58.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(method_color(&method))
                            .child(method),
                    )
                    .child(
                        div()
                            .flex_1()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(rgb(0x111827))
                            .font_family("monospace")
                            .child(path),
                    ),
            )
            .child(
                div()
                    .ml(px(66.))
                    .truncate()
                    .text_size(px(12.))
                    .text_color(rgb(0x6b7280))
                    .child(summary),
            )
    }

    fn render_workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(rgb(0xffffff))
            .child(self.render_request_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .child(self.render_request_panel())
                    .child(self.render_response_panel()),
            )
    }

    fn render_request_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(54.))
            .border_b_1()
            .border_color(rgb(0xe5e7eb))
            .px_3()
            .gap_2()
            .child(self.method_button("GET", cx))
            .child(self.method_button("POST", cx))
            .child(self.method_button("PUT", cx))
            .child(self.method_button("PATCH", cx))
            .child(self.method_button("DELETE", cx))
            .child(div().flex_1().child(self.url.clone()))
            .child(self.action_button(
                "Send",
                true,
                ButtonTone::Primary,
                |app, _event, _window, cx| app.send_request(cx),
                cx,
            ))
    }

    fn render_request_panel(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .border_r_1()
            .border_color(rgb(0xe5e7eb))
            .child(panel_header("Request", None, ResponseTone::Neutral))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .p_3()
                    .gap_3()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0x6b7280))
                            .child("Body"),
                    )
                    .child(self.request_body.clone())
                    .child(
                        div()
                            .flex_1()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(rgb(0xe5e7eb))
                            .bg(rgb(0xffffff))
                            .p_3()
                            .font_family("monospace")
                            .text_size(px(13.))
                            .text_color(rgb(0x6b7280))
                            .child("Request body editing is available in the field above."),
                    ),
            )
    }

    fn render_response_panel(&self) -> impl IntoElement {
        let meta = if self.response_meta.is_empty() {
            None
        } else {
            Some(self.response_meta.as_str())
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .child(panel_header(
                &self.response_status,
                meta,
                self.response_tone,
            ))
            .child(
                div()
                    .flex_1()
                    .p_3()
                    .font_family("monospace")
                    .line_height(px(20.))
                    .text_size(px(13.))
                    .text_color(rgb(0x111827))
                    .whitespace_normal()
                    .child(self.response_body.clone()),
            )
    }
}

impl Render for ZenApiApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .text_color(rgb(0x111827))
            .bg(rgb(0xffffff))
            .child(self.render_top_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .h_full()
                    .child(self.render_sidebar(cx))
                    .child(self.render_workspace(cx)),
            )
    }
}

#[derive(Clone, Copy)]
enum ButtonTone {
    Neutral,
    Primary,
    Warning,
}

struct ButtonColors {
    background: Hsla,
    border: Hsla,
    text: Hsla,
}

impl ButtonTone {
    fn colors(self, enabled: bool) -> ButtonColors {
        if !enabled {
            return ButtonColors {
                background: rgb(0xf3f4f6).into(),
                border: rgb(0xd1d5db).into(),
                text: rgb(0x9ca3af).into(),
            };
        }

        match self {
            Self::Neutral => ButtonColors {
                background: rgb(0xffffff).into(),
                border: rgb(0xd1d5db).into(),
                text: rgb(0x374151).into(),
            },
            Self::Primary => ButtonColors {
                background: rgb(0x2563eb).into(),
                border: rgb(0x1d4ed8).into(),
                text: rgb(0xffffff).into(),
            },
            Self::Warning => ButtonColors {
                background: rgb(0xb45309).into(),
                border: rgb(0x92400e).into(),
                text: rgb(0xffffff).into(),
            },
        }
    }
}

#[derive(Clone, Copy)]
enum ResponseTone {
    Neutral,
    Busy,
    Success,
    Error,
}

impl ResponseTone {
    fn color(self) -> Hsla {
        match self {
            Self::Neutral => rgb(0x6b7280).into(),
            Self::Busy => rgb(0xd97706).into(),
            Self::Success => rgb(0x059669).into(),
            Self::Error => rgb(0xdc2626).into(),
        }
    }
}

fn panel_header(
    title: impl Into<SharedString>,
    meta: Option<&str>,
    tone: ResponseTone,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(40.))
        .border_b_1()
        .border_color(rgb(0xe5e7eb))
        .px_3()
        .bg(rgb(0xffffff))
        .child(
            div()
                .font_weight(FontWeight::BOLD)
                .text_size(px(13.))
                .text_color(rgb(0x111827))
                .child(title.into()),
        )
        .child(
            div()
                .w(px(260.))
                .truncate()
                .text_right()
                .font_family("monospace")
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tone.color())
                .child(meta.unwrap_or("").to_string()),
        )
}

fn method_color(method: &str) -> Hsla {
    match method {
        "GET" => rgb(0x059669).into(),
        "POST" => rgb(0xd97706).into(),
        "PUT" => rgb(0x2563eb).into(),
        "PATCH" => rgb(0x7c3aed).into(),
        "DELETE" => rgb(0xdc2626).into(),
        _ => rgb(0x6b7280).into(),
    }
}

fn response_tone(status: u16) -> ResponseTone {
    if (200..400).contains(&status) {
        ResponseTone::Success
    } else if status >= 400 {
        ResponseTone::Error
    } else {
        ResponseTone::Neutral
    }
}

fn display_spec_name(spec: &ApiSpec) -> String {
    if spec.version.is_empty() {
        spec.title.clone()
    } else {
        format!("{} {}", spec.title, spec.version)
    }
}

fn display_spec_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn filter_routes(routes: &[ApiRoute], query: &str) -> Vec<ApiRoute> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return routes.to_vec();
    }

    routes
        .iter()
        .filter(|route| {
            route.method.to_lowercase().contains(&query)
                || route.path.to_lowercase().contains(&query)
                || route.summary.to_lowercase().contains(&query)
        })
        .cloned()
        .collect()
}

fn default_request_body(method: &str) -> &'static str {
    match method {
        "POST" | "PUT" | "PATCH" => "{}",
        _ => "",
    }
}

fn pretty_json(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn route(method: &str, path: &str, summary: &str) -> ApiRoute {
        ApiRoute {
            method: method.to_string(),
            path: path.to_string(),
            summary: summary.to_string(),
            mock_body: json!({}),
        }
    }

    #[test]
    fn filters_routes_by_method_path_or_summary() {
        let routes = vec![
            route("GET", "/users", "List accounts"),
            route("POST", "/sessions", "Create login session"),
            route("DELETE", "/users/{id}", "Remove account"),
        ];

        assert_eq!(filter_routes(&routes, "post"), vec![routes[1].clone()]);
        assert_eq!(filter_routes(&routes, "sessions"), vec![routes[1].clone()]);
        assert_eq!(filter_routes(&routes, "remove"), vec![routes[2].clone()]);
    }

    #[test]
    fn empty_route_filter_returns_all_routes() {
        let routes = vec![
            route("GET", "/users", "List accounts"),
            route("POST", "/sessions", "Create login session"),
        ];

        assert_eq!(filter_routes(&routes, "   "), routes);
    }

    #[test]
    fn maps_http_status_to_response_tone() {
        assert!(matches!(response_tone(200), ResponseTone::Success));
        assert!(matches!(response_tone(302), ResponseTone::Success));
        assert!(matches!(response_tone(100), ResponseTone::Neutral));
        assert!(matches!(response_tone(404), ResponseTone::Error));
        assert!(matches!(response_tone(500), ResponseTone::Error));
    }
}
