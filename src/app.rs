mod input;
mod read_only_text;

use std::{net::SocketAddr, ops::Range, path::Path, sync::Arc};

use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClipboardItem, Context, DefiniteLength, DragMoveEvent, Entity, Focusable,
    FontWeight, HighlightStyle, Hsla, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, ScrollHandle, SharedString, StyledText, Window, WindowBounds,
    WindowOptions, actions, canvas, div, point, px, rgb, size,
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
    task::JoinHandle,
};
use zenapi::{
    assertions::{
        ResponseAssertion, ResponseAssertionKind, ResponseAssertionResult,
        evaluate_response_assertions,
    },
    client::{self, RequestBody},
    codegen::{CodegenRequest, SnippetLanguage, generate_snippet},
    collection_runner::{
        self, CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions,
    },
    collections::{
        ApiCollection, CollectionBody, CollectionFolder, CollectionItem, CollectionRequest,
        NameValue,
    },
    history::{HistoryRequest, HistoryResponse, RequestHistory},
    mock_server::{MockRequestLog, MockServer},
    openapi::{ApiRoute, ApiSpec, load_openapi_file},
    pre_request::{
        execute_pre_request_actions, pre_request_action_labels, resolve_codegen_request_templates,
    },
    variables::{Variable, VariableStore},
};

use self::{
    input::{TextAccepted, TextChanged, TextInput, TextInputChrome, bind_text_input_keys},
    read_only_text::{ReadOnlyTextView, bind_read_only_text_keys},
};

actions!(
    zenapi_app,
    [
        SendCurrentRequest,
        SaveCurrentRequest,
        FocusActiveSidebarInput,
        FocusRequestUrl,
        SelectRequestTabParams,
        SelectRequestTabHeaders,
        SelectRequestTabAuth,
        SelectRequestTabBody,
        SelectRequestTabScripts,
        SelectRequestTabRealtime,
        SelectRequestTabTools,
        SelectResponseTabPretty,
        SelectResponseTabRaw,
        SelectResponseTabHeaders,
        CloseTransientUi
    ]
);

#[cfg(test)]
use zenapi::variables::replace_variables;

const PLATFORM_UI_FONT: &str = ".SystemUIFont";
const PLATFORM_MONOSPACE_FONT: &str = "monospace";
const INITIAL_RESPONSE_BODY: &str = "No response";
const CODEGEN_EMPTY_SNIPPET_LABEL: &str = "No URL";
const PENDING_RESPONSE_BODY_LABEL: &str = "Pending";
const MOCK_STATUS_STOPPED: &str = "Mock stopped";
const MOCK_STATUS_READY: &str = "Mock ready";
const MOCK_STATUS_NO_ROUTES: &str = "No mock routes";
const MOCK_STATUS_IMPORT_ROUTES_FIRST: &str = "No routes";
const MOCK_STATUS_STARTING: &str = "Mock start";
const MOCK_STATUS_STOPPING: &str = "Mock stop";
const MOCK_STATUS_FAILED: &str = "Mock fail";
const REQUEST_URL_REQUIRED_TITLE: &str = "No URL";
const SAVE_URL_REQUIRED_TITLE: &str = "No URL";
const URL_REQUIRED_BODY: &str = "URL is empty.";
const PATH_REQUIRED_BODY: &str = "Path is empty.";
const RESPONSE_TITLE_IMPORTED: &str = "Imported";
const RESPONSE_TITLE_EXPORTED: &str = "Exported";
const RESPONSE_TITLE_SAVED: &str = "Saved";
const RESPONSE_TITLE_RESTORED: &str = "Restored";
const RESPONSE_TITLE_SELECTED: &str = "Selected";
const RESPONSE_TITLE_IMPORT_FAIL: &str = "Import fail";
const RESPONSE_TITLE_EXPORT_FAIL: &str = "Export fail";
const RESPONSE_TITLE_SAVE_FAIL: &str = "Save fail";
const RESPONSE_TITLE_BUILD_FAIL: &str = "Build fail";
const RESPONSE_TITLE_BAD_TESTS: &str = "Bad tests";
const RESPONSE_TITLE_REQUEST_FAIL: &str = "Request fail";
const RESPONSE_TITLE_RUNNER_FAIL: &str = "Runner fail";
const RESPONSE_TITLE_RUN_PASSED: &str = "Run passed";
const RESPONSE_TITLE_RUN_STOPPED: &str = "Run stopped";
const RESPONSE_TITLE_RUN_FAILED: &str = "Run fail";
const RESPONSE_TITLE_FORMAT_FAIL: &str = "Format fail";
const RESPONSE_BODY_REQUEST: &str = "Request.";
const RESPONSE_BODY_RUNNER: &str = "Runner.";
const NO_HEADERS_BODY: &str = "No headers.";
const NO_MESSAGES_BODY: &str = "No messages.";
const NO_EVENTS_BODY: &str = "No events.";
const IMPORT_PATH_REQUIRED_TITLE: &str = "No path";
const COLLECTION_PATH_REQUIRED_TITLE: &str = "No path";
const EXPORT_PATH_REQUIRED_TITLE: &str = "No path";
const COLLECTION_EXPORT_EMPTY_TITLE: &str = "No export";
const SAVED_REQUESTS_EMPTY_BODY: &str = "No saved requests.";
const RUNNER_EMPTY_TITLE: &str = "No requests";
const MOCK_ROUTES_REQUIRED_TITLE: &str = "No routes";
const MOCK_ROUTES_REQUIRED_BODY: &str = "No routes loaded.";
const UI_COLOR_SURFACE: u32 = 0xffffff;
const UI_COLOR_SURFACE_MUTED: u32 = UI_COLOR_SURFACE;
const UI_COLOR_APP_CHROME: u32 = 0xf7f7f8;
const UI_COLOR_WORKSPACE_GUTTER: u32 = 0xf0f1f3;
const UI_COLOR_SIDEBAR_PANE: u32 = UI_COLOR_SURFACE;
const UI_COLOR_REQUEST_PANE: u32 = UI_COLOR_SURFACE;
const UI_COLOR_REQUEST_TAB_BAR: u32 = UI_COLOR_SURFACE_MUTED;
const UI_COLOR_RESPONSE_PANE: u32 = UI_COLOR_SURFACE;
const UI_COLOR_RESPONSE_TAB_BAR: u32 = UI_COLOR_SURFACE_MUTED;
const UI_COLOR_HOVER: u32 = 0xf4f4f5;
const UI_COLOR_DISABLED_SURFACE: u32 = 0xf2f2f3;
const UI_COLOR_DISABLED_BORDER: u32 = 0xd9d9df;
const UI_COLOR_DISABLED_TEXT: u32 = 0x8a8f98;
const UI_COLOR_BORDER: u32 = 0xe2e4e8;
const UI_COLOR_BORDER_STRONG: u32 = 0xaeb7c2;
const UI_COLOR_TEXT_PRIMARY: u32 = 0x111827;
const UI_COLOR_TEXT_SECONDARY: u32 = 0x4b5563;
const UI_COLOR_TEXT_MUTED: u32 = 0x64748b;
const UI_COLOR_TEXT_PLACEHOLDER: u32 = 0xb8c0cc;
const UI_COLOR_TEXT_BODY: u32 = 0x1f2937;
const UI_COLOR_SIDEBAR_DETAIL_TEXT: u32 = UI_COLOR_TEXT_BODY;
const UI_COLOR_ACCENT: u32 = 0x2563eb;
const UI_COLOR_ACCENT_TEXT: u32 = 0x1d4ed8;
const UI_COLOR_ACCENT_SELECTION_RGBA: u32 = 0x332563eb;
const UI_COLOR_WARNING: u32 = 0xb45309;
const UI_COLOR_WARNING_STRONG: u32 = 0x92400e;
const UI_COLOR_STATUS_BUSY: u32 = 0xd97706;
const UI_COLOR_STATUS_SUCCESS: u32 = 0x059669;
const UI_COLOR_STATUS_ERROR: u32 = 0xdc2626;
const UI_COLOR_METHOD_PATCH: u32 = 0x7c3aed;
const UI_COLOR_METHOD_OPTIONS: u32 = 0x0891b2;
const UI_COLOR_METHOD_HEAD: u32 = 0x4b5563;
const UI_COLOR_SYNTAX_STRING: u32 = 0x047857;
const UI_COLOR_SYNTAX_NUMBER: u32 = UI_COLOR_METHOD_PATCH;
const UI_COLOR_SYNTAX_KEYWORD: u32 = UI_COLOR_ACCENT;
const UI_COLOR_SYNTAX_PUNCTUATION: u32 = UI_COLOR_TEXT_SECONDARY;
const UI_COLOR_SYNTAX_TAG: u32 = UI_COLOR_WARNING;
const UI_COLOR_SYNTAX_ATTRIBUTE: u32 = UI_COLOR_METHOD_OPTIONS;
const PLACEHOLDER_IMPORT_PATH: &str = "Spec path";
const PLACEHOLDER_COLLECTION_PATH: &str = "JSON path";
const PLACEHOLDER_COLLECTION_ITEM: &str = "Item name";
const PLACEHOLDER_ROUTE_FILTER: &str = "Filter";
const PLACEHOLDER_HISTORY_FILTER: &str = "Filter";
const PLACEHOLDER_REQUEST_URL: &str = "URL";
const PLACEHOLDER_ENVIRONMENT_NAME: &str = "Env";
const PLACEHOLDER_BEARER_TOKEN: &str = "Token";
const PLACEHOLDER_OAUTH2_ACCESS_TOKEN: &str = "Access token";
const PLACEHOLDER_BASIC_USERNAME: &str = "User";
const PLACEHOLDER_BASIC_PASSWORD: &str = "Pass";
const PLACEHOLDER_JWT_TOKEN: &str = "JWT";
const PLACEHOLDER_API_KEY_NAME: &str = "X-API-Key";
const PLACEHOLDER_API_KEY_VALUE: &str = "Key value";
const PLACEHOLDER_REQUEST_BODY: &str = "Body";
const PLACEHOLDER_GRAPHQL_QUERY: &str = "Query";
const PLACEHOLDER_GRAPHQL_VARIABLES: &str = "Vars";
const PLACEHOLDER_BINARY_BODY_PATH: &str = "File path";
const APP_BRAND_LABEL: &str = "ZenAPI";
const TOP_BAR_IMPORT_LABEL: &str = "Import";
const IMPORT_OPEN_LABEL: &str = "Open";
const MOCK_START_LABEL: &str = "Mock";
const MOCK_STOP_LABEL: &str = "Stop";
const REQUEST_SEND_LABEL: &str = "Send";
const AUTH_PANEL_TITLE: &str = "Auth";
const AUTH_NONE_LABEL: &str = "None";
const AUTH_BEARER_LABEL: &str = "Bearer";
const AUTH_OAUTH_LABEL: &str = "OAuth";
const AUTH_BASIC_LABEL: &str = "Basic";
const AUTH_JWT_LABEL: &str = "JWT";
const AUTH_API_KEY_LABEL: &str = "API";
const AUTH_API_KEY_HEADER_LABEL: &str = "Header";
const AUTH_API_KEY_QUERY_LABEL: &str = "Query";
const HEADERS_PANEL_TITLE: &str = "Hdrs";
const HEADER_COPY_BULK_LABEL: &str = "Copy";
const HEADER_PASTE_BULK_LABEL: &str = "Paste";
const HEADER_ACCEPT_JSON_LABEL: &str = "Accept";
const HEADER_CONTENT_JSON_LABEL: &str = "Content";
const HEADER_BEARER_AUTH_LABEL: &str = "Bearer";
const HEADER_BULK_CLIPBOARD_EMPTY_TITLE: &str = "No paste";
const HEADER_BULK_CLIPBOARD_EMPTY_BODY: &str = "Clipboard empty.";
const HEADER_BULK_PARSE_EMPTY_TITLE: &str = "No paste";
const HEADER_BULK_PARSE_EMPTY_BODY: &str = "No headers.";
const HEADER_COPY_EMPTY_TITLE: &str = "No copy";
const HEADER_COPIED_TITLE: &str = "Copied";
const HEADER_APPLIED_TITLE: &str = "Applied";
const HEADER_BULK_HEADERS_BODY: &str = "Headers.";
const HEADER_PRESET_BODY: &str = "Header.";
const PARAMS_PANEL_TITLE: &str = "Params";
const PRE_REQUEST_PANEL_TITLE: &str = "Pre";
const RUNNER_PANEL_TITLE: &str = "Runner";
const RUNNER_STOP_ON_FAILURE_LABEL: &str = "Stop Fail";
const RUNNER_RUN_ALL_LABEL: &str = "Run";
const RUNNER_ACTIVE_TITLE: &str = "Running";
const RUNNER_EMPTY_REQUESTS_LABEL: &str = "No requests";
const RUNNER_EMPTY_RESULTS_LABEL: &str = "No results";
const RAW_FORMAT_JSON_LABEL: &str = "Format";
const RAW_FORMAT_JSON_MODE_LABEL: &str = "JSON";
const RAW_FORMAT_XML_MODE_LABEL: &str = "XML";
const RAW_FORMAT_TEXT_MODE_LABEL: &str = "Text";
const RAW_FORMAT_HTML_MODE_LABEL: &str = "HTML";
const RAW_FORMATTED_TITLE: &str = "Formatted";
const RAW_FORMATTED_BODY: &str = "Body.";
const RAW_PREVIEW_TITLE: &str = "Preview";
const RAW_EMPTY_PREVIEW_LABEL: &str = "No body";
const RAW_JSON_EMPTY_FORMAT_BODY: &str = "JSON body is empty.";
const GRAPHQL_PANEL_TITLE: &str = "GraphQL";
const GRAPHQL_INTROSPECT_LABEL: &str = "Schema";
const GRAPHQL_PAYLOAD_TITLE: &str = "Payload";
const GRAPHQL_SCHEMA_TITLE: &str = "Schema";
const GRAPHQL_SCHEMA_BROWSER_TITLE: &str = "Fields";
const GRAPHQL_QUERY_ASSISTANT_TITLE: &str = "Templates";
const CODEGEN_PANEL_TITLE: &str = "Code";
const CODEGEN_COPY_LABEL: &str = "Copy";
const BODY_MODE_NONE_LABEL: &str = "None";
const BODY_MODE_FORM_LABEL: &str = "Form";
const BODY_MODE_URL_ENCODED_LABEL: &str = "URL Enc";
const BODY_MODE_RAW_LABEL: &str = "Raw";
const BODY_MODE_GRAPHQL_LABEL: &str = "GraphQL";
const BODY_MODE_BINARY_LABEL: &str = "Binary";
const BODY_FORM_FIELDS_TITLE: &str = "Form";
const BODY_URL_ENCODED_TITLE: &str = "URL Enc";
const SIDEBAR_ROUTES_LABEL: &str = "Routes";
const SIDEBAR_SAVED_LABEL: &str = "Saved";
const SIDEBAR_HISTORY_LABEL: &str = "History";
const SIDEBAR_EMPTY_ROUTES_LABEL: &str = "No routes";
const SIDEBAR_EMPTY_SAVED_LABEL: &str = "No saved";
const SIDEBAR_EMPTY_HISTORY_LABEL: &str = "No history";
const SIDEBAR_EMPTY_MATCHES_LABEL: &str = "No matches";
const COLLECTION_IMPORT_LABEL: &str = "Import";
const COLLECTION_SAVE_LABEL: &str = "Save";
const COLLECTION_EXPORT_LABEL: &str = "Export";
const COLLECTION_POSTMAN_LABEL: &str = "PM";
const COLLECTION_MENU_CLOSE_LABEL: &str = "x";
const COLLECTION_MENU_NEW_REQUEST_LABEL: &str = "+ Req";
const COLLECTION_MENU_NEW_FOLDER_LABEL: &str = "+ Dir";
const COLLECTION_MENU_COPY_LABEL: &str = "Copy";
const COLLECTION_MENU_DELETE_LABEL: &str = "Del";
const COLLECTION_MENU_RENAME_LABEL: &str = "Rename";
const VARIABLES_PANEL_TITLE: &str = "Vars";
const VARIABLES_ENV_LABEL: &str = "Env";
const VARIABLES_NO_ENV_LABEL: &str = "No Env";
const VARIABLES_ADD_ENV_LABEL: &str = "+ Env";
const VARIABLES_DELETE_ENV_LABEL: &str = "Del";
const VARIABLES_GLOBAL_TITLE: &str = "Global";
const VARIABLES_ENV_TITLE: &str = "Env";
const VARIABLES_ENV_NEEDED_TITLE: &str = "Env needed";
const VARIABLES_ENV_NAME_REQUIRED_BODY: &str = "Env name is empty.";
const VARIABLES_ENV_SELECTED_TITLE: &str = "Active";
const VARIABLES_ENV_CREATED_TITLE: &str = "Created";
const VARIABLES_ENV_DELETED_TITLE: &str = "Removed";
const VARIABLES_ENV_RESPONSE_BODY: &str = "Env.";
const PLACEHOLDER_WEBSOCKET_URL: &str = "WS URL";
const PLACEHOLDER_WEBSOCKET_PROTOCOLS: &str = "Protocols";
const PLACEHOLDER_SSE_URL: &str = "SSE URL";
const REALTIME_WEBSOCKET_TITLE: &str = "WS";
const WEBSOCKET_ACTIVE_TITLE: &str = "WS active";
const WEBSOCKET_CONNECTED_TITLE: &str = "WS open";
const WEBSOCKET_MESSAGE_TITLE: &str = "WS msg";
const WEBSOCKET_CLOSED_TITLE: &str = "WS closed";
const WEBSOCKET_BINARY_INVALID_TITLE: &str = "WS invalid";
const WEBSOCKET_SEND_FAILED_TITLE: &str = "WS send fail";
const WEBSOCKET_FAILED_TITLE: &str = "WS failed";
const WEBSOCKET_URL_REQUIRED_TITLE: &str = "WS no URL";
const WEBSOCKET_URL_INVALID_TITLE: &str = "Bad WS URL";
const WEBSOCKET_URL_INVALID_BODY: &str = "Expected WS(S).";
const REALTIME_WEBSOCKET_CONNECT_LABEL: &str = "Open";
const REALTIME_WEBSOCKET_SEND_LABEL: &str = "Send";
const REALTIME_WEBSOCKET_CLOSE_LABEL: &str = "End";
const REALTIME_WEBSOCKET_HEADERS_TITLE: &str = "WS Hdrs";
const REALTIME_WEBSOCKET_TEXT_LABEL: &str = "Text";
const REALTIME_WEBSOCKET_BINARY_LABEL: &str = "Hex";
const REALTIME_WEBSOCKET_EMPTY_LABEL: &str = "No messages";
const WEBSOCKET_NOT_OPEN_TITLE: &str = "WS not open";
const WEBSOCKET_NOT_OPEN_BODY: &str = "No active session.";
const WEBSOCKET_BINARY_EMPTY_BODY: &str = "Hex body is empty.";
const WEBSOCKET_BINARY_ODD_DIGITS_BODY: &str = "Odd hex length.";
const REALTIME_SSE_TITLE: &str = "SSE";
const SSE_URL_REQUIRED_TITLE: &str = "SSE no URL";
const SSE_URL_INVALID_TITLE: &str = "Bad SSE URL";
const SSE_URL_INVALID_BODY: &str = "Expected HTTP(S).";
const SSE_ACTIVE_TITLE: &str = "SSE active";
const SSE_OK_TITLE: &str = "SSE OK";
const SSE_SUBSCRIBING_TITLE: &str = "SSE sub";
const SSE_STOPPED_TITLE: &str = "Stopped";
const SSE_STOPPED_BODY: &str = "SSE.";
const SSE_SUBSCRIBED_TITLE: &str = "SSE open";
const SSE_EVENT_TITLE: &str = "SSE event";
const SSE_RETRY_TITLE: &str = "SSE retry";
const SSE_CLOSED_TITLE: &str = "SSE closed";
const SSE_FAILED_TITLE: &str = "SSE failed";
const REALTIME_STATUS_NO_URL: &str = "No URL";
const REALTIME_STATUS_BAD_URL: &str = "Bad URL";
const REALTIME_SSE_FETCH_LABEL: &str = "Once";
const REALTIME_SSE_SUBSCRIBE_LABEL: &str = "Stream";
const REALTIME_SSE_STOP_LABEL: &str = "Stop";
const REALTIME_SSE_HEADERS_TITLE: &str = "SSE Hdrs";
const REALTIME_SSE_EMPTY_LABEL: &str = "No events";
const MOCK_LOG_EMPTY_LABEL: &str = "No logs";
const REALTIME_LOG_EMPTY_TITLE: &str = "No log";
const REALTIME_LOG_COPIED_TITLE: &str = "Copied";
const REALTIME_LOG_CLEARED_TITLE: &str = "Cleared";
const REALTIME_LOG_BODY: &str = "Log.";
const MOCK_LOG_TITLE: &str = "Log";
const TESTS_PANEL_TITLE: &str = "Tests";
const TEST_ASSERTION_NAME_HEADER: &str = "Test";
const TEST_ASSERTION_KIND_HEADER: &str = "Kind";
const TEST_ASSERTION_TARGET_HEADER: &str = "Target";
const TEST_ASSERTION_EXPECTED_HEADER: &str = "Expect";
const PLACEHOLDER_TEST_ASSERTION_NAME: &str = "Test";
const PLACEHOLDER_TEST_ASSERTION_TARGET: &str = "Target";
const PLACEHOLDER_TEST_ASSERTION_EXPECTED: &str = "Expect";
const TEST_ASSERTION_STATUS_EQUALS_LABEL: &str = "Status =";
const TEST_ASSERTION_STATUS_RANGE_LABEL: &str = "Range";
const TEST_ASSERTION_HEADER_EXISTS_LABEL: &str = "Header ?";
const TEST_ASSERTION_HEADER_EQUALS_LABEL: &str = "Header =";
const TEST_ASSERTION_BODY_CONTAINS_LABEL: &str = "Body ?";
const TEST_ASSERTION_JSON_PATH_EQUALS_LABEL: &str = "JSON =";
const RESPONSE_PRETTY_LABEL: &str = "Pretty";
const RESPONSE_RAW_LABEL: &str = "Raw";
const RESPONSE_HEADERS_LABEL: &str = "Hdrs";
const RESPONSE_FOLD_LABEL: &str = "Fold";
const RESPONSE_OPEN_LABEL: &str = "Open";
const RESPONSE_COPY_LABEL: &str = "Copy";
const RESPONSE_HEADERS_EMPTY_LABEL: &str = "No headers";
const APP_WINDOW_WIDTH: f32 = 1180.;
const APP_WINDOW_HEIGHT: f32 = 760.;
const SIDEBAR_WIDTH: f32 = 320.;
const WORKSPACE_SIDEBAR_DEFAULT_RATIO: f32 = SIDEBAR_WIDTH / APP_WINDOW_WIDTH;
const WORKSPACE_SIDEBAR_MIN_RATIO: f32 = 0.24;
const WORKSPACE_SIDEBAR_MAX_RATIO: f32 = 0.38;
const WORKSPACE_REQUEST_DEFAULT_RATIO: f32 = 0.37;
const WORKSPACE_REQUEST_MIN_RATIO: f32 = 0.32;
const WORKSPACE_REQUEST_MAX_RATIO: f32 = 0.56;
const WORKSPACE_RESPONSE_MIN_RATIO: f32 = 0.24;
const WORKSPACE_SPLIT_HANDLE_WIDTH: f32 = 8.;
const WORKSPACE_SPLIT_DIVIDER_WIDTH: f32 = 1.;
const WORKSPACE_SPLIT_RATIO_EPSILON: f32 = 0.001;
const WORKSPACE_SPLIT_UPDATE_STEP_PX: f32 = 48.;
const SIDEBAR_NAV_HEIGHT: f32 = 42.;
const LAYOUT_ZERO: f32 = 0.;
const SCROLLBAR_HIDDEN_SIZE: f32 = LAYOUT_ZERO;
const SCROLLBAR_WIDTH: f32 = 6.;
const SCROLLBAR_RIGHT_OFFSET: f32 = 3.;
const SCROLLBAR_GUTTER_WIDTH: f32 = SCROLLBAR_WIDTH + SCROLLBAR_RIGHT_OFFSET * 2.;
const SCROLLBAR_CONTENT_RIGHT_PADDING: f32 = SCROLLBAR_GUTTER_WIDTH + 8.;
const SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 28.;
const TOP_BAR_HEIGHT: f32 = 40.;
const TOP_BAR_BRAND_WIDTH: f32 = 72.;
const TOP_BAR_ACTION_WIDTH: f32 = 76.;
const TOP_BAR_MOCK_ACTION_WIDTH: f32 = 68.;
const DISABLED_CONTROL_OPACITY: f32 = 0.78;
const IMPORT_POPOVER_WIDTH: f32 = 520.;
const IMPORT_POPOVER_HEIGHT: f32 = 58.;
const IMPORT_POPOVER_PADDING: f32 = 10.;
const IMPORT_POPOVER_TOP_OFFSET: f32 = TOP_BAR_HEIGHT + 6.;
const IMPORT_POPOVER_RIGHT_OFFSET: f32 = TOP_BAR_MOCK_ACTION_WIDTH + 20.;
const SECTION_HEADER_HEIGHT: f32 = 24.;
const ROUTE_ROW_HEIGHT: f32 = 48.;
const HISTORY_ROW_HEIGHT: f32 = 46.;
const ROUTE_SELECTED_MARKER_WIDTH: f32 = 3.;
const REQUEST_BAR_HEIGHT: f32 = 54.;
const REQUEST_BAR_CONTROL_Y_OFFSET: f32 = 8.;
const REQUEST_METHOD_SEGMENT_WIDTH: f32 = 100.;
const REQUEST_ADDRESS_DIVIDER_HEIGHT: f32 = 22.;
const REQUEST_ADDRESS_DIVIDER_WIDTH: f32 = 1.;
const REQUEST_ADDRESS_RADIUS: f32 = TEXT_INPUT_RADIUS;
const REQUEST_EDITOR_TAB_BAR_HEIGHT: f32 = 34.;
const REQUEST_EDITOR_TAB_COUNT: usize = 7;
const METHOD_MENU_WIDTH: f32 = REQUEST_METHOD_SEGMENT_WIDTH;
const METHOD_MENU_ITEM_HEIGHT: f32 = 30.;
const METHOD_MENU_TOP_OFFSET: f32 =
    PANEL_HEADER_HEIGHT + REQUEST_BAR_CONTROL_Y_OFFSET + TEXT_INPUT_HEIGHT + 4.;
const METHOD_MENU_LEFT_OFFSET: f32 = 12.;
const REQUEST_SEND_WIDTH: f32 = 86.;
const ACTION_BUTTON_WIDTH: f32 = 112.;
const ACTION_BUTTON_HEIGHT: f32 = 34.;
const SIDEBAR_BUTTON_HEIGHT: f32 = 28.;
const SIDEBAR_SECTION_BUTTON_HEIGHT: f32 = 28.;
const COMPACT_CONTROL_HEIGHT: f32 = 28.;
const COMPACT_TOGGLE_SHORT_WIDTH: f32 = 76.;
const COMPACT_TOGGLE_LONG_WIDTH: f32 = 156.;
const COMPACT_TOGGLE_LONG_LABEL_THRESHOLD: usize = 12;
const TOP_BAR_ACTION_HEIGHT: f32 = 30.;
const STATUS_BAR_HEIGHT: f32 = 32.;
const RESPONSE_TAB_BAR_HEIGHT: f32 = 36.;
const RESPONSE_TAB_WIDTH: f32 = 72.;
const RESPONSE_TAB_COUNT: usize = 3;
const RESPONSE_FOLD_BUTTON_WIDTH: f32 = 54.;
const RESPONSE_COPY_BUTTON_WIDTH: f32 = 54.;
const CODEGEN_COPY_BUTTON_WIDTH: f32 = 72.;
const CODEGEN_MENU_WIDTH: f32 = 156.;
const CODEGEN_SNIPPET_HEIGHT: f32 = 180.;
const CODEGEN_SNIPPET_LINE_HEIGHT: f32 = 26.;
const STATUS_BAR_TRAILING_MAX_WIDTH: f32 = 220.;
const BODY_EDITOR_HEIGHT: f32 = 118.;
const GRAPHQL_VARIABLES_EDITOR_HEIGHT: f32 = 86.;
const BODY_PREVIEW_HEIGHT: f32 = 86.;
const BODY_PREVIEW_LINE_HEIGHT: f32 = 26.;
const RESPONSE_BODY_LINE_HEIGHT: f32 = 26.;
const GRAPHQL_QUERY_TEMPLATE_USE_BUTTON_WIDTH: f32 = 54.;
const GRAPHQL_SCHEMA_BROWSER_HEIGHT: f32 = 112.;
const RESULT_ROW_HEIGHT: f32 = 34.;
const COLLECTION_DRAG_PREVIEW_HEIGHT: f32 = 28.;
const COLLECTION_DRAG_PREVIEW_MAX_WIDTH: f32 = 220.;
const PANEL_HEADER_HEIGHT: f32 = 40.;
const PANEL_HEADER_META_MAX_WIDTH: f32 = 180.;
const PANEL_HEADER_RIGHT_PADDING: f32 = 14.;
const PANEL_HEADER_UNDERLINE_WIDTH: f32 = 80.;
const PANEL_HEADER_UNDERLINE_HEIGHT: f32 = 2.;
const PANEL_HEADER_UNDERLINE_LEFT_OFFSET: f32 = 12.;
const EMPTY_STATE_ROW_HEIGHT: f32 = 36.;
const APP_BASE_TEXT_SIZE: f32 = 14.;
const TOP_BAR_BRAND_TEXT_SIZE: f32 = 16.;
const TOP_BAR_ACTION_TEXT_SIZE: f32 = 14.;
const ACTION_BUTTON_TEXT_SIZE: f32 = 16.;
const COMPACT_CONTROL_TEXT_SIZE: f32 = 15.;
const COMPACT_SYMBOL_TEXT_SIZE: f32 = 13.;
const METHOD_CHEVRON_TEXT_SIZE: f32 = 12.;
const PANE_HEADER_TITLE_TEXT_SIZE: f32 = 19.;
const SIDEBAR_NAV_TEXT_SIZE: f32 = 16.;
const SIDEBAR_ACTION_TEXT_SIZE: f32 = 15.;
const SIDEBAR_PRIMARY_ROW_TEXT_SIZE: f32 = 17.;
const SIDEBAR_METHOD_TEXT_SIZE: f32 = 16.;
const SIDEBAR_COMPACT_METHOD_TEXT_SIZE: f32 = 14.;
const ROW_META_TEXT_SIZE: f32 = 15.;
const REQUEST_PRIMARY_CONTROL_TEXT_SIZE: f32 = 18.;
const REQUEST_EDITOR_TAB_TEXT_SIZE: f32 = 16.;
const PANEL_TITLE_TEXT_SIZE: f32 = 18.;
const PANEL_CONTENT_TEXT_SIZE: f32 = 18.;
const PANEL_META_TEXT_SIZE: f32 = 15.;
const TABLE_HEADER_TEXT_SIZE: f32 = 16.;
const TEXT_INPUT_TEXT_SIZE: f32 = 18.;
const RESPONSE_BODY_TEXT_SIZE: f32 = 18.;
const UI_RADIUS_TIGHT: f32 = 4.;
const UI_RADIUS_CONTROL: f32 = 5.;
const UI_RADIUS_INPUT: f32 = 6.;
const TEXT_INPUT_HEIGHT: f32 = 40.;
const TEXT_INPUT_LINE_HEIGHT: f32 = 24.;
const TEXT_INPUT_RADIUS: f32 = UI_RADIUS_INPUT;
const TEXT_INPUT_BORDER_WIDTH: f32 = 2.;
const KEY_VALUE_KEY_COLUMN_WIDTH: f32 = 150.;
const KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH: f32 = 128.;
const KEY_VALUE_EDITOR_COMPACT_KEY_COLUMN_WIDTH: f32 = 112.;
const KEY_VALUE_ROW_ACTION_BUTTON_WIDTH: f32 = 28.;
const TEST_ASSERTION_NAME_COLUMN_WIDTH: f32 = 132.;
const TEST_ASSERTION_KIND_COLUMN_WIDTH: f32 = 96.;
const TESTS_CLEAR_RESULTS_BUTTON_WIDTH: f32 = 54.;
const COLLECTION_TREE_ROOT_ROW_HEIGHT: f32 = 32.;
const COLLECTION_TREE_FOLDER_ROW_HEIGHT: f32 = 32.;
const COLLECTION_TREE_REQUEST_ROW_HEIGHT: f32 = 40.;
const COLLECTION_TREE_INDENT_BASE: f32 = 8.;
const COLLECTION_TREE_INDENT_STEP: f32 = 14.;
const COLLECTION_TREE_INDENT_MAX: f32 = 78.;
const COLLECTION_TREE_MARKER_WIDTH: f32 = 14.;
const HTTP_METHOD_LABEL_WIDTH: f32 = 64.;
const SIDEBAR_SECONDARY_ROW_INDENT: f32 = HTTP_METHOD_LABEL_WIDTH + 8.;
const RUNNER_METHOD_COLUMN_WIDTH: f32 = 70.;
const RUNNER_STATUS_COLUMN_WIDTH: f32 = 42.;
const RUNNER_PRE_REQUEST_COLUMN_WIDTH: f32 = 52.;
const RUNNER_TESTS_COLUMN_WIDTH: f32 = 64.;
const TEST_RESULT_STATUS_COLUMN_WIDTH: f32 = 48.;
const TEST_RESULT_NAME_COLUMN_WIDTH: f32 = 140.;
const WEBSOCKET_DIRECTION_COLUMN_WIDTH: f32 = 52.;
const WEBSOCKET_KIND_COLUMN_WIDTH: f32 = 52.;
const SSE_EVENT_COLUMN_WIDTH: f32 = 74.;
const SSE_ID_COLUMN_WIDTH: f32 = 58.;
const GRAPHQL_SCHEMA_FIELD_LIMIT: usize = 12;
const GRAPHQL_SCHEMA_TYPE_LIMIT: usize = 18;
const GRAPHQL_QUERY_TEMPLATE_LIMIT: usize = 5;
const WEBSOCKET_LOG_LIMIT: usize = 24;
const SSE_EVENT_FETCH_LIMIT: usize = 6;
const SSE_LOG_LIMIT: usize = 24;
const MOCK_SERVER_PORT: u16 = 8080;
const HTTP_METHODS: [&str; 7] = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "HEAD"];
const GRAPHQL_INTROSPECTION_QUERY: &str = "query IntrospectionQuery { __schema { queryType { name } mutationType { name } subscriptionType { name } types { kind name description fields(includeDeprecated: true) { name description args { name description type { kind name ofType { kind name ofType { kind name } } } defaultValue } type { kind name ofType { kind name ofType { kind name } } } isDeprecated deprecationReason } inputFields { name description type { kind name ofType { kind name ofType { kind name } } } defaultValue } interfaces { kind name ofType { kind name } } enumValues(includeDeprecated: true) { name description isDeprecated deprecationReason } possibleTypes { kind name ofType { kind name } } } directives { name description locations args { name description type { kind name ofType { kind name ofType { kind name } } } defaultValue } } } }";

pub fn run() -> Result<()> {
    let runtime = Arc::new(Runtime::new()?);

    gpui_platform::application().run(move |cx: &mut App| {
        bind_app_keys(cx);
        bind_text_input_keys(cx);
        bind_read_only_text_keys(cx);

        let bounds = Bounds::centered(None, size(px(APP_WINDOW_WIDTH), px(APP_WINDOW_HEIGHT)), cx);
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

fn bind_app_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-enter", SendCurrentRequest, None),
        KeyBinding::new("cmd-enter", SendCurrentRequest, None),
        KeyBinding::new("ctrl-f", FocusActiveSidebarInput, None),
        KeyBinding::new("cmd-f", FocusActiveSidebarInput, None),
        KeyBinding::new("ctrl-l", FocusRequestUrl, None),
        KeyBinding::new("cmd-l", FocusRequestUrl, None),
        KeyBinding::new("ctrl-s", SaveCurrentRequest, None),
        KeyBinding::new("cmd-s", SaveCurrentRequest, None),
        KeyBinding::new("ctrl-1", SelectRequestTabParams, None),
        KeyBinding::new("cmd-1", SelectRequestTabParams, None),
        KeyBinding::new("ctrl-2", SelectRequestTabHeaders, None),
        KeyBinding::new("cmd-2", SelectRequestTabHeaders, None),
        KeyBinding::new("ctrl-3", SelectRequestTabAuth, None),
        KeyBinding::new("cmd-3", SelectRequestTabAuth, None),
        KeyBinding::new("ctrl-4", SelectRequestTabBody, None),
        KeyBinding::new("cmd-4", SelectRequestTabBody, None),
        KeyBinding::new("ctrl-5", SelectRequestTabScripts, None),
        KeyBinding::new("cmd-5", SelectRequestTabScripts, None),
        KeyBinding::new("ctrl-6", SelectRequestTabRealtime, None),
        KeyBinding::new("cmd-6", SelectRequestTabRealtime, None),
        KeyBinding::new("ctrl-7", SelectRequestTabTools, None),
        KeyBinding::new("cmd-7", SelectRequestTabTools, None),
        KeyBinding::new("ctrl-shift-1", SelectResponseTabPretty, None),
        KeyBinding::new("cmd-shift-1", SelectResponseTabPretty, None),
        KeyBinding::new("ctrl-shift-2", SelectResponseTabRaw, None),
        KeyBinding::new("cmd-shift-2", SelectResponseTabRaw, None),
        KeyBinding::new("ctrl-shift-3", SelectResponseTabHeaders, None),
        KeyBinding::new("cmd-shift-3", SelectResponseTabHeaders, None),
        KeyBinding::new("escape", CloseTransientUi, None),
    ]);
}

struct ZenApiApp {
    runtime: Arc<Runtime>,
    workspace_sidebar_ratio: f32,
    workspace_request_ratio: f32,
    workspace_split_pending: Option<WorkspaceSplitPreview>,
    active_sidebar_section: SidebarSection,
    active_request_tab: RequestPaneTab,
    sidebar_scroll: ScrollHandle,
    request_scroll: ScrollHandle,
    response_scroll: ScrollHandle,
    scrollbar_drag: Option<ScrollbarDragState>,
    import_path: Entity<TextInput>,
    collection_path: Entity<TextInput>,
    collection_rename_input: Entity<TextInput>,
    route_filter: Entity<TextInput>,
    history_filter: Entity<TextInput>,
    url: Entity<TextInput>,
    import_popover_open: bool,
    method_menu_open: bool,
    environment_name_input: Entity<TextInput>,
    active_environment: Option<String>,
    global_variables: Vec<KeyValueRow>,
    environments: Vec<EnvironmentConfig>,
    query_params: Vec<KeyValueRow>,
    request_headers: Vec<KeyValueRow>,
    auth_mode: AuthMode,
    bearer_token: Entity<TextInput>,
    oauth2_access_token: Entity<TextInput>,
    basic_username: Entity<TextInput>,
    basic_password: Entity<TextInput>,
    jwt_token: Entity<TextInput>,
    api_key_name: Entity<TextInput>,
    api_key_value: Entity<TextInput>,
    api_key_placement: ApiKeyPlacement,
    pre_request_script: Entity<TextInput>,
    pre_request_status: String,
    last_pre_request_actions: Vec<String>,
    request_body_mode: RequestBodyMode,
    raw_body_format: RawBodyFormat,
    request_body: Entity<TextInput>,
    graphql_query: Entity<TextInput>,
    graphql_variables: Entity<TextInput>,
    graphql_schema_summary: String,
    graphql_schema_browser: String,
    graphql_query_templates: Vec<GraphqlQueryTemplate>,
    websocket_url: Entity<TextInput>,
    websocket_protocols: Entity<TextInput>,
    websocket_headers: Vec<KeyValueRow>,
    websocket_message: Entity<TextInput>,
    websocket_message_mode: WebSocketMessageMode,
    websocket_session_url: String,
    websocket_status: String,
    websocket_running: bool,
    websocket_command_tx: Option<mpsc::UnboundedSender<client::WebSocketSessionCommand>>,
    websocket_messages: Vec<WebSocketLogEntry>,
    sse_url: Entity<TextInput>,
    sse_headers: Vec<KeyValueRow>,
    sse_session_url: String,
    sse_status: String,
    sse_running: bool,
    sse_subscription: Option<JoinHandle<()>>,
    sse_last_event_id: Option<String>,
    sse_events: Vec<SseLogEntry>,
    form_data_body: Vec<KeyValueRow>,
    urlencoded_body: Vec<KeyValueRow>,
    binary_body_path: Entity<TextInput>,
    request_assertions: Vec<TestAssertionRow>,
    last_assertion_results: Vec<ResponseAssertionResult>,
    routes: Vec<ApiRoute>,
    visible_routes: Vec<ApiRoute>,
    selected_route: Option<usize>,
    collection: ApiCollection,
    expanded_collection_nodes: Vec<String>,
    collection_status: String,
    collection_context_menu: Option<CollectionContextMenu>,
    method: String,
    spec_label: String,
    response_status: String,
    response_meta: String,
    response_tone: ResponseTone,
    response_body: String,
    response_raw_body: String,
    response_headers: String,
    response_view: ResponseView,
    response_pretty_collapsed: bool,
    response_body_viewer: Entity<ReadOnlyTextView>,
    codegen_language: SnippetLanguage,
    codegen_menu_open: bool,
    server: Option<MockServer>,
    server_running: bool,
    server_status: String,
    mock_logs: Vec<MockRequestLog>,
    runner_running: bool,
    runner_stop_on_failure: bool,
    runner_status: String,
    runner_results: Vec<CollectionRunResult>,
    history: RequestHistory,
    history_query: String,
    busy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum WorkspaceSplitDrag {
    SidebarRequest,
    RequestResponse,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WorkspaceSplitPreview {
    split: WorkspaceSplitDrag,
    sidebar_ratio: f32,
    request_ratio: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScrollbarKind {
    Sidebar,
    Request,
    Response,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarDragState {
    kind: ScrollbarKind,
    track_top: f32,
    thumb_grab_y: f32,
    max_thumb_top: f32,
    max_offset: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TransientUiState {
    import_popover_open: bool,
    method_menu_open: bool,
    codegen_menu_open: bool,
    collection_menu_open: bool,
}

#[derive(Clone, Copy, Debug)]
struct ScrollbarMetrics {
    max_offset: f32,
    thumb_top: f32,
    thumb_height: f32,
    max_thumb_top: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarSection {
    Endpoints,
    Collections,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarFocusTarget {
    RouteFilter,
    CollectionPath,
    HistoryFilter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestPaneTab {
    Params,
    Headers,
    Auth,
    Body,
    Scripts,
    Realtime,
    Tools,
}

fn request_editor_tabs() -> [(&'static str, RequestPaneTab); REQUEST_EDITOR_TAB_COUNT] {
    [
        ("Params", RequestPaneTab::Params),
        ("Hdrs", RequestPaneTab::Headers),
        ("Auth", RequestPaneTab::Auth),
        ("Body", RequestPaneTab::Body),
        ("Script", RequestPaneTab::Scripts),
        ("Live", RequestPaneTab::Realtime),
        ("Tools", RequestPaneTab::Tools),
    ]
}

#[cfg(test)]
fn request_tab_shortcuts() -> [(usize, RequestPaneTab); REQUEST_EDITOR_TAB_COUNT] {
    [
        (1, RequestPaneTab::Params),
        (2, RequestPaneTab::Headers),
        (3, RequestPaneTab::Auth),
        (4, RequestPaneTab::Body),
        (5, RequestPaneTab::Scripts),
        (6, RequestPaneTab::Realtime),
        (7, RequestPaneTab::Tools),
    ]
}

struct KeyValueRow {
    key: Entity<TextInput>,
    value: Entity<TextInput>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyValueEditorTarget {
    QueryParams,
    RequestHeaders,
    GlobalVariables,
    ActiveEnvironmentVariables,
    FormDataBody,
    UrlEncodedBody,
    WebSocketHeaders,
    SseHeaders,
}

struct TestAssertionRow {
    name: Entity<TextInput>,
    kind: TestAssertionKind,
    target: Entity<TextInput>,
    expected: Entity<TextInput>,
}

struct EnvironmentConfig {
    name: String,
    variables: Vec<KeyValueRow>,
}

#[derive(Clone)]
struct CollectionContextMenu {
    node_id: String,
    label: String,
    kind: CollectionNodeKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CollectionNodeKind {
    Root,
    Folder,
    Request,
}

#[derive(Clone)]
struct DraggedCollectionNode {
    node_id: String,
    label: String,
}

struct CollectionDragPreview {
    label: String,
}

struct WorkspaceSplitDragPreview;

struct RequestBuild {
    request: CodegenRequest,
    pre_request_actions: usize,
    pre_request_action_labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GraphqlQueryTemplate {
    field_name: String,
    operation: String,
    variables: String,
}

struct GraphqlOperationArg {
    name: String,
    type_ref: String,
    default_value: Option<String>,
    placeholder: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WebSocketLogEntry {
    direction: WebSocketDirection,
    kind: String,
    data: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebSocketDirection {
    Sent,
    Received,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WebSocketMessageMode {
    Text,
    BinaryHex,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SseLogEntry {
    event: String,
    data: String,
    id: Option<String>,
}

fn ui_surface() -> Hsla {
    rgb(UI_COLOR_SURFACE).into()
}

fn ui_surface_muted() -> Hsla {
    rgb(UI_COLOR_SURFACE_MUTED).into()
}

fn ui_app_chrome() -> Hsla {
    rgb(UI_COLOR_APP_CHROME).into()
}

fn ui_workspace_gutter() -> Hsla {
    rgb(UI_COLOR_WORKSPACE_GUTTER).into()
}

fn ui_sidebar_pane() -> Hsla {
    rgb(UI_COLOR_SIDEBAR_PANE).into()
}

fn ui_request_pane() -> Hsla {
    rgb(UI_COLOR_REQUEST_PANE).into()
}

fn ui_request_tab_bar() -> Hsla {
    rgb(UI_COLOR_REQUEST_TAB_BAR).into()
}

fn ui_response_pane() -> Hsla {
    rgb(UI_COLOR_RESPONSE_PANE).into()
}

fn ui_response_tab_bar() -> Hsla {
    rgb(UI_COLOR_RESPONSE_TAB_BAR).into()
}

fn ui_hover() -> Hsla {
    rgb(UI_COLOR_HOVER).into()
}

fn ui_disabled_surface() -> Hsla {
    rgb(UI_COLOR_DISABLED_SURFACE).into()
}

fn ui_disabled_border() -> Hsla {
    rgb(UI_COLOR_DISABLED_BORDER).into()
}

fn ui_disabled_text() -> Hsla {
    rgb(UI_COLOR_DISABLED_TEXT).into()
}

fn ui_border() -> Hsla {
    rgb(UI_COLOR_BORDER).into()
}

fn ui_border_strong() -> Hsla {
    rgb(UI_COLOR_BORDER_STRONG).into()
}

fn ui_text_primary() -> Hsla {
    rgb(UI_COLOR_TEXT_PRIMARY).into()
}

fn ui_text_secondary() -> Hsla {
    rgb(UI_COLOR_TEXT_SECONDARY).into()
}

fn ui_text_muted() -> Hsla {
    rgb(UI_COLOR_TEXT_MUTED).into()
}

fn ui_text_placeholder() -> Hsla {
    rgb(UI_COLOR_TEXT_PLACEHOLDER).into()
}

fn ui_text_body() -> Hsla {
    rgb(UI_COLOR_TEXT_BODY).into()
}

fn ui_sidebar_detail_text() -> Hsla {
    rgb(UI_COLOR_SIDEBAR_DETAIL_TEXT).into()
}

fn ui_accent() -> Hsla {
    rgb(UI_COLOR_ACCENT).into()
}

fn ui_accent_text() -> Hsla {
    rgb(UI_COLOR_ACCENT_TEXT).into()
}

fn ui_warning_strong() -> Hsla {
    rgb(UI_COLOR_WARNING_STRONG).into()
}

fn ui_status_busy() -> Hsla {
    rgb(UI_COLOR_STATUS_BUSY).into()
}

fn ui_status_success() -> Hsla {
    rgb(UI_COLOR_STATUS_SUCCESS).into()
}

fn ui_status_error() -> Hsla {
    rgb(UI_COLOR_STATUS_ERROR).into()
}

fn control_opacity(enabled: bool) -> f32 {
    if enabled {
        1.0
    } else {
        DISABLED_CONTROL_OPACITY
    }
}

fn can_activate_render_enabled_control(render_enabled: bool, busy: bool) -> bool {
    render_enabled && !busy
}

impl ZenApiApp {
    fn new(runtime: Arc<Runtime>, cx: &mut Context<Self>) -> Self {
        let import_path = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_IMPORT_PATH, true));
        let collection_path = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_COLLECTION_PATH, true));
        let collection_rename_input =
            cx.new(|cx| TextInput::new(cx, PLACEHOLDER_COLLECTION_ITEM, true));
        let route_filter = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_ROUTE_FILTER, false));
        let history_filter = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_HISTORY_FILTER, false));
        let url = cx.new(|cx| {
            TextInput::new(cx, PLACEHOLDER_REQUEST_URL, true).with_chrome(TextInputChrome::Inline)
        });
        let environment_name_input =
            cx.new(|cx| TextInput::new(cx, PLACEHOLDER_ENVIRONMENT_NAME, true));
        let global_variables = key_value_rows(
            cx,
            &[
                ("baseUrl", "https://api.example.com"),
                ("token", "secret"),
                ("", ""),
            ],
        );
        let environments = vec![
            environment_config(
                cx,
                "dev",
                &[
                    ("baseUrl", "http://localhost:8080"),
                    ("token", "dev-token"),
                    ("", ""),
                ],
            ),
            environment_config(
                cx,
                "test",
                &[
                    ("baseUrl", "https://test.example.com"),
                    ("token", "test-token"),
                    ("", ""),
                ],
            ),
            environment_config(
                cx,
                "prod",
                &[
                    ("baseUrl", "https://api.example.com"),
                    ("token", "prod-token"),
                    ("", ""),
                ],
            ),
        ];
        let query_params = key_value_rows(
            cx,
            &[("page", "1"), ("limit", "20"), ("search", "term"), ("", "")],
        );
        let request_headers = key_value_rows(
            cx,
            &[
                ("Accept", "application/json"),
                ("Authorization", "Bearer token"),
                ("X-Request-Id", "request-id"),
                ("", ""),
            ],
        );
        let bearer_token = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_BEARER_TOKEN, true));
        let oauth2_access_token =
            cx.new(|cx| TextInput::new(cx, PLACEHOLDER_OAUTH2_ACCESS_TOKEN, true));
        let basic_username = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_BASIC_USERNAME, true));
        let basic_password = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_BASIC_PASSWORD, true));
        let jwt_token = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_JWT_TOKEN, true));
        let api_key_name = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_API_KEY_NAME, true));
        let api_key_value = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_API_KEY_VALUE, true));
        let pre_request_script = cx.new(|cx| TextInput::new(cx, "Action", true));
        let request_body = cx.new(|cx| {
            TextInput::new(cx, PLACEHOLDER_REQUEST_BODY, true).with_multiline(BODY_EDITOR_HEIGHT)
        });
        let graphql_query = cx.new(|cx| {
            TextInput::new(cx, PLACEHOLDER_GRAPHQL_QUERY, true).with_multiline(BODY_EDITOR_HEIGHT)
        });
        let graphql_variables = cx.new(|cx| {
            TextInput::new(cx, PLACEHOLDER_GRAPHQL_VARIABLES, true)
                .with_multiline(GRAPHQL_VARIABLES_EDITOR_HEIGHT)
        });
        let websocket_url = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_WEBSOCKET_URL, true));
        let websocket_protocols =
            cx.new(|cx| TextInput::new(cx, PLACEHOLDER_WEBSOCKET_PROTOCOLS, true));
        let websocket_headers = key_value_rows(cx, &[("X-Token", "token"), ("", "")]);
        let websocket_message = cx.new(|cx| TextInput::new(cx, "Message", true));
        let sse_url = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_SSE_URL, true));
        let sse_headers = key_value_rows(cx, &[("Authorization", "Bearer token"), ("", "")]);
        let response_body_viewer = cx.new(|cx| ReadOnlyTextView::new(cx, INITIAL_RESPONSE_BODY));
        let form_data_body = key_value_rows(
            cx,
            &[
                ("field", "value"),
                ("file", "@/path/to/file"),
                ("", ""),
                ("", ""),
            ],
        );
        let urlencoded_body = key_value_rows(
            cx,
            &[
                ("username", "dev"),
                ("password", "secret"),
                ("", ""),
                ("", ""),
            ],
        );
        let binary_body_path = cx.new(|cx| TextInput::new(cx, PLACEHOLDER_BINARY_BODY_PATH, true));
        let request_assertions = assertion_rows_from_assertions(cx, &[]);

        cx.subscribe(&import_path, |app, _input, _event: &TextAccepted, cx| {
            if can_submit_path_action(app.busy, &app.import_path.read(cx).text()) {
                app.import_openapi(cx);
            }
        })
        .detach();

        cx.subscribe(
            &collection_path,
            |app, _input, _event: &TextAccepted, cx| {
                if can_submit_path_action(app.busy, &app.collection_path.read(cx).text()) {
                    app.import_collection(cx);
                }
            },
        )
        .detach();

        cx.subscribe(
            &collection_rename_input,
            |app, _input, _event: &TextAccepted, cx| {
                let kind = app.collection_context_menu.as_ref().map(|menu| menu.kind);
                if can_submit_collection_rename(
                    app.busy,
                    kind,
                    &app.collection_rename_input.read(cx).text(),
                ) {
                    app.rename_collection_target(cx);
                }
            },
        )
        .detach();

        cx.subscribe(&route_filter, |app, _input, event: &TextChanged, cx| {
            app.apply_route_filter(&event.text);
            cx.notify();
        })
        .detach();

        cx.subscribe(&history_filter, |app, _input, event: &TextChanged, cx| {
            app.history_query = event.text.clone();
            cx.notify();
        })
        .detach();

        cx.subscribe(&url, |app, _input, _event: &TextAccepted, cx| {
            if can_send_request_shortcut(
                app.busy,
                &app.url.read(cx).text(),
                &app.pre_request_script.read(cx).text(),
            ) {
                app.send_request(cx);
            }
        })
        .detach();

        cx.subscribe(
            &environment_name_input,
            |app, _input, _event: &TextAccepted, cx| {
                if can_submit_environment_add(app.busy, &app.environment_name_input.read(cx).text())
                {
                    app.add_environment(cx);
                }
            },
        )
        .detach();

        Self {
            runtime,
            workspace_sidebar_ratio: WORKSPACE_SIDEBAR_DEFAULT_RATIO,
            workspace_request_ratio: WORKSPACE_REQUEST_DEFAULT_RATIO,
            workspace_split_pending: None,
            active_sidebar_section: SidebarSection::Endpoints,
            active_request_tab: RequestPaneTab::Params,
            sidebar_scroll: ScrollHandle::new(),
            request_scroll: ScrollHandle::new(),
            response_scroll: ScrollHandle::new(),
            scrollbar_drag: None,
            import_path,
            collection_path,
            collection_rename_input,
            route_filter,
            history_filter,
            url,
            import_popover_open: false,
            method_menu_open: false,
            environment_name_input,
            active_environment: None,
            global_variables,
            environments,
            query_params,
            request_headers,
            auth_mode: AuthMode::None,
            bearer_token,
            oauth2_access_token,
            basic_username,
            basic_password,
            jwt_token,
            api_key_name,
            api_key_value,
            api_key_placement: ApiKeyPlacement::Header,
            pre_request_script,
            pre_request_status: "idle".to_string(),
            last_pre_request_actions: Vec::new(),
            request_body_mode: RequestBodyMode::None,
            raw_body_format: RawBodyFormat::Json,
            request_body,
            graphql_query,
            graphql_variables,
            graphql_schema_summary: String::new(),
            graphql_schema_browser: String::new(),
            graphql_query_templates: Vec::new(),
            websocket_url,
            websocket_protocols,
            websocket_headers,
            websocket_message,
            websocket_message_mode: WebSocketMessageMode::Text,
            websocket_session_url: String::new(),
            websocket_status: "idle".to_string(),
            websocket_running: false,
            websocket_command_tx: None,
            websocket_messages: Vec::new(),
            sse_url,
            sse_headers,
            sse_session_url: String::new(),
            sse_status: "idle".to_string(),
            sse_running: false,
            sse_subscription: None,
            sse_last_event_id: None,
            sse_events: Vec::new(),
            form_data_body,
            urlencoded_body,
            binary_body_path,
            request_assertions,
            last_assertion_results: Vec::new(),
            routes: Vec::new(),
            visible_routes: Vec::new(),
            selected_route: None,
            collection: ApiCollection::new("ZenAPI Collection"),
            expanded_collection_nodes: vec!["collection".to_string()],
            collection_status: "No collection file".to_string(),
            collection_context_menu: None,
            method: "GET".to_string(),
            spec_label: "No spec loaded".to_string(),
            response_status: "Idle".to_string(),
            response_meta: String::new(),
            response_tone: ResponseTone::Neutral,
            response_body: INITIAL_RESPONSE_BODY.to_string(),
            response_raw_body: INITIAL_RESPONSE_BODY.to_string(),
            response_headers: String::new(),
            response_view: ResponseView::Pretty,
            response_pretty_collapsed: false,
            response_body_viewer,
            codegen_language: SnippetLanguage::Curl,
            codegen_menu_open: false,
            server: None,
            server_running: false,
            server_status: MOCK_STATUS_STOPPED.to_string(),
            mock_logs: Vec::new(),
            runner_running: false,
            runner_stop_on_failure: false,
            runner_status: "Runner idle".to_string(),
            runner_results: Vec::new(),
            history: RequestHistory::new(),
            history_query: String::new(),
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
                IMPORT_PATH_REQUIRED_TITLE,
                "",
                ResponseTone::Error,
                PATH_REQUIRED_BODY,
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
                let spec_label = display_spec_label(path);
                self.spec_label = spec_label.clone();
                self.import_popover_open = false;
                self.server_running = false;
                self.server_status = if self.routes.is_empty() {
                    MOCK_STATUS_NO_ROUTES.to_string()
                } else {
                    MOCK_STATUS_READY.to_string()
                };
                self.route_filter
                    .update(cx, |input, cx| input.set_text("", cx));
                self.set_response(
                    format!("Imported {spec_name}"),
                    route_count_label(self.routes.len()),
                    ResponseTone::Success,
                    import_success_body(&spec_label),
                );
            }
            Err(error) => {
                self.set_response(
                    RESPONSE_TITLE_IMPORT_FAIL,
                    "",
                    ResponseTone::Error,
                    file_operation_error("OpenAPI import failed.", path, &error.to_string()),
                );
            }
        }

        cx.notify();
    }

    fn import_collection(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let path = self.collection_path.read(cx).text();
        let path = path.trim();
        if path.is_empty() {
            self.set_response(
                COLLECTION_PATH_REQUIRED_TITLE,
                "",
                ResponseTone::Error,
                PATH_REQUIRED_BODY,
            );
            cx.notify();
            return;
        }

        match ApiCollection::load_file(path) {
            Ok(collection) => {
                self.collection_status = format!("Imported {}", collection.name);
                self.collection = collection;
                self.expanded_collection_nodes = vec!["collection".to_string()];
                self.set_response(
                    RESPONSE_TITLE_IMPORTED,
                    self.collection.items.len().to_string(),
                    ResponseTone::Success,
                    self.collection.name.clone(),
                );
            }
            Err(error) => {
                self.collection_status = "Import failed".to_string();
                self.set_response(
                    RESPONSE_TITLE_IMPORT_FAIL,
                    "",
                    ResponseTone::Error,
                    file_operation_error("Collection import failed.", path, &error.to_string()),
                );
            }
        }
        cx.notify();
    }

    fn export_collection(&mut self, postman: bool, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if !can_export_collection(&self.collection) {
            self.collection_status = "Nothing to export".to_string();
            self.set_response(
                COLLECTION_EXPORT_EMPTY_TITLE,
                "",
                ResponseTone::Neutral,
                SAVED_REQUESTS_EMPTY_BODY,
            );
            cx.notify();
            return;
        }

        let path = self.collection_path.read(cx).text();
        let path = path.trim();
        if path.is_empty() {
            self.set_response(
                EXPORT_PATH_REQUIRED_TITLE,
                "",
                ResponseTone::Error,
                PATH_REQUIRED_BODY,
            );
            cx.notify();
            return;
        }

        let result = if postman {
            self.collection.save_postman_file(path)
        } else {
            self.collection.save_file(path)
        };

        match result {
            Ok(()) => {
                self.collection_status = if postman {
                    "Exported Postman".to_string()
                } else {
                    "Exported ZenAPI".to_string()
                };
                self.set_response(
                    RESPONSE_TITLE_EXPORTED,
                    "",
                    ResponseTone::Success,
                    path.to_string(),
                );
            }
            Err(error) => {
                self.collection_status = "Export failed".to_string();
                self.set_response(
                    RESPONSE_TITLE_EXPORT_FAIL,
                    "",
                    ResponseTone::Error,
                    file_operation_error("Collection export failed.", path, &error.to_string()),
                );
            }
        }
        cx.notify();
    }

    fn save_current_request_to_collection(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        let raw_request = self.current_raw_codegen_request(cx);
        match self.current_request_build(cx) {
            Ok(build) if build.request.url.is_empty() => {
                self.pre_request_status = pre_request_status_label(build.pre_request_actions);
                self.last_pre_request_actions = build.pre_request_action_labels;
                self.set_response(
                    SAVE_URL_REQUIRED_TITLE,
                    "",
                    ResponseTone::Error,
                    URL_REQUIRED_BODY,
                );
                cx.notify();
                return;
            }
            Ok(build) => {
                self.pre_request_status = pre_request_status_label(build.pre_request_actions);
                self.last_pre_request_actions = build.pre_request_action_labels;
            }
            Err(error) => {
                let error = error.to_string();
                self.pre_request_status = pre_request_error_label(&error);
                self.last_pre_request_actions.clear();
                self.set_response(
                    RESPONSE_TITLE_SAVE_FAIL,
                    "",
                    ResponseTone::Error,
                    editor_error("Save build failed.", &error),
                );
                cx.notify();
                return;
            }
        }
        let tests = match self.current_response_assertions(cx) {
            Ok(tests) => tests,
            Err(error) => {
                self.set_response(
                    RESPONSE_TITLE_SAVE_FAIL,
                    "",
                    ResponseTone::Error,
                    editor_error("Tests save failed.", &error.to_string()),
                );
                cx.notify();
                return;
            }
        };

        let collection_request = collection_request_for_save(
            &raw_request,
            self.pre_request_script.read(cx).text(),
            tests,
        );
        self.collection
            .items
            .push(CollectionItem::Request(collection_request));
        self.collection_status = format!("{} items", self.collection.items.len());
        self.set_response(
            RESPONSE_TITLE_SAVED,
            self.collection.name.clone(),
            ResponseTone::Success,
            RESPONSE_BODY_REQUEST,
        );
        cx.notify();
    }

    fn save_current_request_shortcut(
        &mut self,
        _: &SaveCurrentRequest,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if can_save_current_request_shortcut(
            self.busy,
            &self.url.read(cx).text(),
            &self.pre_request_script.read(cx).text(),
        ) {
            self.save_current_request_to_collection(cx);
        }
    }

    fn send_current_request_shortcut(
        &mut self,
        _: &SendCurrentRequest,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if can_send_request_shortcut(
            self.busy,
            &self.url.read(cx).text(),
            &self.pre_request_script.read(cx).text(),
        ) {
            self.send_request(cx);
        }
    }

    fn transient_ui_state(&self) -> TransientUiState {
        TransientUiState {
            import_popover_open: self.import_popover_open,
            method_menu_open: self.method_menu_open,
            codegen_menu_open: self.codegen_menu_open,
            collection_menu_open: self.collection_context_menu.is_some(),
        }
    }

    fn apply_transient_ui_state(&mut self, state: TransientUiState) {
        self.import_popover_open = state.import_popover_open;
        self.method_menu_open = state.method_menu_open;
        self.codegen_menu_open = state.codegen_menu_open;
        if !state.collection_menu_open {
            self.collection_context_menu = None;
        }
    }

    fn close_transient_layers(&mut self) -> bool {
        let mut state = self.transient_ui_state();
        if close_transient_ui_state(&mut state) {
            self.apply_transient_ui_state(state);
            true
        } else {
            false
        }
    }

    fn close_transient_ui(&mut self, _: &CloseTransientUi, _: &mut Window, cx: &mut Context<Self>) {
        if self.close_transient_layers() {
            cx.notify();
        }
    }

    fn select_request_tab(&mut self, tab: RequestPaneTab, cx: &mut Context<Self>) {
        self.close_transient_layers();
        self.active_request_tab = tab;
        reset_scroll_handle(&self.request_scroll);
        cx.notify();
    }

    fn select_request_tab_params(
        &mut self,
        _: &SelectRequestTabParams,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Params, cx);
    }

    fn select_request_tab_headers(
        &mut self,
        _: &SelectRequestTabHeaders,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Headers, cx);
    }

    fn select_request_tab_auth(
        &mut self,
        _: &SelectRequestTabAuth,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Auth, cx);
    }

    fn select_request_tab_body(
        &mut self,
        _: &SelectRequestTabBody,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Body, cx);
    }

    fn select_request_tab_scripts(
        &mut self,
        _: &SelectRequestTabScripts,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Scripts, cx);
    }

    fn select_request_tab_realtime(
        &mut self,
        _: &SelectRequestTabRealtime,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Realtime, cx);
    }

    fn select_request_tab_tools(
        &mut self,
        _: &SelectRequestTabTools,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_request_tab(RequestPaneTab::Tools, cx);
    }

    fn select_response_tab(&mut self, view: ResponseView, cx: &mut Context<Self>) {
        self.close_transient_layers();
        self.response_view = view;
        reset_scroll_handle(&self.response_scroll);
        cx.notify();
    }

    fn select_response_tab_pretty(
        &mut self,
        _: &SelectResponseTabPretty,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_response_tab(ResponseView::Pretty, cx);
    }

    fn select_response_tab_raw(
        &mut self,
        _: &SelectResponseTabRaw,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_response_tab(ResponseView::Raw, cx);
    }

    fn select_response_tab_headers(
        &mut self,
        _: &SelectResponseTabHeaders,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_response_tab(ResponseView::Headers, cx);
    }

    fn set_busy(&mut self, busy: bool, cx: &mut Context<Self>) {
        if self.busy == busy {
            self.sync_busy_sensitive_inputs_enabled(cx);
            return;
        }

        self.busy = busy;
        if busy {
            self.close_transient_layers();
        }
        self.sync_busy_sensitive_inputs_enabled(cx);
    }

    fn sync_busy_sensitive_inputs_enabled(&mut self, cx: &mut Context<Self>) {
        let enabled = can_edit_request_configuration(self.busy);
        for input in [
            &self.import_path,
            &self.collection_path,
            &self.collection_rename_input,
            &self.url,
            &self.environment_name_input,
            &self.bearer_token,
            &self.oauth2_access_token,
            &self.basic_username,
            &self.basic_password,
            &self.jwt_token,
            &self.api_key_name,
            &self.api_key_value,
            &self.pre_request_script,
            &self.request_body,
            &self.graphql_query,
            &self.graphql_variables,
            &self.websocket_url,
            &self.websocket_protocols,
            &self.websocket_message,
            &self.sse_url,
            &self.binary_body_path,
        ] {
            set_text_input_enabled(input, enabled, cx);
        }
        set_key_value_rows_enabled(&self.global_variables, enabled, cx);
        set_key_value_rows_enabled(&self.query_params, enabled, cx);
        set_key_value_rows_enabled(&self.request_headers, enabled, cx);
        set_key_value_rows_enabled(&self.websocket_headers, enabled, cx);
        set_key_value_rows_enabled(&self.sse_headers, enabled, cx);
        set_key_value_rows_enabled(&self.form_data_body, enabled, cx);
        set_key_value_rows_enabled(&self.urlencoded_body, enabled, cx);
        for environment in &self.environments {
            set_key_value_rows_enabled(&environment.variables, enabled, cx);
        }
        set_assertion_rows_enabled(&self.request_assertions, enabled, cx);
    }

    fn focus_active_sidebar_input(
        &mut self,
        _: &FocusActiveSidebarInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = sidebar_focus_target(self.active_sidebar_section);
        if !can_focus_sidebar_input(self.busy, target) {
            return;
        }

        let input = match target {
            SidebarFocusTarget::RouteFilter => self.route_filter.clone(),
            SidebarFocusTarget::CollectionPath => self.collection_path.clone(),
            SidebarFocusTarget::HistoryFilter => self.history_filter.clone(),
        };
        let focus_handle = input.read(cx).focus_handle(cx);
        focus_handle.focus(window, cx);
        if self.close_transient_layers() {
            cx.notify();
        }
    }

    fn focus_request_url(
        &mut self,
        _: &FocusRequestUrl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !can_focus_request_url(self.busy) {
            return;
        }

        let focus_handle = self.url.read(cx).focus_handle(cx);
        focus_handle.focus(window, cx);
        if self.close_transient_layers() {
            cx.notify();
        }
    }

    fn open_collection_menu(&mut self, menu: CollectionContextMenu, cx: &mut Context<Self>) {
        if !can_open_collection_context_menu(self.busy) {
            return;
        }

        let label = menu.label.clone();
        self.collection_rename_input
            .update(cx, |input, cx| input.set_text(label, cx));
        self.import_popover_open = false;
        self.method_menu_open = false;
        self.codegen_menu_open = false;
        self.collection_context_menu = Some(menu);
        cx.notify();
    }

    fn close_collection_menu(&mut self, cx: &mut Context<Self>) {
        self.collection_context_menu = None;
        cx.notify();
    }

    fn add_collection_request(&mut self, target_id: String, cx: &mut Context<Self>) {
        if !can_mutate_collection(self.busy) {
            return;
        }

        let request = CollectionItem::Request(blank_collection_request());
        if insert_collection_item(&mut self.collection.items, &target_id, request) {
            self.ensure_collection_node_expanded(target_id);
            self.refresh_collection_status("Request created");
        } else {
            self.collection_status = "Create failed".to_string();
        }
        cx.notify();
    }

    fn add_collection_folder(&mut self, target_id: String, cx: &mut Context<Self>) {
        if !can_mutate_collection(self.busy) {
            return;
        }

        let folder = CollectionItem::Folder(CollectionFolder {
            name: "New Folder".to_string(),
            description: String::new(),
            items: Vec::new(),
        });
        if insert_collection_item(&mut self.collection.items, &target_id, folder) {
            self.ensure_collection_node_expanded(target_id);
            self.refresh_collection_status("Folder created");
        } else {
            self.collection_status = "Create failed".to_string();
        }
        cx.notify();
    }

    fn copy_collection_target(&mut self, target_id: String, cx: &mut Context<Self>) {
        if !can_mutate_collection(self.busy) {
            return;
        }

        if duplicate_collection_item(&mut self.collection.items, &target_id) {
            self.refresh_collection_status("Item copied");
            self.collection_context_menu = None;
        } else {
            self.collection_status = "Copy failed".to_string();
        }
        cx.notify();
    }

    fn delete_collection_target(&mut self, target_id: String, cx: &mut Context<Self>) {
        if !can_mutate_collection(self.busy) {
            return;
        }

        if remove_collection_item(&mut self.collection.items, &target_id).is_some() {
            self.expanded_collection_nodes
                .retain(|node| !node.starts_with(&target_id));
            self.refresh_collection_status("Item deleted");
            self.collection_context_menu = None;
        } else {
            self.collection_status = "Delete failed".to_string();
        }
        cx.notify();
    }

    fn move_collection_target(
        &mut self,
        source_id: String,
        target_id: String,
        cx: &mut Context<Self>,
    ) {
        if !can_mutate_collection(self.busy) {
            return;
        }

        if move_collection_item(&mut self.collection.items, &source_id, &target_id) {
            self.ensure_collection_node_expanded("collection".to_string());
            self.refresh_collection_status("Item moved");
            self.collection_context_menu = None;
        } else if source_id != target_id {
            self.collection_status = "Move failed".to_string();
        }
        cx.notify();
    }

    fn rename_collection_target(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.collection_context_menu.clone() else {
            return;
        };
        let name = self.collection_rename_input.read(cx).text();
        let name = name.trim();
        if !can_submit_collection_rename(self.busy, Some(menu.kind), name) {
            self.collection_status = if name.is_empty() {
                "Name needed".to_string()
            } else if self.busy {
                "Rename unavailable while busy".to_string()
            } else {
                "Rename unavailable".to_string()
            };
            cx.notify();
            return;
        }

        if rename_collection_node(&mut self.collection, &menu.node_id, name) {
            self.refresh_collection_status("Item renamed");
            self.collection_context_menu = None;
        } else {
            self.collection_status = "Rename failed".to_string();
        }
        cx.notify();
    }

    fn ensure_collection_node_expanded(&mut self, node_id: String) {
        if !self
            .expanded_collection_nodes
            .iter()
            .any(|expanded| expanded == &node_id)
        {
            self.expanded_collection_nodes.push(node_id);
        }
    }

    fn refresh_collection_status(&mut self, prefix: &str) {
        self.collection_status = format!(
            "{prefix}: {} requests",
            collection_item_count(&self.collection.items)
        );
    }

    fn toggle_collection_node(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(index) = self
            .expanded_collection_nodes
            .iter()
            .position(|expanded| expanded == &id)
        {
            self.expanded_collection_nodes.remove(index);
        } else {
            self.expanded_collection_nodes.push(id);
        }
        cx.notify();
    }

    fn restore_collection_request(&mut self, request: CollectionRequest, cx: &mut Context<Self>) {
        if !can_restore_request_from_sidebar(self.busy) {
            return;
        }

        self.close_transient_layers();
        self.method = request.method;
        self.url
            .update(cx, |input, cx| input.set_text(request.url, cx));
        set_key_value_rows(&self.request_headers, request.headers, cx);
        set_key_value_rows(&self.query_params, request.query_params, cx);
        self.apply_collection_body(request.body, cx);
        self.pre_request_script.update(cx, |input, cx| {
            input.set_text(request.pre_request_script, cx)
        });
        self.pre_request_status = "idle".to_string();
        self.last_pre_request_actions.clear();
        self.request_assertions = assertion_rows_from_assertions(cx, &request.tests);
        self.last_assertion_results.clear();
        self.set_response(
            RESPONSE_TITLE_RESTORED,
            request.name,
            ResponseTone::Neutral,
            RESPONSE_BODY_REQUEST,
        );
        cx.notify();
    }

    fn apply_collection_body(&mut self, body: CollectionBody, cx: &mut Context<Self>) {
        match body {
            CollectionBody::None => {
                self.request_body_mode = RequestBodyMode::None;
                self.request_body
                    .update(cx, |input, cx| input.set_text("", cx));
            }
            CollectionBody::Raw { content_type, body } => {
                if let Some((query, variables)) = graphql_fields_from_body(&content_type, &body) {
                    self.request_body_mode = RequestBodyMode::GraphQL;
                    self.graphql_query
                        .update(cx, |input, cx| input.set_text(query, cx));
                    self.graphql_variables
                        .update(cx, |input, cx| input.set_text(variables, cx));
                } else {
                    self.request_body_mode = RequestBodyMode::Raw;
                    self.raw_body_format = raw_format_from_content_type(&content_type);
                    self.request_body
                        .update(cx, |input, cx| input.set_text(body, cx));
                }
            }
            CollectionBody::FormData { fields } => {
                self.request_body_mode = RequestBodyMode::FormData;
                set_key_value_rows(&self.form_data_body, fields, cx);
            }
            CollectionBody::UrlEncoded { fields } => {
                self.request_body_mode = RequestBodyMode::UrlEncoded;
                set_key_value_rows(&self.urlencoded_body, fields, cx);
            }
            CollectionBody::Binary { path, .. } => {
                self.request_body_mode = RequestBodyMode::Binary;
                self.binary_body_path
                    .update(cx, |input, cx| input.set_text(path, cx));
            }
        }
    }

    fn add_response_assertion_row(&mut self, cx: &mut Context<Self>) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        self.request_assertions.push(blank_assertion_row(cx));
        cx.notify();
    }

    fn cycle_response_assertion_kind(&mut self, index: usize, cx: &mut Context<Self>) {
        if !can_edit_response_assertion_row(self.busy, self.request_assertions.len(), index) {
            return;
        }

        if let Some(row) = self.request_assertions.get_mut(index) {
            row.kind = row.kind.next();
            cx.notify();
        }
    }

    fn remove_response_assertion_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if !can_remove_response_assertion_row(self.busy, self.request_assertions.len(), index) {
            return;
        }

        self.request_assertions.remove(index);
        cx.notify();
    }

    fn clear_response_assertion_results(&mut self, cx: &mut Context<Self>) {
        if !can_clear_response_assertion_results(self.busy, self.last_assertion_results.len()) {
            return;
        }

        self.last_assertion_results.clear();
        cx.notify();
    }

    fn load_graphql_introspection_query(&mut self, cx: &mut Context<Self>) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        self.request_body_mode = RequestBodyMode::GraphQL;
        self.graphql_query.update(cx, |input, cx| {
            input.set_text(GRAPHQL_INTROSPECTION_QUERY, cx)
        });
        self.graphql_variables
            .update(cx, |input, cx| input.set_text("{}", cx));
        cx.notify();
    }

    fn apply_graphql_query_template(
        &mut self,
        operation: String,
        variables: String,
        cx: &mut Context<Self>,
    ) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        self.request_body_mode = RequestBodyMode::GraphQL;
        self.graphql_query
            .update(cx, |input, cx| input.set_text(operation, cx));
        self.graphql_variables
            .update(cx, |input, cx| input.set_text(variables, cx));
        cx.notify();
    }

    fn apply_route_filter(&mut self, query: &str) {
        self.visible_routes = filter_routes(&self.routes, query);
        self.selected_route = None;
    }

    fn select_route(&mut self, index: usize, cx: &mut Context<Self>) {
        if !can_restore_request_from_sidebar(self.busy) {
            return;
        }

        let Some(route) = self.visible_routes.get(index).cloned() else {
            return;
        };

        self.close_transient_layers();
        self.selected_route = Some(index);
        self.method = route.method.clone();
        self.url.update(cx, |input, cx| {
            input.set_text(format!("http://localhost:8080{}", route.path), cx)
        });
        self.request_body.update(cx, |input, cx| {
            input.set_text(default_request_body(&route.method), cx)
        });
        self.request_body_mode = if default_request_body(&route.method).is_empty() {
            RequestBodyMode::None
        } else {
            RequestBodyMode::Raw
        };
        self.pre_request_script
            .update(cx, |input, cx| input.set_text("", cx));
        self.pre_request_status = "idle".to_string();
        self.last_pre_request_actions.clear();
        self.request_assertions = assertion_rows_from_assertions(cx, &[]);
        self.last_assertion_results.clear();
        self.set_response(
            RESPONSE_TITLE_SELECTED,
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
        self.close_transient_layers();

        let build = match self.current_request_build(cx) {
            Ok(build) => build,
            Err(error) => {
                let error = error.to_string();
                self.pre_request_status = pre_request_error_label(&error);
                self.last_pre_request_actions.clear();
                self.set_response(
                    RESPONSE_TITLE_BUILD_FAIL,
                    "",
                    ResponseTone::Error,
                    editor_error("Request build failed.", &error),
                );
                cx.notify();
                return;
            }
        };
        self.pre_request_status = pre_request_status_label(build.pre_request_actions);
        self.last_pre_request_actions = build.pre_request_action_labels;
        let request = build.request;
        if request.url.is_empty() {
            self.set_response(
                REQUEST_URL_REQUIRED_TITLE,
                "",
                ResponseTone::Error,
                URL_REQUIRED_BODY,
            );
            cx.notify();
            return;
        }
        let assertions = match self.current_response_assertions(cx) {
            Ok(assertions) => assertions,
            Err(error) => {
                self.set_response(
                    RESPONSE_TITLE_BAD_TESTS,
                    "",
                    ResponseTone::Error,
                    editor_error("Tests invalid.", &error.to_string()),
                );
                cx.notify();
                return;
            }
        };

        let history_request =
            history_request_from_body(&request.method, &request.url, &request.body);
        let method = request.method.clone();
        let url = request.url.clone();
        let headers = request.headers.clone();
        let query_params = request.query_params.clone();
        let body = request.body.clone();
        let method_for_error = method.clone();
        let url_for_error = url.clone();
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();

        self.set_busy(true, cx);
        self.last_assertion_results.clear();
        self.set_response(
            "Sending",
            "",
            ResponseTone::Busy,
            sending_response_body(&method, &url),
        );
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(
                client::send_request_with_body(&method, &url, &headers, &query_params, body).await,
            );
        });

        cx.spawn(async move |app, cx| {
            let result = rx.await;
            app.update(cx, |app, cx| {
                match result {
                    Ok(Ok(response)) => {
                        let response_status = format!("HTTP {}", response.status);
                        let assertion_results =
                            evaluate_response_assertions(&response, &assertions);
                        let response_meta = response_panel_meta(&assertion_results);
                        let history_response = HistoryResponse {
                            status: response_status.clone(),
                            meta: response_meta.clone(),
                            body_preview: preview_text(&response.body),
                        };
                        let headers = format_headers(&response.headers);
                        let tone = if assertion_results.iter().any(|result| !result.passed) {
                            ResponseTone::Error
                        } else {
                            response_tone(response.status)
                        };
                        app.record_history(history_request.clone(), history_response);
                        app.last_assertion_results = assertion_results;
                        app.set_http_response(
                            response_status,
                            response_meta,
                            tone,
                            response.body,
                            response.raw_body,
                            headers,
                        );
                    }
                    Ok(Err(error)) => {
                        let error = error.to_string();
                        let body =
                            request_transport_error(&method_for_error, &url_for_error, &error);
                        app.record_history(
                            history_request.clone(),
                            HistoryResponse {
                                status: "Request failed".to_string(),
                                meta: String::new(),
                                body_preview: preview_text(&body),
                            },
                        );
                        app.last_assertion_results.clear();
                        app.set_response(
                            RESPONSE_TITLE_REQUEST_FAIL,
                            "",
                            ResponseTone::Error,
                            body,
                        );
                    }
                    Err(_) => {
                        let error = request_worker_stopped_message();
                        app.record_history(
                            history_request.clone(),
                            HistoryResponse {
                                status: "Request failed".to_string(),
                                meta: String::new(),
                                body_preview: preview_text(error),
                            },
                        );
                        app.last_assertion_results.clear();
                        app.set_response(
                            RESPONSE_TITLE_REQUEST_FAIL,
                            "",
                            ResponseTone::Error,
                            error,
                        );
                    }
                }
                app.set_busy(false, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn connect_websocket(&mut self, cx: &mut Context<Self>) {
        let url = self.websocket_url.read(cx).text();
        if !can_connect_websocket(self.busy, self.websocket_running, &url) {
            if self.busy || self.websocket_running {
                return;
            }
            let (title, body) = websocket_url_validation_response(&url);
            self.websocket_status = if has_trimmed_text(&url) {
                REALTIME_STATUS_BAD_URL.to_string()
            } else {
                REALTIME_STATUS_NO_URL.to_string()
            };
            self.set_response(title, "", ResponseTone::Error, body);
            cx.notify();
            return;
        }

        let options = client::WebSocketSessionOptions {
            headers: read_key_value_rows(&self.websocket_headers, cx),
            protocols: websocket_protocol_list(&self.websocket_protocols.read(cx).text()),
        };
        let runtime = self.runtime.clone();
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        self.websocket_running = true;
        self.websocket_command_tx = Some(command_tx);
        self.websocket_session_url = url.trim().to_string();
        self.websocket_status = "connecting".to_string();
        self.set_response(
            WEBSOCKET_ACTIVE_TITLE,
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        runtime.spawn(async move {
            client::run_websocket_session_with_options(url, options, command_rx, event_tx).await;
        });

        cx.spawn(async move |app, cx| {
            let mut event_rx = event_rx;
            while let Some(event) = event_rx.recv().await {
                app.update(cx, |app, cx| {
                    app.handle_websocket_event(event);
                    cx.notify();
                })
                .ok();
            }
            app.update(cx, |app, cx| {
                if app.websocket_running {
                    app.websocket_running = false;
                    app.websocket_command_tx = None;
                    app.websocket_status = "closed".to_string();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn send_websocket_message(&mut self, cx: &mut Context<Self>) {
        let message = self.websocket_message.read(cx).text();
        if self.busy {
            return;
        }

        let Some(command_tx) = self.websocket_command_tx.as_ref() else {
            self.websocket_status = "not connected".to_string();
            self.set_response(
                WEBSOCKET_NOT_OPEN_TITLE,
                "",
                ResponseTone::Error,
                WEBSOCKET_NOT_OPEN_BODY,
            );
            cx.notify();
            return;
        };

        let command = match self.websocket_message_mode {
            WebSocketMessageMode::Text => client::WebSocketSessionCommand::SendText(message),
            WebSocketMessageMode::BinaryHex => match websocket_hex_bytes(&message) {
                Ok(bytes) => client::WebSocketSessionCommand::SendBinary(bytes),
                Err(error) => {
                    let body = editor_error("WS binary invalid.", &error);
                    self.websocket_status = "invalid binary".to_string();
                    self.set_response(
                        WEBSOCKET_BINARY_INVALID_TITLE,
                        "",
                        ResponseTone::Error,
                        body,
                    );
                    cx.notify();
                    return;
                }
            },
        };

        if command_tx.send(command).is_err() {
            self.websocket_running = false;
            self.websocket_command_tx = None;
            self.websocket_status = "closed".to_string();
            self.set_response(
                WEBSOCKET_SEND_FAILED_TITLE,
                "",
                ResponseTone::Error,
                realtime_operation_error(
                    "WS send failed.",
                    "WS URL",
                    &self.websocket_session_url,
                    "Closed.",
                ),
            );
        } else {
            self.websocket_status = "sending".to_string();
        }
        cx.notify();
    }

    fn close_websocket(&mut self, cx: &mut Context<Self>) {
        if !can_close_websocket(self.busy, self.websocket_running) {
            return;
        }

        if let Some(command_tx) = self.websocket_command_tx.as_ref() {
            let _ = command_tx.send(client::WebSocketSessionCommand::Close);
            self.websocket_status = "closing".to_string();
        } else {
            self.websocket_running = false;
            self.websocket_status = "closed".to_string();
        }
        cx.notify();
    }

    fn handle_websocket_event(&mut self, event: client::WebSocketSessionEvent) {
        match event {
            client::WebSocketSessionEvent::Connected { url } => {
                self.websocket_running = true;
                self.websocket_session_url = url.clone();
                self.websocket_status = "connected".to_string();
                self.set_response(WEBSOCKET_CONNECTED_TITLE, "", ResponseTone::Success, url);
            }
            client::WebSocketSessionEvent::Sent(message) => {
                self.push_websocket_log(WebSocketLogEntry {
                    direction: WebSocketDirection::Sent,
                    kind: websocket_message_kind_label(&message.kind).to_string(),
                    data: message.data,
                });
                self.websocket_status = "sent".to_string();
            }
            client::WebSocketSessionEvent::Received(message) => {
                let kind = websocket_message_kind_label(&message.kind).to_string();
                let data = message.data;
                self.push_websocket_log(WebSocketLogEntry {
                    direction: WebSocketDirection::Received,
                    kind: kind.clone(),
                    data: data.clone(),
                });
                self.websocket_status = format!("received {kind}");
                self.set_response(WEBSOCKET_MESSAGE_TITLE, kind, ResponseTone::Success, data);
            }
            client::WebSocketSessionEvent::Closed(reason) => {
                self.websocket_running = false;
                self.websocket_command_tx = None;
                self.websocket_status = format!("closed: {}", preview_text(&reason));
                self.set_response(WEBSOCKET_CLOSED_TITLE, "", ResponseTone::Neutral, reason);
            }
            client::WebSocketSessionEvent::Error(error) => {
                self.websocket_running = false;
                self.websocket_command_tx = None;
                self.websocket_status = format!("error: {}", preview_text(&error));
                self.set_response(
                    WEBSOCKET_FAILED_TITLE,
                    "",
                    ResponseTone::Error,
                    realtime_operation_error(
                        "WS session failed.",
                        "WS URL",
                        &self.websocket_session_url,
                        &error,
                    ),
                );
            }
        }
    }

    fn push_websocket_log(&mut self, entry: WebSocketLogEntry) {
        self.websocket_messages.push(entry);
        let overflow = self
            .websocket_messages
            .len()
            .saturating_sub(WEBSOCKET_LOG_LIMIT);
        if overflow > 0 {
            self.websocket_messages.drain(0..overflow);
        }
    }

    fn fetch_sse_events(&mut self, cx: &mut Context<Self>) {
        let url = self.sse_url.read(cx).text();
        if !can_start_sse(self.busy, self.sse_running, &url) {
            if self.busy || self.sse_running {
                return;
            }
            let (title, body) = sse_url_validation_response(&url);
            self.sse_status = if has_trimmed_text(&url) {
                REALTIME_STATUS_BAD_URL.to_string()
            } else {
                REALTIME_STATUS_NO_URL.to_string()
            };
            self.set_response(title, "", ResponseTone::Error, body);
            cx.notify();
            return;
        }

        let runtime = self.runtime.clone();
        let headers = read_key_value_rows(&self.sse_headers, cx);
        let url_for_error = url.trim().to_string();
        let (tx, rx) = oneshot::channel();
        self.sse_running = true;
        self.sse_session_url = url_for_error.clone();
        self.sse_status = "connecting".to_string();
        self.set_response(
            SSE_ACTIVE_TITLE,
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(
                client::collect_sse_events_with_headers(&url, SSE_EVENT_FETCH_LIMIT, headers).await,
            );
        });

        cx.spawn(async move |app, cx| {
            if let Ok(result) = rx.await {
                app.update(cx, |app, cx| {
                    match result {
                        Ok(exchange) => {
                            let entries = sse_log_entries(&exchange);
                            for entry in entries {
                                app.push_sse_log(entry);
                            }
                            let meta = sse_event_count_label(exchange.events.len());
                            app.sse_status = meta.clone();
                            app.set_response(
                                SSE_OK_TITLE,
                                meta,
                                ResponseTone::Success,
                                sse_exchange_text(&exchange),
                            );
                        }
                        Err(error) => {
                            let error = error.to_string();
                            let body = realtime_operation_error(
                                "SSE fetch failed.",
                                "SSE URL",
                                &url_for_error,
                                &error,
                            );
                            app.sse_status = format!("error: {}", preview_text(&error));
                            app.set_response(SSE_FAILED_TITLE, "", ResponseTone::Error, body);
                        }
                    }
                    app.sse_running = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn subscribe_sse_events(&mut self, cx: &mut Context<Self>) {
        let url = self.sse_url.read(cx).text();
        if !can_start_sse(self.busy, self.sse_running, &url) {
            if self.busy || self.sse_running {
                return;
            }
            let (title, body) = sse_url_validation_response(&url);
            self.sse_status = if has_trimmed_text(&url) {
                REALTIME_STATUS_BAD_URL.to_string()
            } else {
                REALTIME_STATUS_NO_URL.to_string()
            };
            self.set_response(title, "", ResponseTone::Error, body);
            cx.notify();
            return;
        }

        let runtime = self.runtime.clone();
        let last_event_id = self.sse_last_event_id.clone();
        let headers = read_key_value_rows(&self.sse_headers, cx);
        self.sse_session_url = url.trim().to_string();
        let options = client::SseSubscriptionOptions {
            last_event_id,
            headers,
            ..Default::default()
        };
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let handle = runtime.spawn(client::run_sse_subscription_with_options(
            url, options, event_tx,
        ));
        self.sse_subscription = Some(handle);
        self.sse_running = true;
        self.sse_status = "subscribing".to_string();
        self.set_response(
            SSE_SUBSCRIBING_TITLE,
            "",
            ResponseTone::Busy,
            self.response_body.clone(),
        );
        cx.notify();

        cx.spawn(async move |app, cx| {
            let mut event_rx = event_rx;
            while let Some(event) = event_rx.recv().await {
                app.update(cx, |app, cx| {
                    app.handle_sse_stream_event(event);
                    cx.notify();
                })
                .ok();
            }
            app.update(cx, |app, cx| {
                if app.sse_subscription.is_some() {
                    app.sse_subscription = None;
                    app.sse_running = false;
                    app.sse_status = "closed".to_string();
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn stop_sse_subscription(&mut self, cx: &mut Context<Self>) {
        if !can_stop_sse_subscription(self.busy, self.sse_subscription.is_some()) {
            return;
        }

        if let Some(handle) = self.sse_subscription.take() {
            handle.abort();
            self.sse_running = false;
            self.sse_status = "stopped".to_string();
            self.set_response(
                SSE_STOPPED_TITLE,
                "",
                ResponseTone::Neutral,
                SSE_STOPPED_BODY,
            );
        }
        cx.notify();
    }

    fn handle_sse_stream_event(&mut self, event: client::SseStreamEvent) {
        match event {
            client::SseStreamEvent::Connected { url } => {
                self.sse_running = true;
                self.sse_session_url = url.clone();
                self.sse_status = "subscribed".to_string();
                self.set_response(SSE_SUBSCRIBED_TITLE, "", ResponseTone::Success, url);
            }
            client::SseStreamEvent::Event(event) => {
                let label = sse_event_label(&event).to_string();
                let data = event.data.clone();
                self.push_sse_log(sse_log_entry(&event));
                self.sse_status = format!("event {label}");
                self.set_response(SSE_EVENT_TITLE, label, ResponseTone::Success, data);
            }
            client::SseStreamEvent::Reconnecting {
                attempt,
                delay_ms,
                reason,
            } => {
                self.sse_running = true;
                self.sse_status = sse_reconnect_status(attempt, delay_ms);
                self.set_response(
                    SSE_RETRY_TITLE,
                    self.sse_status.clone(),
                    ResponseTone::Busy,
                    realtime_operation_error(
                        "SSE retry.",
                        "SSE URL",
                        &self.sse_session_url,
                        &reason,
                    ),
                );
            }
            client::SseStreamEvent::Closed(reason) => {
                self.sse_subscription = None;
                self.sse_running = false;
                self.sse_status = format!("closed: {}", preview_text(&reason));
                self.set_response(SSE_CLOSED_TITLE, "", ResponseTone::Neutral, reason);
            }
            client::SseStreamEvent::Error(error) => {
                self.sse_subscription = None;
                self.sse_running = false;
                self.sse_status = format!("error: {}", preview_text(&error));
                self.set_response(
                    SSE_FAILED_TITLE,
                    "",
                    ResponseTone::Error,
                    realtime_operation_error(
                        "SSE subscription failed.",
                        "SSE URL",
                        &self.sse_session_url,
                        &error,
                    ),
                );
            }
        }
    }

    fn push_sse_log(&mut self, entry: SseLogEntry) {
        if let Some(id) = &entry.id {
            self.sse_last_event_id = Some(id.clone());
        }
        self.sse_events.push(entry);
        let overflow = self.sse_events.len().saturating_sub(SSE_LOG_LIMIT);
        if overflow > 0 {
            self.sse_events.drain(0..overflow);
        }
    }

    fn run_collection_runner(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.runner_running {
            return;
        }

        let total = collection_item_count(&self.collection.items);
        if total == 0 {
            self.runner_status = RUNNER_EMPTY_REQUESTS_LABEL.to_string();
            self.set_response(
                RUNNER_EMPTY_TITLE,
                "",
                ResponseTone::Error,
                SAVED_REQUESTS_EMPTY_BODY,
            );
            cx.notify();
            return;
        }

        let collection = self.collection.clone();
        let variables = self.variable_store(cx);
        let active_environment = self.active_environment.clone();
        let options = RunnerOptions {
            delay_ms: 0,
            failure_strategy: if self.runner_stop_on_failure {
                FailureStrategy::StopOnFailure
            } else {
                FailureStrategy::Continue
            },
        };
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();

        self.set_busy(true, cx);
        self.runner_running = true;
        self.runner_results.clear();
        self.runner_status = format!("Run {total}");
        self.set_response(
            RUNNER_ACTIVE_TITLE,
            "",
            ResponseTone::Busy,
            RESPONSE_BODY_RUNNER,
        );
        cx.notify();

        runtime.spawn(async move {
            let summary = collection_runner::run_collection(
                &collection,
                &variables,
                active_environment.as_deref(),
                options,
            )
            .await;
            let _ = tx.send(summary);
        });

        cx.spawn(async move |app, cx| {
            let result = rx.await;
            app.update(cx, |app, cx| match result {
                Ok(summary) => app.apply_collection_run_summary(summary, cx),
                Err(_) => {
                    app.runner_running = false;
                    app.set_busy(false, cx);
                    app.runner_status = RESPONSE_TITLE_RUN_FAILED.to_string();
                    app.runner_results.clear();
                    app.set_response(
                        RESPONSE_TITLE_RUNNER_FAIL,
                        "",
                        ResponseTone::Error,
                        runner_worker_stopped_message(),
                    );
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn apply_collection_run_summary(
        &mut self,
        summary: CollectionRunSummary,
        cx: &mut Context<Self>,
    ) {
        let tone = if summary.failed == 0 {
            ResponseTone::Success
        } else {
            ResponseTone::Error
        };
        let status = if summary.failed == 0 {
            RESPONSE_TITLE_RUN_PASSED
        } else if summary.stopped_early {
            RESPONSE_TITLE_RUN_STOPPED
        } else {
            RESPONSE_TITLE_RUN_FAILED
        };
        let body = if summary.failed == 0 {
            runner_summary_text(&summary)
        } else {
            runner_failure_text(&summary)
        };

        self.runner_running = false;
        self.set_busy(false, cx);
        self.runner_status = runner_status_text(&summary);
        self.runner_results = summary.results.clone();
        self.set_response(status, self.runner_status.clone(), tone, body);
        cx.notify();
    }

    fn toggle_mock_server(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if let Some(server) = self.server.take() {
            self.set_busy(true, cx);
            self.server_running = false;
            self.server_status = MOCK_STATUS_STOPPING.to_string();
            let runtime = self.runtime.clone();
            let (tx, rx) = oneshot::channel();

            runtime.spawn(async move {
                server.stop().await;
                let _ = tx.send(());
            });

            cx.spawn(async move |app, cx| {
                if rx.await.is_ok() {
                    app.update(cx, |app, cx| {
                        app.set_busy(false, cx);
                        app.server_running = false;
                        app.server_status = MOCK_STATUS_STOPPED.to_string();
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
                MOCK_ROUTES_REQUIRED_TITLE,
                "",
                ResponseTone::Error,
                MOCK_ROUTES_REQUIRED_BODY,
            );
            self.server_status = MOCK_STATUS_IMPORT_ROUTES_FIRST.to_string();
            cx.notify();
            return;
        }

        let routes = self.routes.clone();
        let runtime = self.runtime.clone();
        let (tx, rx) = oneshot::channel();
        let (log_tx, mut log_rx) = mpsc::unbounded_channel();
        self.set_busy(true, cx);
        self.server_status = MOCK_STATUS_STARTING.to_string();
        self.mock_logs.clear();
        cx.notify();

        runtime.spawn(async move {
            let _ = tx.send(MockServer::start_with_logs(routes, MOCK_SERVER_PORT, log_tx).await);
        });

        cx.spawn(async move |app, cx| {
            while let Some(entry) = log_rx.recv().await {
                if app
                    .update(cx, |app, cx| {
                        app.record_mock_log(entry);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

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
                            app.server_status = MOCK_STATUS_FAILED.to_string();
                            app.set_response(
                                MOCK_STATUS_FAILED,
                                "",
                                ResponseTone::Error,
                                mock_server_error(MOCK_SERVER_PORT, &error.to_string()),
                            );
                        }
                    }
                    app.set_busy(false, cx);
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
        let body = body.into();
        self.response_body = body.clone();
        self.response_raw_body = body;
        self.response_headers.clear();
        self.response_view = ResponseView::Pretty;
        self.response_pretty_collapsed = false;
        self.graphql_schema_summary.clear();
        self.graphql_schema_browser.clear();
        self.graphql_query_templates.clear();
        reset_scroll_handle(&self.response_scroll);
    }

    fn set_http_response(
        &mut self,
        status: impl Into<String>,
        meta: impl Into<String>,
        tone: ResponseTone,
        pretty_body: impl Into<String>,
        raw_body: impl Into<String>,
        headers: impl Into<String>,
    ) {
        self.response_status = status.into();
        self.response_meta = meta.into();
        self.response_tone = tone;
        self.response_body = pretty_body.into();
        self.response_raw_body = raw_body.into();
        self.graphql_schema_summary =
            graphql_schema_summary(&self.response_raw_body).unwrap_or_default();
        self.graphql_schema_browser =
            graphql_schema_browser(&self.response_raw_body).unwrap_or_default();
        self.graphql_query_templates =
            graphql_query_templates(&self.response_raw_body).unwrap_or_default();
        self.response_headers = headers.into();
        self.response_pretty_collapsed = false;
        reset_scroll_handle(&self.response_scroll);
    }

    fn record_mock_log(&mut self, entry: MockRequestLog) {
        const MAX_LOGS: usize = 50;

        self.mock_logs.push(entry);
        let overflow = self.mock_logs.len().saturating_sub(MAX_LOGS);
        if overflow > 0 {
            self.mock_logs.drain(0..overflow);
        }
    }

    fn record_history(&mut self, request: HistoryRequest, response: HistoryResponse) {
        const MAX_HISTORY: usize = 100;

        self.history.record(request, response);
        while self.history.entries().len() > MAX_HISTORY {
            if let Some(id) = self.history.entries().last().map(|entry| entry.id) {
                self.history.remove(id);
            } else {
                break;
            }
        }
    }

    fn add_environment(&mut self, cx: &mut Context<Self>) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        let name = self.environment_name_input.read(cx).text();
        let name = normalized_environment_name(&name);
        if name.is_empty() {
            self.set_response(
                VARIABLES_ENV_NEEDED_TITLE,
                "",
                ResponseTone::Error,
                VARIABLES_ENV_NAME_REQUIRED_BODY,
            );
            cx.notify();
            return;
        }

        if self
            .environments
            .iter()
            .any(|environment| environment.name == name)
        {
            self.active_environment = Some(name.clone());
            self.environment_name_input
                .update(cx, |input, cx| input.set_text("", cx));
            self.set_response(
                VARIABLES_ENV_SELECTED_TITLE,
                name,
                ResponseTone::Neutral,
                VARIABLES_ENV_RESPONSE_BODY,
            );
            cx.notify();
            return;
        }

        self.environments.push(environment_config(
            cx,
            &name,
            &[("baseUrl", ""), ("token", ""), ("", "")],
        ));
        self.active_environment = Some(name.clone());
        self.environment_name_input
            .update(cx, |input, cx| input.set_text("", cx));
        self.set_response(
            VARIABLES_ENV_CREATED_TITLE,
            name,
            ResponseTone::Success,
            VARIABLES_ENV_RESPONSE_BODY,
        );
        cx.notify();
    }

    fn delete_active_environment(&mut self, cx: &mut Context<Self>) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        let Some(active_environment) = self.active_environment.clone() else {
            return;
        };

        if let Some(index) = self
            .environments
            .iter()
            .position(|environment| environment.name == active_environment)
        {
            self.environments.remove(index);
            self.active_environment = None;
            self.set_response(
                VARIABLES_ENV_DELETED_TITLE,
                active_environment,
                ResponseTone::Success,
                VARIABLES_ENV_RESPONSE_BODY,
            );
            cx.notify();
        }
    }

    fn copy_headers_bulk(&mut self, cx: &mut Context<Self>) {
        let headers = read_key_value_rows(&self.request_headers, cx);
        if !can_copy_headers_bulk(self.busy, !headers.is_empty()) {
            if self.busy {
                return;
            }
            self.set_response(
                HEADER_COPY_EMPTY_TITLE,
                "",
                ResponseTone::Neutral,
                NO_HEADERS_BODY,
            );
            cx.notify();
            return;
        }

        cx.write_to_clipboard(ClipboardItem::new_string(format_header_bulk(&headers)));
        self.set_response(
            HEADER_COPIED_TITLE,
            headers.len().to_string(),
            ResponseTone::Success,
            HEADER_BULK_HEADERS_BODY,
        );
        cx.notify();
    }

    fn copy_websocket_log(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if !can_use_realtime_log_actions(self.busy, self.websocket_messages.len()) {
            self.set_response(
                REALTIME_LOG_EMPTY_TITLE,
                "",
                ResponseTone::Neutral,
                NO_MESSAGES_BODY,
            );
            cx.notify();
            return;
        }

        let log = format_websocket_log(&self.websocket_messages);
        cx.write_to_clipboard(ClipboardItem::new_string(log));
        self.set_response(
            REALTIME_LOG_COPIED_TITLE,
            self.websocket_messages.len().to_string(),
            ResponseTone::Success,
            REALTIME_LOG_BODY,
        );
        cx.notify();
    }

    fn clear_websocket_log(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if !can_use_realtime_log_actions(self.busy, self.websocket_messages.len()) {
            self.set_response(
                REALTIME_LOG_EMPTY_TITLE,
                "",
                ResponseTone::Neutral,
                NO_MESSAGES_BODY,
            );
            cx.notify();
            return;
        }

        self.websocket_messages.clear();
        self.websocket_status = if self.websocket_running {
            "connected".to_string()
        } else {
            "idle".to_string()
        };
        self.set_response(
            REALTIME_LOG_CLEARED_TITLE,
            "",
            ResponseTone::Neutral,
            REALTIME_LOG_BODY,
        );
        cx.notify();
    }

    fn copy_sse_log(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if !can_use_realtime_log_actions(self.busy, self.sse_events.len()) {
            self.set_response(
                REALTIME_LOG_EMPTY_TITLE,
                "",
                ResponseTone::Neutral,
                NO_EVENTS_BODY,
            );
            cx.notify();
            return;
        }

        let log = format_sse_log(&self.sse_events);
        cx.write_to_clipboard(ClipboardItem::new_string(log));
        self.set_response(
            REALTIME_LOG_COPIED_TITLE,
            self.sse_events.len().to_string(),
            ResponseTone::Success,
            REALTIME_LOG_BODY,
        );
        cx.notify();
    }

    fn copy_response_body(&mut self, cx: &mut Context<Self>) {
        if !can_copy_response_view(
            self.busy,
            self.response_view,
            self.response_pretty_collapsed,
            &self.response_body,
            &self.response_raw_body,
            &self.response_headers,
        ) {
            return;
        }

        if let Some(text) = response_copy_text(
            self.response_view,
            self.response_pretty_collapsed,
            &self.response_body,
            &self.response_raw_body,
            &self.response_headers,
        ) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn clear_sse_log(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }

        if !can_use_realtime_log_actions(self.busy, self.sse_events.len()) {
            self.set_response(
                REALTIME_LOG_EMPTY_TITLE,
                "",
                ResponseTone::Neutral,
                NO_EVENTS_BODY,
            );
            cx.notify();
            return;
        }

        self.sse_events.clear();
        self.sse_last_event_id = None;
        self.sse_status = if self.sse_running {
            "subscribed".to_string()
        } else {
            "idle".to_string()
        };
        self.set_response(
            REALTIME_LOG_CLEARED_TITLE,
            "",
            ResponseTone::Neutral,
            REALTIME_LOG_BODY,
        );
        cx.notify();
    }

    fn paste_headers_bulk(&mut self, cx: &mut Context<Self>) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            self.set_response(
                HEADER_BULK_CLIPBOARD_EMPTY_TITLE,
                "",
                ResponseTone::Error,
                HEADER_BULK_CLIPBOARD_EMPTY_BODY,
            );
            cx.notify();
            return;
        };
        let headers = parse_header_bulk(&text);
        if headers.is_empty() {
            self.set_response(
                HEADER_BULK_PARSE_EMPTY_TITLE,
                "",
                ResponseTone::Error,
                HEADER_BULK_PARSE_EMPTY_BODY,
            );
            cx.notify();
            return;
        }

        set_key_value_pairs(&mut self.request_headers, headers.clone(), cx);
        self.set_response(
            HEADER_APPLIED_TITLE,
            headers.len().to_string(),
            ResponseTone::Success,
            HEADER_BULK_HEADERS_BODY,
        );
        cx.notify();
    }

    fn apply_header_preset(
        &mut self,
        name: &'static str,
        value: &'static str,
        cx: &mut Context<Self>,
    ) {
        if !can_edit_request_configuration(self.busy) {
            return;
        }

        let headers = read_key_value_rows(&self.request_headers, cx);
        let headers = upsert_header_pair(&headers, name, value);
        set_key_value_pairs(&mut self.request_headers, headers, cx);
        self.set_response(
            HEADER_APPLIED_TITLE,
            name,
            ResponseTone::Success,
            HEADER_PRESET_BODY,
        );
        cx.notify();
    }

    fn format_raw_json_body(&mut self, cx: &mut Context<Self>) {
        let body = self.request_body.read(cx).text();
        if !can_format_request_raw_json(self.busy, self.raw_body_format, &body) {
            return;
        }

        match formatted_json_body(&body) {
            Ok(formatted) => {
                self.request_body
                    .update(cx, |input, cx| input.set_text(formatted, cx));
                self.set_response(
                    RAW_FORMATTED_TITLE,
                    "JSON",
                    ResponseTone::Success,
                    RAW_FORMATTED_BODY,
                );
            }
            Err(error) => {
                self.set_response(
                    RESPONSE_TITLE_FORMAT_FAIL,
                    "JSON",
                    ResponseTone::Error,
                    error.to_string(),
                );
            }
        }
        cx.notify();
    }

    fn restore_history_entry(&mut self, id: u64, cx: &mut Context<Self>) {
        if !can_restore_request_from_sidebar(self.busy) {
            return;
        }

        let Some(entry) = self.history.find(id).cloned() else {
            return;
        };

        self.close_transient_layers();
        self.method = entry.request.method;
        self.url
            .update(cx, |input, cx| input.set_text(entry.request.url, cx));
        self.request_body.update(cx, |input, cx| {
            input.set_text(entry.request.body_preview.clone(), cx)
        });
        self.request_body_mode = if entry.request.body_preview.is_empty() {
            RequestBodyMode::None
        } else {
            RequestBodyMode::Raw
        };
        self.set_response(
            entry.response.status,
            entry.response.meta,
            ResponseTone::Neutral,
            entry.response.body_preview,
        );
        cx.notify();
    }

    fn auth_pairs(&self, cx: &mut Context<Self>) -> (Vec<(String, String)>, Vec<(String, String)>) {
        match self.auth_mode {
            AuthMode::None => (Vec::new(), Vec::new()),
            AuthMode::Bearer => {
                let token = self.bearer_token.read(cx).text();
                let headers = bearer_auth_pair(&token).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::OAuth2 => {
                let token = self.oauth2_access_token.read(cx).text();
                let headers = oauth2_access_token_pair(&token).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::Basic => {
                let username = self.basic_username.read(cx).text();
                let password = self.basic_password.read(cx).text();
                let headers = basic_auth_pair(&username, &password).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::Jwt => {
                let token = self.jwt_token.read(cx).text();
                let headers = jwt_auth_pair(&token).into_iter().collect();
                (headers, Vec::new())
            }
            AuthMode::ApiKey => {
                let name = self.api_key_name.read(cx).text();
                let value = self.api_key_value.read(cx).text();
                let Some(pair) = api_key_pair(&name, &value) else {
                    return (Vec::new(), Vec::new());
                };

                match self.api_key_placement {
                    ApiKeyPlacement::Header => (vec![pair], Vec::new()),
                    ApiKeyPlacement::Query => (Vec::new(), vec![pair]),
                }
            }
        }
    }

    fn current_raw_codegen_request(&self, cx: &mut Context<Self>) -> CodegenRequest {
        let mut headers = read_key_value_rows(&self.request_headers, cx);
        let mut query_params = read_key_value_rows(&self.query_params, cx);
        let (auth_headers, auth_query_params) = self.auth_pairs(cx);
        headers.extend(auth_headers);
        query_params.extend(auth_query_params);

        CodegenRequest {
            method: self.method.clone(),
            url: self.url.read(cx).text(),
            headers,
            query_params,
            body: self.request_body_for_send(cx),
        }
    }

    fn current_request_build(&self, cx: &mut Context<Self>) -> Result<RequestBuild> {
        let variable_store = self.variable_store(cx);
        let active_environment = self.active_environment.as_deref();
        let raw_request = self.current_raw_codegen_request(cx);
        let execution = execute_pre_request_actions(
            &self.pre_request_script.read(cx).text(),
            raw_request,
            variable_store,
            active_environment,
        )?;

        let request = resolve_codegen_request_templates(
            execution.request,
            &execution.variables,
            active_environment,
        )?;

        Ok(RequestBuild {
            request,
            pre_request_actions: execution.actions_applied,
            pre_request_action_labels: pre_request_action_labels(&execution.actions),
        })
    }

    fn current_codegen_request(&self, cx: &mut Context<Self>) -> Result<CodegenRequest> {
        Ok(self.current_request_build(cx)?.request)
    }

    fn variable_store(&self, cx: &mut Context<Self>) -> VariableStore {
        let active_environment = self.active_environment.as_deref();
        let environment_variables = self
            .active_environment_variables()
            .map(|variables| read_key_value_rows(variables, cx))
            .unwrap_or_default();

        variable_store_from_pairs(
            read_key_value_rows(&self.global_variables, cx),
            active_environment,
            environment_variables,
        )
    }

    fn active_environment_variables(&self) -> Option<&[KeyValueRow]> {
        let active_environment = self.active_environment.as_deref()?;
        self.environments
            .iter()
            .find(|environment| environment.name == active_environment)
            .map(|environment| environment.variables.as_slice())
    }

    fn add_key_value_row(&mut self, target: KeyValueEditorTarget, cx: &mut Context<Self>) {
        if !can_add_key_value_row(self.busy, target, self.active_environment.is_some()) {
            return;
        }

        let (key_placeholder, value_placeholder) = key_value_editor_add_placeholders(target);
        let row = key_value_row_entity(cx, key_placeholder, value_placeholder);

        match target {
            KeyValueEditorTarget::QueryParams => self.query_params.push(row),
            KeyValueEditorTarget::RequestHeaders => self.request_headers.push(row),
            KeyValueEditorTarget::GlobalVariables => self.global_variables.push(row),
            KeyValueEditorTarget::ActiveEnvironmentVariables => {
                let Some(active_environment) = self.active_environment.as_deref() else {
                    return;
                };
                let Some(environment) = self
                    .environments
                    .iter_mut()
                    .find(|environment| environment.name == active_environment)
                else {
                    return;
                };
                environment.variables.push(row);
            }
            KeyValueEditorTarget::FormDataBody => self.form_data_body.push(row),
            KeyValueEditorTarget::UrlEncodedBody => self.urlencoded_body.push(row),
            KeyValueEditorTarget::WebSocketHeaders => self.websocket_headers.push(row),
            KeyValueEditorTarget::SseHeaders => self.sse_headers.push(row),
        }

        cx.notify();
    }

    fn remove_key_value_row(
        &mut self,
        target: KeyValueEditorTarget,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if !can_remove_key_value_row(
            self.busy,
            target,
            self.active_environment.is_some(),
            self.key_value_row_count(target),
            index,
        ) {
            return;
        }

        match target {
            KeyValueEditorTarget::QueryParams => {
                self.query_params.remove(index);
            }
            KeyValueEditorTarget::RequestHeaders => {
                self.request_headers.remove(index);
            }
            KeyValueEditorTarget::GlobalVariables => {
                self.global_variables.remove(index);
            }
            KeyValueEditorTarget::ActiveEnvironmentVariables => {
                let Some(active_environment) = self.active_environment.as_deref() else {
                    return;
                };
                let Some(environment) = self
                    .environments
                    .iter_mut()
                    .find(|environment| environment.name == active_environment)
                else {
                    return;
                };
                environment.variables.remove(index);
            }
            KeyValueEditorTarget::FormDataBody => {
                self.form_data_body.remove(index);
            }
            KeyValueEditorTarget::UrlEncodedBody => {
                self.urlencoded_body.remove(index);
            }
            KeyValueEditorTarget::WebSocketHeaders => {
                self.websocket_headers.remove(index);
            }
            KeyValueEditorTarget::SseHeaders => {
                self.sse_headers.remove(index);
            }
        }

        cx.notify();
    }

    fn key_value_row_count(&self, target: KeyValueEditorTarget) -> usize {
        match target {
            KeyValueEditorTarget::QueryParams => self.query_params.len(),
            KeyValueEditorTarget::RequestHeaders => self.request_headers.len(),
            KeyValueEditorTarget::GlobalVariables => self.global_variables.len(),
            KeyValueEditorTarget::ActiveEnvironmentVariables => self
                .active_environment_variables()
                .map_or(0, |rows| rows.len()),
            KeyValueEditorTarget::FormDataBody => self.form_data_body.len(),
            KeyValueEditorTarget::UrlEncodedBody => self.urlencoded_body.len(),
            KeyValueEditorTarget::WebSocketHeaders => self.websocket_headers.len(),
            KeyValueEditorTarget::SseHeaders => self.sse_headers.len(),
        }
    }

    fn request_body_for_send(&self, cx: &mut Context<Self>) -> RequestBody {
        match self.request_body_mode {
            RequestBodyMode::None => RequestBody::None,
            RequestBodyMode::FormData => {
                RequestBody::Multipart(read_key_value_rows(&self.form_data_body, cx))
            }
            RequestBodyMode::UrlEncoded => {
                RequestBody::FormUrlEncoded(read_key_value_rows(&self.urlencoded_body, cx))
            }
            RequestBodyMode::Raw => RequestBody::Raw {
                content_type: Some(self.raw_body_format.content_type().to_string()),
                body: self.request_body.read(cx).text(),
            },
            RequestBodyMode::GraphQL => RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: graphql_body(
                    &self.graphql_query.read(cx).text(),
                    &self.graphql_variables.read(cx).text(),
                ),
            },
            RequestBodyMode::Binary => RequestBody::BinaryFile {
                path: self.binary_body_path.read(cx).text(),
                content_type: Some("application/octet-stream".to_string()),
            },
        }
    }

    fn current_response_assertions(
        &self,
        cx: &mut Context<Self>,
    ) -> Result<Vec<ResponseAssertion>> {
        self.request_assertions
            .iter()
            .filter_map(|row| {
                let name = row.name.read(cx).text();
                let target = row.target.read(cx).text();
                let expected = row.expected.read(cx).text();
                match response_assertion_from_fields(row.kind, &name, &target, &expected) {
                    Ok(Some(assertion)) => Some(Ok(assertion)),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                }
            })
            .collect()
    }

    fn workspace_ratios(&self) -> (f32, f32, f32) {
        let sidebar = self
            .workspace_sidebar_ratio
            .clamp(WORKSPACE_SIDEBAR_MIN_RATIO, WORKSPACE_SIDEBAR_MAX_RATIO);
        let max_request = WORKSPACE_REQUEST_MAX_RATIO
            .min(1.0 - sidebar - WORKSPACE_RESPONSE_MIN_RATIO)
            .max(WORKSPACE_REQUEST_MIN_RATIO);
        let request = self
            .workspace_request_ratio
            .clamp(WORKSPACE_REQUEST_MIN_RATIO, max_request);
        let response = 1.0 - sidebar - request;
        (sidebar, request, response)
    }

    fn resize_workspace_split(
        &mut self,
        event: &DragMoveEvent<WorkspaceSplitDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let width = event.bounds.right() - event.bounds.left();
        let width_px = width.as_f32();
        if width_px <= 0.0 {
            return;
        }

        let pointer_ratio = quantize_workspace_split_ratio(
            ((event.event.position.x - event.bounds.left()) / width).clamp(0.0, 1.0),
            width_px,
        );
        let (current_sidebar, current_request, _) = self.workspace_ratios();
        let next_pending = workspace_split_preview(
            *event.drag(cx),
            pointer_ratio,
            current_sidebar,
            current_request,
        );
        if workspace_split_pending_changed(self.workspace_split_pending, next_pending) {
            // Keep pane layout stable during drag; the GPUI drag preview tracks the pointer.
            self.workspace_split_pending = next_pending;
        }
    }

    fn finish_workspace_split_resize(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(preview) = self.workspace_split_pending.take() else {
            return false;
        };
        let (current_sidebar, current_request, _) = self.workspace_ratios();
        if workspace_split_ratios_changed(
            current_sidebar,
            current_request,
            preview.sidebar_ratio,
            preview.request_ratio,
        ) {
            self.workspace_sidebar_ratio = preview.sidebar_ratio;
            self.workspace_request_ratio = preview.request_ratio;
            cx.notify();
        }
        true
    }

    fn reset_workspace_split(&mut self, cx: &mut Context<Self>) {
        self.workspace_split_pending = None;
        self.workspace_sidebar_ratio = WORKSPACE_SIDEBAR_DEFAULT_RATIO;
        self.workspace_request_ratio = WORKSPACE_REQUEST_DEFAULT_RATIO;
        cx.notify();
    }

    fn method_selector(&self, cx: &mut Context<Self>) -> gpui::Div {
        let enabled = can_select_request_method(self.busy);
        div()
            .relative()
            .flex()
            .items_center()
            .justify_between()
            .h(px(TEXT_INPUT_HEIGHT))
            .w(px(REQUEST_METHOD_SEGMENT_WIDTH))
            .flex_shrink_0()
            .px_2()
            .text_size(px(REQUEST_PRIMARY_CONTROL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(if enabled {
                method_color(&self.method)
            } else {
                ui_disabled_text()
            })
            .opacity(control_opacity(enabled))
            .when(enabled, |selector| selector.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                    if can_select_request_method(app.busy) {
                        app.method_menu_open = !app.method_menu_open;
                        app.import_popover_open = false;
                        app.codegen_menu_open = false;
                        app.collection_context_menu = None;
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(self.method.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(METHOD_CHEVRON_TEXT_SIZE))
                    .text_color(if !enabled {
                        ui_disabled_text()
                    } else if self.method_menu_open {
                        method_color(&self.method)
                    } else {
                        ui_text_body()
                    })
                    .child(if self.method_menu_open { "^" } else { "v" }),
            )
    }

    fn workspace_split_handle(
        &self,
        split: WorkspaceSplitDrag,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(match split {
                WorkspaceSplitDrag::SidebarRequest => "sidebar-request-split-handle",
                WorkspaceSplitDrag::RequestResponse => "request-response-split-handle",
            })
            .relative()
            .h_full()
            .w(px(WORKSPACE_SPLIT_HANDLE_WIDTH))
            .flex_shrink_0()
            .bg(ui_workspace_gutter())
            .cursor_col_resize()
            .hover(|handle| handle.bg(ui_hover()))
            .on_click(cx.listener(|app, event: &gpui::ClickEvent, _window, cx| {
                if event.click_count() >= 2 {
                    app.reset_workspace_split(cx);
                }
            }))
            .on_drag(split, |_, _, _, cx| cx.new(|_| WorkspaceSplitDragPreview))
            .child(
                div()
                    .absolute()
                    .left(px((WORKSPACE_SPLIT_HANDLE_WIDTH
                        - WORKSPACE_SPLIT_DIVIDER_WIDTH)
                        / 2.))
                    .w(px(WORKSPACE_SPLIT_DIVIDER_WIDTH))
                    .h_full()
                    .bg(ui_border_strong()),
            )
    }

    fn scrollbar_metrics(scroll: &ScrollHandle) -> Option<ScrollbarMetrics> {
        let viewport_height = scroll.bounds().size.height.as_f32();
        let max_offset = scroll.max_offset().y.as_f32();

        if viewport_height <= 0.0 || max_offset <= 1.0 {
            return None;
        }

        let content_height = viewport_height + max_offset;
        let thumb_height = (viewport_height * viewport_height / content_height)
            .clamp(SCROLLBAR_MIN_THUMB_HEIGHT, viewport_height);
        let max_thumb_top = (viewport_height - thumb_height).max(0.0);
        let scroll_top = (-scroll.offset().y.as_f32()).clamp(0.0, max_offset);
        let thumb_top = if max_offset <= 0.0 {
            0.0
        } else {
            (scroll_top / max_offset * max_thumb_top).clamp(0.0, max_thumb_top)
        };

        Some(ScrollbarMetrics {
            max_offset,
            thumb_top,
            thumb_height,
            max_thumb_top,
        })
    }

    fn set_scrollbar_offset_from_pointer(
        scroll: &ScrollHandle,
        drag: ScrollbarDragState,
        pointer_y: f32,
    ) {
        let thumb_top =
            (pointer_y - drag.track_top - drag.thumb_grab_y).clamp(0.0, drag.max_thumb_top);
        let scroll_top = if drag.max_thumb_top <= 0.0 {
            0.0
        } else {
            thumb_top / drag.max_thumb_top * drag.max_offset
        };
        let current_offset = scroll.offset();
        scroll.set_offset(point(current_offset.x, px(-scroll_top)));
    }

    fn render_vertical_scrollbar(
        &self,
        kind: ScrollbarKind,
        scroll: &ScrollHandle,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let Some(metrics) = Self::scrollbar_metrics(scroll) else {
            return div()
                .absolute()
                .right(px(LAYOUT_ZERO))
                .top(px(LAYOUT_ZERO))
                .w(px(SCROLLBAR_HIDDEN_SIZE))
                .h(px(SCROLLBAR_HIDDEN_SIZE));
        };
        let entity = cx.entity();
        let scroll_handle = scroll.clone();
        let dragging = self
            .scrollbar_drag
            .map(|drag| drag.kind == kind)
            .unwrap_or(false);
        let track_bg = match kind {
            ScrollbarKind::Sidebar => ui_sidebar_pane(),
            ScrollbarKind::Request => ui_request_pane(),
            ScrollbarKind::Response => ui_response_pane(),
        };

        div()
            .absolute()
            .top(px(LAYOUT_ZERO))
            .right(px(LAYOUT_ZERO))
            .w(px(SCROLLBAR_GUTTER_WIDTH))
            .h_full()
            .border_l_1()
            .border_color(ui_border())
            .bg(if dragging { ui_hover() } else { track_bg })
            .cursor_pointer()
            .hover(|track| track.bg(ui_hover()))
            .child(
                div()
                    .absolute()
                    .right(px(SCROLLBAR_RIGHT_OFFSET))
                    .top(px(metrics.thumb_top))
                    .w(px(SCROLLBAR_WIDTH))
                    .h(px(metrics.thumb_height))
                    .rounded(px(SCROLLBAR_WIDTH / 2.))
                    .bg(if dragging {
                        ui_text_secondary()
                    } else {
                        ui_border_strong()
                    })
                    .hover(|thumb| thumb.bg(ui_text_muted())),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |track_bounds, _, window, _| {
                        window.on_mouse_event({
                            let entity = entity.clone();
                            let scroll_handle = scroll_handle.clone();
                            move |event: &MouseDownEvent, _, _, cx| {
                                if event.button != MouseButton::Left
                                    || !track_bounds.contains(&event.position)
                                {
                                    return;
                                }

                                let pointer_y = (event.position.y - track_bounds.origin.y).as_f32();
                                let thumb_bottom = metrics.thumb_top + metrics.thumb_height;
                                let pointer_in_thumb =
                                    pointer_y >= metrics.thumb_top && pointer_y <= thumb_bottom;
                                let thumb_grab_y = if pointer_in_thumb {
                                    pointer_y - metrics.thumb_top
                                } else {
                                    metrics.thumb_height / 2.
                                };
                                let drag = ScrollbarDragState {
                                    kind,
                                    track_top: track_bounds.origin.y.as_f32(),
                                    thumb_grab_y,
                                    max_thumb_top: metrics.max_thumb_top,
                                    max_offset: metrics.max_offset,
                                };

                                if !pointer_in_thumb {
                                    Self::set_scrollbar_offset_from_pointer(
                                        &scroll_handle,
                                        drag,
                                        event.position.y.as_f32(),
                                    );
                                }

                                entity.update(cx, |app, _| {
                                    app.scrollbar_drag = Some(drag);
                                });
                                cx.stop_propagation();
                                cx.notify(entity.entity_id());
                            }
                        });
                        window.on_mouse_event({
                            let entity = entity.clone();
                            move |_: &MouseUpEvent, _, _, cx| {
                                let should_clear = entity
                                    .read(cx)
                                    .scrollbar_drag
                                    .map(|drag| drag.kind == kind)
                                    .unwrap_or(false);
                                if should_clear {
                                    entity.update(cx, |app, _| {
                                        app.scrollbar_drag = None;
                                    });
                                    cx.stop_propagation();
                                    cx.notify(entity.entity_id());
                                }
                            }
                        });
                        window.on_mouse_event({
                            let entity = entity.clone();
                            let scroll_handle = scroll_handle.clone();
                            move |event: &MouseMoveEvent, _, _, cx| {
                                if !event.dragging() {
                                    let should_clear = entity
                                        .read(cx)
                                        .scrollbar_drag
                                        .map(|drag| drag.kind == kind)
                                        .unwrap_or(false);
                                    if should_clear {
                                        entity.update(cx, |app, _| {
                                            app.scrollbar_drag = None;
                                        });
                                        cx.stop_propagation();
                                        cx.notify(entity.entity_id());
                                    }
                                    return;
                                }

                                let Some(drag) = entity.read(cx).scrollbar_drag else {
                                    return;
                                };
                                if drag.kind != kind {
                                    return;
                                }

                                Self::set_scrollbar_offset_from_pointer(
                                    &scroll_handle,
                                    drag,
                                    event.position.y.as_f32(),
                                );
                                cx.stop_propagation();
                                cx.notify(entity.entity_id());
                            }
                        });
                    },
                )
                .size_full(),
            )
    }

    fn render_method_menu_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let items = HTTP_METHODS
            .iter()
            .map(|method| self.method_menu_item(*method, cx))
            .collect::<Vec<_>>();

        div()
            .absolute()
            .top(px(METHOD_MENU_TOP_OFFSET))
            .left(px(METHOD_MENU_LEFT_OFFSET))
            .w(px(METHOD_MENU_WIDTH))
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(ui_border_strong())
            .bg(ui_surface())
            .occlude()
            .children(items)
    }

    fn method_menu_item(&self, method: &'static str, cx: &mut Context<Self>) -> gpui::Div {
        let active = self.method == method;
        div()
            .flex()
            .items_center()
            .h(px(METHOD_MENU_ITEM_HEIGHT))
            .px_2()
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .text_size(px(REQUEST_PRIMARY_CONTROL_TEXT_SIZE))
            .text_color(if active {
                method_color(method)
            } else {
                ui_text_body()
            })
            .hover(|row| row.bg(ui_hover()))
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if can_select_request_method(app.busy) {
                        app.method = method.to_string();
                        app.method_menu_open = false;
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
        self.sized_action_button(label, ACTION_BUTTON_WIDTH, enabled, tone, on_click, cx)
    }

    fn sized_action_button(
        &self,
        label: impl Into<SharedString>,
        width: f32,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(ACTION_BUTTON_HEIGHT))
            .w(px(width))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_INPUT))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(ACTION_BUTTON_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if can_activate_render_enabled_control(enabled, app.busy) {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(label.into()))
    }

    fn top_bar_action_button(
        &self,
        label: impl Into<SharedString>,
        width: f32,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(TOP_BAR_ACTION_HEIGHT))
            .w(px(width))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(TOP_BAR_ACTION_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if can_activate_render_enabled_control(enabled, app.busy) {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(label.into()))
    }

    fn request_send_segment(&self, cx: &mut Context<Self>) -> gpui::Div {
        let enabled = can_send_request_shortcut(
            self.busy,
            &self.url.read(cx).text(),
            &self.pre_request_script.read(cx).text(),
        );

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(TEXT_INPUT_HEIGHT))
            .w(px(REQUEST_SEND_WIDTH))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .border_l_1()
            .border_color(ui_border())
            .bg(if enabled {
                ui_surface()
            } else {
                ui_disabled_surface()
            })
            .text_size(px(REQUEST_PRIMARY_CONTROL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(if enabled {
                ui_accent()
            } else {
                ui_disabled_text()
            })
            .when(enabled, |segment| segment.cursor_pointer())
            .hover(|segment| {
                if enabled {
                    segment.bg(ui_hover())
                } else {
                    segment
                }
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if can_send_request_shortcut(
                        app.busy,
                        &app.url.read(cx).text(),
                        &app.pre_request_script.read(cx).text(),
                    ) {
                        app.send_request(cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(REQUEST_SEND_LABEL))
    }

    fn render_top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_toggle_mock = self.server_running || !self.routes.is_empty();
        div()
            .relative()
            .flex()
            .items_center()
            .h(px(TOP_BAR_HEIGHT))
            .w_full()
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_app_chrome())
            .px_3()
            .gap_2()
            .child(
                div()
                    .w(px(TOP_BAR_BRAND_WIDTH))
                    .flex_shrink_0()
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(TOP_BAR_BRAND_TEXT_SIZE))
                    .text_color(ui_text_primary())
                    .child(APP_BRAND_LABEL),
            )
            .child(div().flex_1())
            .child(self.top_bar_action_button(
                TOP_BAR_IMPORT_LABEL,
                TOP_BAR_ACTION_WIDTH,
                can_toggle_import_popover(self.busy),
                ButtonTone::Neutral,
                |app, _event, _window, cx| {
                    if !can_toggle_import_popover(app.busy) {
                        return;
                    }
                    app.import_popover_open = !app.import_popover_open;
                    app.method_menu_open = false;
                    app.codegen_menu_open = false;
                    app.collection_context_menu = None;
                    cx.notify();
                },
                cx,
            ))
            .child(self.top_bar_action_button(
                mock_button_label(self.server_running),
                TOP_BAR_MOCK_ACTION_WIDTH,
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

    fn render_import_popover(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let can_open = text_input_has_text(&self.import_path, cx);

        div()
            .absolute()
            .top(px(IMPORT_POPOVER_TOP_OFFSET))
            .right(px(IMPORT_POPOVER_RIGHT_OFFSET))
            .w(px(IMPORT_POPOVER_WIDTH))
            .h(px(IMPORT_POPOVER_HEIGHT))
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(ui_border_strong())
            .bg(ui_surface())
            .occlude()
            .p(px(IMPORT_POPOVER_PADDING))
            .child(
                panel_button_row()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(bounded_text_input(self.import_path.clone())),
                    )
                    .child(self.action_button(
                        IMPORT_OPEN_LABEL,
                        can_open,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.import_openapi(cx),
                        cx,
                    )),
            )
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .h_full()
            .min_w_0()
            .bg(ui_sidebar_pane())
            .overflow_hidden()
            .child(self.render_sidebar_nav(cx))
            .child(self.render_sidebar_body(cx))
    }

    fn render_sidebar_nav(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let endpoints_count = filtered_count_label(self.routes.len(), self.visible_routes.len());
        let collection_count = collection_item_count(&self.collection.items).to_string();
        let history_total = self.history.entries().len();
        let history_count = filtered_count_label(
            history_total,
            self.history.filtered(&self.history_query).len(),
        );

        div()
            .flex()
            .items_center()
            .h(px(SIDEBAR_NAV_HEIGHT))
            .flex_shrink_0()
            .gap_1()
            .p_2()
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_sidebar_pane())
            .child(self.sidebar_section_button(
                SIDEBAR_ROUTES_LABEL,
                endpoints_count,
                SidebarSection::Endpoints,
                cx,
            ))
            .child(self.sidebar_section_button(
                SIDEBAR_SAVED_LABEL,
                collection_count,
                SidebarSection::Collections,
                cx,
            ))
            .child(self.sidebar_section_button(
                SIDEBAR_HISTORY_LABEL,
                history_count,
                SidebarSection::History,
                cx,
            ))
    }

    fn sidebar_section_button(
        &self,
        label: &'static str,
        count: String,
        section: SidebarSection,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let active = self.active_sidebar_section == section;

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(SIDEBAR_SECTION_BUTTON_HEIGHT))
            .flex_1()
            .min_w_0()
            .px_2()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(if active { ui_accent() } else { ui_border() })
            .bg(if active {
                ui_surface()
            } else {
                ui_sidebar_pane()
            })
            .text_size(px(SIDEBAR_NAV_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(if active { ui_accent() } else { ui_text_body() })
            .cursor_pointer()
            .hover(|button| {
                if active {
                    button
                } else {
                    button.bg(ui_hover())
                }
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.active_sidebar_section = section;
                    app.close_transient_layers();
                    cx.notify();
                }),
            )
            .child(div().min_w_0().truncate().child(label))
            .child(
                div()
                    .ml_1()
                    .flex_shrink_0()
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .text_color(if active { ui_accent() } else { ui_text_muted() })
                    .child(count),
            )
    }

    fn render_sidebar_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.active_sidebar_section {
            SidebarSection::Endpoints => self.render_endpoints_sidebar(cx).into_any(),
            SidebarSection::Collections => self.render_collection_section(cx).into_any(),
            SidebarSection::History => self.render_history_section(cx).into_any(),
        };

        div()
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .child(
                div()
                    .id("sidebar-scroll")
                    .h_full()
                    .min_w_0()
                    .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .track_scroll(&self.sidebar_scroll)
                    .child(body),
            )
            .child(self.render_vertical_scrollbar(ScrollbarKind::Sidebar, &self.sidebar_scroll, cx))
    }

    fn render_endpoints_sidebar(&self, cx: &mut Context<Self>) -> gpui::Div {
        let can_restore_request = can_restore_request_from_sidebar(self.busy);
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
                    can_restore_request,
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let count = filtered_count_label(self.routes.len(), self.visible_routes.len());

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .p_3()
            .gap_3()
            .child(sidebar_section_header(
                SIDEBAR_ROUTES_LABEL,
                sidebar_count_text(count),
            ))
            .child(bounded_text_input(self.route_filter.clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_1()
                    .children(rows)
                    .when(self.visible_routes.is_empty(), |list| {
                        list.child(empty_state_row(
                            if self.routes.is_empty() {
                                SIDEBAR_EMPTY_ROUTES_LABEL
                            } else {
                                SIDEBAR_EMPTY_MATCHES_LABEL
                            },
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    }),
            )
    }

    fn render_collection_section(&self, cx: &mut Context<Self>) -> gpui::Div {
        let can_export = can_export_collection(&self.collection);
        let can_restore_request = can_restore_request_from_sidebar(self.busy);
        let can_mutate_collection_tree = can_mutate_collection(self.busy);
        let has_collection_path = text_input_has_text(&self.collection_path, cx);
        let can_save_current_request = can_save_current_request_to_collection(
            &self.url.read(cx).text(),
            &self.pre_request_script.read(cx).text(),
        );
        let mut rows = vec![collection_root_row(
            self.collection.name.clone(),
            collection_item_count(&self.collection.items),
            self.expanded_collection_nodes
                .iter()
                .any(|node| node == "collection"),
            can_mutate_collection_tree,
            cx,
        )];

        if self
            .expanded_collection_nodes
            .iter()
            .any(|node| node == "collection")
        {
            append_collection_rows(
                &mut rows,
                &self.collection.items,
                "collection",
                1,
                &self.expanded_collection_nodes,
                can_restore_request,
                can_mutate_collection_tree,
                cx,
            );
        }

        div()
            .relative()
            .flex()
            .flex_col()
            .min_w_0()
            .p_3()
            .gap_3()
            .child(sidebar_section_header(
                SIDEBAR_SAVED_LABEL,
                sidebar_status_text(
                    collection_sidebar_status_label(&self.collection_status).unwrap_or_default(),
                ),
            ))
            .child(bounded_text_input(self.collection_path.clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .overflow_hidden()
                    .gap_2()
                    .child(
                        panel_button_row()
                            .child(self.sidebar_fluid_button(
                                COLLECTION_IMPORT_LABEL,
                                has_collection_path,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.import_collection(cx),
                                cx,
                            ))
                            .child(self.sidebar_fluid_button(
                                COLLECTION_SAVE_LABEL,
                                can_save_current_request,
                                ButtonTone::Primary,
                                |app, _event, _window, cx| {
                                    app.save_current_request_to_collection(cx)
                                },
                                cx,
                            )),
                    )
                    .child(
                        panel_button_row()
                            .child(self.sidebar_fluid_button(
                                COLLECTION_EXPORT_LABEL,
                                can_export && has_collection_path,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.export_collection(false, cx),
                                cx,
                            ))
                            .child(self.sidebar_fluid_button(
                                COLLECTION_POSTMAN_LABEL,
                                can_export && has_collection_path,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.export_collection(true, cx),
                                cx,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_1()
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_1()
                    .children(rows)
                    .when(self.collection.items.is_empty(), |list| {
                        list.child(empty_state_row(
                            SIDEBAR_EMPTY_SAVED_LABEL,
                            COLLECTION_TREE_ROOT_ROW_HEIGHT,
                        ))
                    }),
            )
            .when(self.collection_context_menu.is_some(), |section| {
                let menu = self
                    .collection_context_menu
                    .clone()
                    .expect("checked collection context menu");
                section.child(self.render_collection_context_menu(menu, cx))
            })
    }

    fn render_collection_context_menu(
        &self,
        menu: CollectionContextMenu,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let can_create_child = can_use_collection_context_action(self.busy, true);
        let can_mutate_item =
            can_use_collection_context_action(self.busy, menu.kind != CollectionNodeKind::Root);
        let can_rename = can_submit_collection_rename(
            self.busy,
            Some(menu.kind),
            &self.collection_rename_input.read(cx).text(),
        );
        let new_request_target = menu.node_id.clone();
        let new_folder_target = menu.node_id.clone();
        let copy_target = menu.node_id.clone();
        let delete_target = menu.node_id.clone();

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(ui_border_strong())
            .bg(ui_surface())
            .p_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(PANEL_META_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_body())
                            .child(menu.label),
                    )
                    .child(
                        sidebar_small_button(
                            COLLECTION_MENU_CLOSE_LABEL,
                            28.,
                            22.,
                            true,
                            ButtonTone::Neutral,
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                                app.close_collection_menu(cx);
                            }),
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .overflow_hidden()
                    .gap_2()
                    .child(
                        panel_button_row()
                            .child(self.sidebar_fluid_button(
                                COLLECTION_MENU_NEW_REQUEST_LABEL,
                                can_create_child,
                                ButtonTone::Neutral,
                                move |app, _event, _window, cx| {
                                    app.add_collection_request(new_request_target.clone(), cx);
                                },
                                cx,
                            ))
                            .child(self.sidebar_fluid_button(
                                COLLECTION_MENU_NEW_FOLDER_LABEL,
                                can_create_child,
                                ButtonTone::Neutral,
                                move |app, _event, _window, cx| {
                                    app.add_collection_folder(new_folder_target.clone(), cx);
                                },
                                cx,
                            )),
                    )
                    .child(
                        panel_button_row()
                            .child(self.sidebar_fluid_button(
                                COLLECTION_MENU_COPY_LABEL,
                                can_mutate_item,
                                ButtonTone::Neutral,
                                move |app, _event, _window, cx| {
                                    app.copy_collection_target(copy_target.clone(), cx);
                                },
                                cx,
                            ))
                            .child(self.sidebar_fluid_button(
                                COLLECTION_MENU_DELETE_LABEL,
                                can_mutate_item,
                                ButtonTone::Warning,
                                move |app, _event, _window, cx| {
                                    app.delete_collection_target(delete_target.clone(), cx);
                                },
                                cx,
                            )),
                    ),
            )
            .child(
                panel_button_row()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(bounded_text_input(self.collection_rename_input.clone())),
                    )
                    .child(self.sidebar_action_button(
                        COLLECTION_MENU_RENAME_LABEL,
                        70.,
                        can_rename,
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.rename_collection_target(cx),
                        cx,
                    )),
            )
    }

    fn sidebar_action_button(
        &self,
        label: &'static str,
        width: f32,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(SIDEBAR_BUTTON_HEIGHT))
            .w(px(width))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(SIDEBAR_ACTION_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if can_activate_render_enabled_control(enabled, app.busy) {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(label))
    }

    fn sidebar_fluid_button(
        &self,
        label: &'static str,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(SIDEBAR_BUTTON_HEIGHT))
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(SIDEBAR_ACTION_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if can_activate_render_enabled_control(enabled, app.busy) {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(label))
    }

    fn panel_action_button(
        &self,
        label: &'static str,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(ACTION_BUTTON_HEIGHT))
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_INPUT))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(ACTION_BUTTON_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if can_activate_render_enabled_control(enabled, app.busy) {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(label))
    }

    fn render_history_section(&self, cx: &mut Context<Self>) -> gpui::Div {
        let filtered_entries = self.history.filtered(&self.history_query);
        let can_restore_request = can_restore_request_from_sidebar(self.busy);
        let history_total = self.history.entries().len();
        let visible_count = filtered_entries.len();
        let has_history = history_total > 0;
        let has_matches = visible_count > 0;
        let count = filtered_count_label(history_total, visible_count);
        let clear_enabled = can_clear_history(self.busy, history_total);
        let delete_enabled = can_delete_history_entry(self.busy);
        let rows = filtered_entries
            .into_iter()
            .map(|entry| {
                history_row(
                    entry.id,
                    entry.request.method.clone(),
                    entry.request.url.clone(),
                    entry.response.status.clone(),
                    can_restore_request,
                    delete_enabled,
                    cx,
                )
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .p_3()
            .gap_3()
            .child(sidebar_section_header(
                SIDEBAR_HISTORY_LABEL,
                panel_button_row()
                    .justify_end()
                    .child(sidebar_count_text(count))
                    .child(
                        sidebar_small_button(
                            "Clear",
                            58.,
                            SECTION_HEADER_HEIGHT,
                            clear_enabled,
                            ButtonTone::Neutral,
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                                if can_clear_history(app.busy, app.history.entries().len()) {
                                    app.history.clear();
                                    cx.notify();
                                }
                            }),
                        ),
                    ),
            ))
            .child(bounded_text_input(self.history_filter.clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_1()
                    .children(rows)
                    .when(!has_history, |list| {
                        list.child(empty_state_row(
                            SIDEBAR_EMPTY_HISTORY_LABEL,
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    })
                    .when(has_history && !has_matches, |list| {
                        list.child(empty_state_row(
                            SIDEBAR_EMPTY_MATCHES_LABEL,
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    }),
            )
    }

    fn render_route_row(
        &self,
        index: usize,
        method: String,
        path: String,
        summary: String,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + 'static + use<> {
        let selected = self.selected_route == Some(index);
        let method_color = method_color(&method);

        div()
            .id(("route", index))
            .flex()
            .flex_col()
            .h(px(ROUTE_ROW_HEIGHT))
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_TIGHT))
            .border_l(px(ROUTE_SELECTED_MARKER_WIDTH))
            .border_color(if selected {
                ui_accent()
            } else {
                ui_surface_muted()
            })
            .bg(if selected {
                ui_surface()
            } else {
                ui_surface_muted()
            })
            .px_2()
            .py_1()
            .opacity(control_opacity(enabled))
            .when(enabled, |row| row.cursor_pointer())
            .hover(move |row| {
                if enabled && !selected {
                    row.bg(ui_hover())
                } else {
                    row
                }
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if can_restore_request_from_sidebar(app.busy) {
                        app.select_route(index, cx);
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap_2()
                    .child(
                        fixed_row_cell(method, HTTP_METHOD_LABEL_WIDTH, method_color, true, false)
                            .text_size(px(SIDEBAR_METHOD_TEXT_SIZE)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(SIDEBAR_PRIMARY_ROW_TEXT_SIZE))
                            .text_color(ui_text_primary())
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .child(path),
                    ),
            )
            .child(
                div()
                    .ml(px(SIDEBAR_SECONDARY_ROW_INDENT))
                    .min_w_0()
                    .truncate()
                    .text_size(px(PANEL_META_TEXT_SIZE))
                    .text_color(ui_sidebar_detail_text())
                    .child(summary),
            )
    }

    fn render_workspace(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (sidebar_ratio, request_ratio, response_ratio) = self.workspace_ratios();

        div()
            .relative()
            .flex()
            .flex_row()
            .flex_1()
            .h_full()
            .bg(ui_workspace_gutter())
            .overflow_hidden()
            .on_drag_move::<WorkspaceSplitDrag>(cx.listener(Self::resize_workspace_split))
            .capture_any_mouse_up(cx.listener(|app, event: &MouseUpEvent, _window, cx| {
                if event.button == MouseButton::Left && app.finish_workspace_split_resize(cx) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                    if app.finish_workspace_split_resize(cx) {
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                div()
                    .h_full()
                    .min_w_0()
                    .flex_shrink_1()
                    .w(DefiniteLength::Fraction(sidebar_ratio))
                    .overflow_hidden()
                    .child(self.render_sidebar(cx)),
            )
            .child(self.workspace_split_handle(WorkspaceSplitDrag::SidebarRequest, cx))
            .child(
                div()
                    .h_full()
                    .min_w_0()
                    .flex_shrink_1()
                    .w(DefiniteLength::Fraction(request_ratio))
                    .overflow_hidden()
                    .child(self.render_request_panel(window, cx)),
            )
            .child(self.workspace_split_handle(WorkspaceSplitDrag::RequestResponse, cx))
            .child(
                div()
                    .h_full()
                    .min_w_0()
                    .flex_shrink_1()
                    .w(DefiniteLength::Fraction(response_ratio))
                    .overflow_hidden()
                    .child(self.render_response_panel(cx)),
            )
    }

    fn render_request_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let address_enabled = !self.busy;
        let url_focused = address_enabled && self.url.read(cx).focus_handle(cx).is_focused(window);
        let address_border = request_address_border_color(request_address_border_tone(
            address_enabled,
            url_focused,
            self.method_menu_open,
        ));
        let address_background = if address_enabled {
            ui_surface()
        } else {
            ui_disabled_surface()
        };

        div()
            .flex()
            .items_center()
            .h(px(REQUEST_BAR_HEIGHT))
            .min_w_0()
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_request_pane())
            .px_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .h(px(TEXT_INPUT_HEIGHT))
                    .rounded(px(REQUEST_ADDRESS_RADIUS))
                    .border(px(TEXT_INPUT_BORDER_WIDTH))
                    .border_color(address_border)
                    .bg(address_background)
                    .overflow_hidden()
                    .child(self.method_selector(cx))
                    .child(
                        div()
                            .h(px(REQUEST_ADDRESS_DIVIDER_HEIGHT))
                            .w(px(REQUEST_ADDRESS_DIVIDER_WIDTH))
                            .bg(ui_border()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(bounded_text_input(self.url.clone())),
                    )
                    .child(self.request_send_segment(cx)),
            )
    }

    fn render_request_editor_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = request_editor_tabs()
            .into_iter()
            .map(|(label, tab)| self.request_editor_tab(label, tab, cx))
            .collect::<Vec<_>>();

        div()
            .flex()
            .items_center()
            .h(px(REQUEST_EDITOR_TAB_BAR_HEIGHT))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_request_tab_bar())
            .px_2()
            .children(tabs)
    }

    fn request_editor_tab(
        &self,
        label: &'static str,
        tab: RequestPaneTab,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let active = self.active_request_tab == tab;

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_between()
            .h_full()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .px_1()
            .text_size(px(REQUEST_EDITOR_TAB_TEXT_SIZE))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if active { ui_accent() } else { ui_text_body() })
            .cursor_pointer()
            .hover(|tab| if active { tab } else { tab.bg(ui_hover()) })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.select_request_tab(tab, cx);
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .min_w_0()
                    .truncate()
                    .child(label),
            )
            .child(
                div()
                    .h(px(PANEL_HEADER_UNDERLINE_HEIGHT))
                    .w_full()
                    .bg(if active {
                        ui_accent()
                    } else {
                        ui_request_tab_bar()
                    }),
            )
    }

    fn render_request_tab_body(&self, cx: &mut Context<Self>) -> gpui::Div {
        let active_tab = self.active_request_tab;

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap_3()
            .when(active_tab == RequestPaneTab::Params, |body| {
                body.child(self.render_variables_panel(cx))
                    .child(self.key_value_editor(
                        PARAMS_PANEL_TITLE,
                        &self.query_params,
                        KeyValueEditorTarget::QueryParams,
                        cx,
                    ))
            })
            .when(active_tab == RequestPaneTab::Headers, |body| {
                body.child(self.render_headers_editor(cx))
            })
            .when(active_tab == RequestPaneTab::Auth, |body| {
                body.child(self.render_auth_panel(cx))
            })
            .when(active_tab == RequestPaneTab::Body, |body| {
                body.child(self.render_body_panel(cx))
            })
            .when(active_tab == RequestPaneTab::Scripts, |body| {
                body.child(self.render_pre_request_panel())
                    .child(self.render_tests_panel(cx))
            })
            .when(active_tab == RequestPaneTab::Realtime, |body| {
                body.child(self.render_websocket_panel(cx))
                    .child(self.render_sse_panel(cx))
            })
            .when(active_tab == RequestPaneTab::Tools, |body| {
                body.child(self.render_codegen_panel(cx))
                    .child(self.render_collection_runner(cx))
                    .child(self.render_mock_log())
            })
    }

    fn render_request_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(ui_request_pane())
            .child(panel_header("Request", None, ResponseTone::Neutral))
            .child(self.render_request_bar(window, cx))
            .child(self.render_request_editor_tabs(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("request-pane-scroll")
                            .flex()
                            .flex_col()
                            .h_full()
                            .min_w_0()
                            .bg(ui_request_pane())
                            .p_3()
                            .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                            .gap_3()
                            .overflow_y_scroll()
                            .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                            .track_scroll(&self.request_scroll)
                            .child(self.render_request_tab_body(cx)),
                    )
                    .child(self.render_vertical_scrollbar(
                        ScrollbarKind::Request,
                        &self.request_scroll,
                        cx,
                    )),
            )
            .when(self.method_menu_open && !self.busy, |panel| {
                panel.child(self.render_method_menu_overlay(cx))
            })
    }

    fn render_websocket_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let websocket_url = self.websocket_url.read(cx).text();
        let can_connect_websocket =
            can_connect_websocket(self.busy, self.websocket_running, &websocket_url);
        let websocket_message_text = self.websocket_message.read(cx).text();
        let can_send_websocket = can_send_websocket_message(
            self.busy,
            self.websocket_running,
            self.websocket_message_mode,
            &websocket_message_text,
        );
        let rows = self
            .websocket_messages
            .iter()
            .rev()
            .cloned()
            .map(websocket_log_row)
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(REALTIME_WEBSOCKET_TITLE),
                    )
                    .when_some(
                        realtime_header_status_label(&self.websocket_status),
                        |header, status| {
                            header.child(panel_status_text(
                                status,
                                if self.websocket_running {
                                    ResponseTone::Busy.color()
                                } else {
                                    ui_text_body()
                                },
                            ))
                        },
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_2()
                    .child(bounded_text_input(self.websocket_url.clone()))
                    .child(
                        panel_button_row()
                            .child(self.panel_action_button(
                                REALTIME_WEBSOCKET_CONNECT_LABEL,
                                can_connect_websocket,
                                ButtonTone::Primary,
                                |app, _event, _window, cx| app.connect_websocket(cx),
                                cx,
                            ))
                            .child(self.panel_action_button(
                                REALTIME_WEBSOCKET_SEND_LABEL,
                                can_send_websocket,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.send_websocket_message(cx),
                                cx,
                            ))
                            .child(self.panel_action_button(
                                REALTIME_WEBSOCKET_CLOSE_LABEL,
                                can_close_websocket(self.busy, self.websocket_running),
                                ButtonTone::Warning,
                                |app, _event, _window, cx| app.close_websocket(cx),
                                cx,
                            )),
                    ),
            )
            .child(bounded_text_input(self.websocket_protocols.clone()))
            .child(self.key_value_editor(
                REALTIME_WEBSOCKET_HEADERS_TITLE,
                &self.websocket_headers,
                KeyValueEditorTarget::WebSocketHeaders,
                cx,
            ))
            .child(
                div().flex().flex_col().min_w_0().gap_2().child(
                    panel_button_row()
                        .child(self.websocket_message_mode_button(
                            REALTIME_WEBSOCKET_TEXT_LABEL,
                            WebSocketMessageMode::Text,
                            cx,
                        ))
                        .child(self.websocket_message_mode_button(
                            REALTIME_WEBSOCKET_BINARY_LABEL,
                            WebSocketMessageMode::BinaryHex,
                            cx,
                        )),
                ),
            )
            .child(bounded_text_input(self.websocket_message.clone()))
            .child(
                panel_button_row()
                    .child(self.panel_action_button(
                        "Copy",
                        can_use_realtime_log_actions(self.busy, self.websocket_messages.len()),
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.copy_websocket_log(cx),
                        cx,
                    ))
                    .child(self.panel_action_button(
                        "Clear",
                        can_use_realtime_log_actions(self.busy, self.websocket_messages.len()),
                        ButtonTone::Warning,
                        |app, _event, _window, cx| app.clear_websocket_log(cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .overflow_hidden()
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.websocket_messages.is_empty(), |list| {
                        list.child(empty_state_row(
                            REALTIME_WEBSOCKET_EMPTY_LABEL,
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    }),
            )
    }

    fn render_sse_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sse_url = self.sse_url.read(cx).text();
        let can_start_sse = can_start_sse(self.busy, self.sse_running, &sse_url);
        let rows = self
            .sse_events
            .iter()
            .rev()
            .cloned()
            .map(sse_log_row)
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(REALTIME_SSE_TITLE),
                    )
                    .when_some(
                        realtime_header_status_label(&self.sse_status),
                        |header, status| {
                            header.child(panel_status_text(
                                status,
                                if self.sse_running {
                                    ResponseTone::Busy.color()
                                } else {
                                    ui_text_body()
                                },
                            ))
                        },
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_2()
                    .child(bounded_text_input(self.sse_url.clone()))
                    .child(
                        panel_button_row()
                            .child(self.panel_action_button(
                                REALTIME_SSE_FETCH_LABEL,
                                can_start_sse,
                                ButtonTone::Primary,
                                |app, _event, _window, cx| app.fetch_sse_events(cx),
                                cx,
                            ))
                            .child(self.panel_action_button(
                                REALTIME_SSE_SUBSCRIBE_LABEL,
                                can_start_sse,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.subscribe_sse_events(cx),
                                cx,
                            ))
                            .child(self.panel_action_button(
                                REALTIME_SSE_STOP_LABEL,
                                can_stop_sse_subscription(
                                    self.busy,
                                    self.sse_subscription.is_some(),
                                ),
                                ButtonTone::Warning,
                                |app, _event, _window, cx| app.stop_sse_subscription(cx),
                                cx,
                            )),
                    ),
            )
            .child(self.key_value_editor(
                REALTIME_SSE_HEADERS_TITLE,
                &self.sse_headers,
                KeyValueEditorTarget::SseHeaders,
                cx,
            ))
            .child(
                panel_button_row()
                    .child(self.panel_action_button(
                        "Copy",
                        can_use_realtime_log_actions(self.busy, self.sse_events.len()),
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.copy_sse_log(cx),
                        cx,
                    ))
                    .child(self.panel_action_button(
                        "Clear",
                        can_use_realtime_log_actions(self.busy, self.sse_events.len()),
                        ButtonTone::Warning,
                        |app, _event, _window, cx| app.clear_sse_log(cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .overflow_hidden()
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.sse_events.is_empty(), |list| {
                        list.child(empty_state_row(
                            REALTIME_SSE_EMPTY_LABEL,
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    }),
            )
    }

    fn render_tests_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let result_rows = self
            .last_assertion_results
            .iter()
            .cloned()
            .map(assertion_result_row)
            .collect::<Vec<_>>();
        let editor_rows = self
            .request_assertions
            .iter()
            .enumerate()
            .map(|(index, row)| {
                assertion_editor_row(index, row, self.request_assertions.len(), self.busy, cx)
            })
            .collect::<Vec<_>>();
        let meta = tests_header_status_label(
            &self.last_assertion_results,
            configured_assertion_count(&self.request_assertions, cx),
        );

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(TESTS_PANEL_TITLE),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .min_w_0()
                            .gap_2()
                            .when_some(meta, |actions, meta| {
                                actions.child(panel_status_text(meta, ui_text_body()))
                            })
                            .when(!self.last_assertion_results.is_empty(), |actions| {
                                actions.child(self.response_assertions_clear_button(cx))
                            })
                            .child(self.response_assertions_add_button(cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .min_w_0()
                    .gap_2()
                    .px_2()
                    .text_size(px(TABLE_HEADER_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_body())
                    .child(
                        div()
                            .w(px(TEST_ASSERTION_NAME_COLUMN_WIDTH))
                            .flex_shrink_0()
                            .child(TEST_ASSERTION_NAME_HEADER),
                    )
                    .child(
                        div()
                            .w(px(TEST_ASSERTION_KIND_COLUMN_WIDTH))
                            .flex_shrink_0()
                            .child(TEST_ASSERTION_KIND_HEADER),
                    )
                    .child(div().flex_1().min_w_0().child(TEST_ASSERTION_TARGET_HEADER))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(TEST_ASSERTION_EXPECTED_HEADER),
                    )
                    .child(
                        div()
                            .w(px(KEY_VALUE_ROW_ACTION_BUTTON_WIDTH))
                            .flex_shrink_0(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_1()
                    .children(editor_rows),
            )
            .when(!self.last_assertion_results.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .overflow_hidden()
                        .rounded(px(UI_RADIUS_TIGHT))
                        .border_1()
                        .border_color(ui_border())
                        .bg(ui_surface())
                        .children(result_rows),
                )
            })
    }

    fn response_assertions_add_button(&self, cx: &mut Context<Self>) -> gpui::Div {
        self.compact_action_button(
            "+",
            KEY_VALUE_ROW_ACTION_BUTTON_WIDTH,
            can_edit_request_configuration(self.busy),
            ButtonTone::Neutral,
            |app, _event, _window, cx| app.add_response_assertion_row(cx),
            cx,
        )
    }

    fn response_assertions_clear_button(&self, cx: &mut Context<Self>) -> gpui::Div {
        self.compact_action_button(
            "Clear",
            TESTS_CLEAR_RESULTS_BUTTON_WIDTH,
            can_clear_response_assertion_results(self.busy, self.last_assertion_results.len()),
            ButtonTone::Neutral,
            |app, _event, _window, cx| app.clear_response_assertion_results(cx),
            cx,
        )
    }

    fn compact_action_button(
        &self,
        label: &'static str,
        width: f32,
        enabled: bool,
        tone: ButtonTone,
        on_click: impl Fn(&mut Self, &MouseUpEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = enabled && !self.busy;
        let colors = tone.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(COMPACT_CONTROL_HEIGHT))
            .w(px(width))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(COMPACT_SYMBOL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, event, window, cx| {
                    if can_activate_render_enabled_control(enabled, app.busy) {
                        on_click(app, event, window, cx);
                    }
                }),
            )
            .child(div().min_w_0().truncate().child(label))
    }

    fn render_collection_runner(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let total = collection_item_count(&self.collection.items);
        let stop_toggle_enabled = runner_stop_toggle_enabled(self.runner_running, self.busy);
        let rows = self
            .runner_results
            .iter()
            .rev()
            .cloned()
            .map(runner_result_row)
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(RUNNER_PANEL_TITLE),
                    )
                    .when_some(
                        runner_header_status_label(&self.runner_status),
                        |header, status| {
                            header.child(panel_status_text(
                                status,
                                if self.runner_running {
                                    ResponseTone::Busy.color()
                                } else {
                                    ui_text_body()
                                },
                            ))
                        },
                    ),
            )
            .child(
                panel_button_row()
                    .child(
                        flexible_toggle_enabled(
                            RUNNER_STOP_ON_FAILURE_LABEL,
                            self.runner_stop_on_failure,
                            stop_toggle_enabled,
                        )
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                                if runner_stop_toggle_enabled(app.runner_running, app.busy) {
                                    app.runner_stop_on_failure = !app.runner_stop_on_failure;
                                    cx.notify();
                                }
                            }),
                        ),
                    )
                    .child(self.panel_action_button(
                        RUNNER_RUN_ALL_LABEL,
                        can_run_collection_runner(total, self.runner_running, self.busy),
                        ButtonTone::Primary,
                        |app, _event, _window, cx| app.run_collection_runner(cx),
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .overflow_hidden()
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.runner_results.is_empty(), |list| {
                        list.child(empty_state_row(
                            if total == 0 {
                                RUNNER_EMPTY_REQUESTS_LABEL
                            } else {
                                RUNNER_EMPTY_RESULTS_LABEL
                            },
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    }),
            )
    }

    fn render_mock_log(&self) -> impl IntoElement {
        let rows = self
            .mock_logs
            .iter()
            .rev()
            .map(|entry| mock_log_row(entry.method.clone(), entry.path.clone(), entry.status))
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(MOCK_LOG_TITLE),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .children(rows)
                    .when(self.mock_logs.is_empty(), |list| {
                        list.child(empty_state_row(
                            MOCK_LOG_EMPTY_LABEL,
                            EMPTY_STATE_ROW_HEIGHT,
                        ))
                    }),
            )
    }

    fn render_headers_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let has_request_headers = has_key_value_rows(&self.request_headers, cx);
        let can_edit_request = can_edit_request_configuration(self.busy);
        let can_copy_bulk_headers = can_copy_headers_bulk(self.busy, has_request_headers);

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(self.key_value_editor(
                HEADERS_PANEL_TITLE,
                &self.request_headers,
                KeyValueEditorTarget::RequestHeaders,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_2()
                    .child(
                        panel_button_row()
                            .child(self.panel_action_button(
                                HEADER_COPY_BULK_LABEL,
                                can_copy_bulk_headers,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.copy_headers_bulk(cx),
                                cx,
                            ))
                            .child(self.panel_action_button(
                                HEADER_PASTE_BULK_LABEL,
                                can_edit_request,
                                ButtonTone::Primary,
                                |app, _event, _window, cx| app.paste_headers_bulk(cx),
                                cx,
                            )),
                    )
                    .child(
                        panel_button_row()
                            .child(self.panel_action_button(
                                HEADER_ACCEPT_JSON_LABEL,
                                can_edit_request,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| {
                                    app.apply_header_preset("Accept", "application/json", cx)
                                },
                                cx,
                            ))
                            .child(self.panel_action_button(
                                HEADER_CONTENT_JSON_LABEL,
                                can_edit_request,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| {
                                    app.apply_header_preset("Content-Type", "application/json", cx)
                                },
                                cx,
                            ))
                            .child(self.panel_action_button(
                                HEADER_BEARER_AUTH_LABEL,
                                can_edit_request,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| {
                                    app.apply_header_preset("Authorization", "Bearer {{token}}", cx)
                                },
                                cx,
                            )),
                    ),
            )
    }

    fn render_variables_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let environment_buttons = self
            .environments
            .iter()
            .map(|environment| self.environment_button(environment.name.clone(), cx))
            .collect::<Vec<_>>();
        let active_environment = self.active_environment.as_deref().unwrap_or("none");
        let can_add_environment =
            can_submit_environment_add(self.busy, &self.environment_name_input.read(cx).text());

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(VARIABLES_PANEL_TITLE),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(PANEL_META_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_body())
                            .child(VARIABLES_ENV_LABEL),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(PANEL_META_TEXT_SIZE))
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_color(ui_text_body())
                            .child(active_environment.to_string()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_1()
                    .child(self.environment_button(VARIABLES_NO_ENV_LABEL.to_string(), cx))
                    .children(environment_buttons),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_2()
                    .child(bounded_text_input(self.environment_name_input.clone()))
                    .child(
                        panel_button_row()
                            .child(self.panel_action_button(
                                VARIABLES_ADD_ENV_LABEL,
                                can_add_environment,
                                ButtonTone::Neutral,
                                |app, _event, _window, cx| app.add_environment(cx),
                                cx,
                            ))
                            .child(self.panel_action_button(
                                VARIABLES_DELETE_ENV_LABEL,
                                can_delete_environment(
                                    self.busy,
                                    self.active_environment.is_some(),
                                ),
                                ButtonTone::Warning,
                                |app, _event, _window, cx| app.delete_active_environment(cx),
                                cx,
                            )),
                    ),
            )
            .child(self.key_value_editor(
                VARIABLES_GLOBAL_TITLE,
                &self.global_variables,
                KeyValueEditorTarget::GlobalVariables,
                cx,
            ))
            .when_some(self.active_environment_variables(), |panel, variables| {
                panel.child(self.key_value_editor(
                    VARIABLES_ENV_TITLE,
                    variables,
                    KeyValueEditorTarget::ActiveEnvironmentVariables,
                    cx,
                ))
            })
    }

    fn environment_button(&self, label: String, cx: &mut Context<Self>) -> gpui::Div {
        let environment = if label == VARIABLES_NO_ENV_LABEL {
            None
        } else {
            Some(label.clone())
        };
        let active = self.active_environment == environment;
        let enabled = can_edit_request_configuration(self.busy);

        full_width_toggle_enabled(label, active, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_edit_request_configuration(app.busy) {
                    app.active_environment = environment.clone();
                    cx.notify();
                }
            }),
        )
    }

    fn render_auth_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(AUTH_PANEL_TITLE),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_2()
                    .child(
                        panel_button_row()
                            .child(self.auth_mode_button(AUTH_NONE_LABEL, AuthMode::None, cx))
                            .child(self.auth_mode_button(AUTH_BEARER_LABEL, AuthMode::Bearer, cx))
                            .child(self.auth_mode_button(AUTH_OAUTH_LABEL, AuthMode::OAuth2, cx)),
                    )
                    .child(
                        panel_button_row()
                            .child(self.auth_mode_button(AUTH_BASIC_LABEL, AuthMode::Basic, cx))
                            .child(self.auth_mode_button(AUTH_JWT_LABEL, AuthMode::Jwt, cx))
                            .child(self.auth_mode_button(AUTH_API_KEY_LABEL, AuthMode::ApiKey, cx)),
                    ),
            )
            .when(self.auth_mode == AuthMode::Bearer, |panel| {
                panel.child(bounded_text_input(self.bearer_token.clone()))
            })
            .when(self.auth_mode == AuthMode::OAuth2, |panel| {
                panel.child(bounded_text_input(self.oauth2_access_token.clone()))
            })
            .when(self.auth_mode == AuthMode::Basic, |panel| {
                panel.child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .overflow_hidden()
                        .gap_2()
                        .child(
                            div()
                                .w(px(KEY_VALUE_KEY_COLUMN_WIDTH))
                                .flex_shrink_0()
                                .child(bounded_text_input(self.basic_username.clone())),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(bounded_text_input(self.basic_password.clone())),
                        ),
                )
            })
            .when(self.auth_mode == AuthMode::Jwt, |panel| {
                panel.child(bounded_text_input(self.jwt_token.clone()))
            })
            .when(self.auth_mode == AuthMode::ApiKey, |panel| {
                panel
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .min_w_0()
                            .overflow_hidden()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(KEY_VALUE_KEY_COLUMN_WIDTH))
                                    .flex_shrink_0()
                                    .child(bounded_text_input(self.api_key_name.clone())),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(bounded_text_input(self.api_key_value.clone())),
                            ),
                    )
                    .child(
                        panel_button_row()
                            .child(self.api_key_placement_button(
                                AUTH_API_KEY_HEADER_LABEL,
                                ApiKeyPlacement::Header,
                                cx,
                            ))
                            .child(self.api_key_placement_button(
                                AUTH_API_KEY_QUERY_LABEL,
                                ApiKeyPlacement::Query,
                                cx,
                            )),
                    )
            })
    }

    fn render_pre_request_panel(&self) -> impl IntoElement {
        let action_rows = self
            .last_pre_request_actions
            .iter()
            .cloned()
            .map(pre_request_action_row)
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(PRE_REQUEST_PANEL_TITLE),
                    )
                    .when_some(
                        panel_header_status_label(&self.pre_request_status),
                        |header, status| {
                            header.child(panel_status_text(
                                status,
                                if self.pre_request_status.starts_with("error") {
                                    ResponseTone::Error.color()
                                } else {
                                    ui_text_body()
                                },
                            ))
                        },
                    ),
            )
            .child(bounded_text_input(self.pre_request_script.clone()))
            .when(!self.last_pre_request_actions.is_empty(), |panel| {
                panel.child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .overflow_hidden()
                        .rounded(px(UI_RADIUS_TIGHT))
                        .border_1()
                        .border_color(ui_border())
                        .bg(ui_surface())
                        .children(action_rows),
                )
            })
    }

    fn render_body_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(div().flex().flex_col().min_w_0().gap_2().children(
                request_body_mode_rows().map(|row| {
                    let [first, second, third] = row;
                    panel_button_row()
                        .child(self.body_mode_button(first.0, first.1, cx))
                        .child(self.body_mode_button(second.0, second.1, cx))
                        .child(self.body_mode_button(third.0, third.1, cx))
                }),
            ))
            .when(
                self.request_body_mode == RequestBodyMode::FormData,
                |panel| {
                    panel.child(self.key_value_editor(
                        request_body_editor_title(RequestBodyMode::FormData).unwrap_or("Form"),
                        &self.form_data_body,
                        KeyValueEditorTarget::FormDataBody,
                        cx,
                    ))
                },
            )
            .when(
                self.request_body_mode == RequestBodyMode::UrlEncoded,
                |panel| {
                    panel.child(
                        self.key_value_editor(
                            request_body_editor_title(RequestBodyMode::UrlEncoded)
                                .unwrap_or(BODY_URL_ENCODED_TITLE),
                            &self.urlencoded_body,
                            KeyValueEditorTarget::UrlEncodedBody,
                            cx,
                        ),
                    )
                },
            )
            .when(self.request_body_mode == RequestBodyMode::Raw, |panel| {
                panel
                    .child(
                        panel_button_row()
                            .child(self.raw_format_button(
                                RAW_FORMAT_JSON_MODE_LABEL,
                                RawBodyFormat::Json,
                                cx,
                            ))
                            .child(self.raw_format_button(
                                RAW_FORMAT_XML_MODE_LABEL,
                                RawBodyFormat::Xml,
                                cx,
                            ))
                            .child(self.raw_format_button(
                                RAW_FORMAT_TEXT_MODE_LABEL,
                                RawBodyFormat::Text,
                                cx,
                            ))
                            .child(self.raw_format_button(
                                RAW_FORMAT_HTML_MODE_LABEL,
                                RawBodyFormat::Html,
                                cx,
                            )),
                    )
                    .child(panel_button_row().child(self.panel_action_button(
                        RAW_FORMAT_JSON_LABEL,
                        can_format_request_raw_json(
                            self.busy,
                            self.raw_body_format,
                            &self.request_body.read(cx).text(),
                        ),
                        ButtonTone::Neutral,
                        |app, _event, _window, cx| app.format_raw_json_body(cx),
                        cx,
                    )))
                    .child(bounded_text_input(self.request_body.clone()))
                    .child(self.render_raw_body_preview(cx))
            })
            .when(
                self.request_body_mode == RequestBodyMode::GraphQL,
                |panel| {
                    panel
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .min_w_0()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(PANEL_TITLE_TEXT_SIZE))
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(ui_text_primary())
                                        .child(GRAPHQL_PANEL_TITLE),
                                )
                                .child(self.action_button(
                                    GRAPHQL_INTROSPECT_LABEL,
                                    can_edit_request_configuration(self.busy),
                                    ButtonTone::Neutral,
                                    |app, _event, _window, cx| {
                                        app.load_graphql_introspection_query(cx)
                                    },
                                    cx,
                                )),
                        )
                        .child(bounded_text_input(self.graphql_query.clone()))
                        .child(bounded_text_input(self.graphql_variables.clone()))
                        .child(self.render_graphql_preview(cx))
                        .when(!self.graphql_schema_summary.is_empty(), |panel| {
                            panel.child(self.render_graphql_schema_summary())
                        })
                        .when(!self.graphql_schema_browser.is_empty(), |panel| {
                            panel.child(self.render_graphql_schema_browser())
                        })
                        .when(!self.graphql_query_templates.is_empty(), |panel| {
                            panel.child(self.render_graphql_query_assistant(cx))
                        })
                },
            )
            .when(self.request_body_mode == RequestBodyMode::Binary, |panel| {
                panel.child(bounded_text_input(self.binary_body_path.clone()))
            })
    }

    fn render_graphql_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = preview_text(&graphql_body(
            &self.graphql_query.read(cx).text(),
            &self.graphql_variables.read(cx).text(),
        ));
        let highlights = syntax_highlights_for_gpui(&preview, RawBodyFormat::Json);

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(GRAPHQL_PAYLOAD_TITLE),
            )
            .child(
                div()
                    .id("graphql-preview-scroll")
                    .min_w_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .h(px(BODY_PREVIEW_HEIGHT))
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(BODY_PREVIEW_LINE_HEIGHT))
                    .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(StyledText::new(preview).with_highlights(highlights)),
            )
    }

    fn render_graphql_schema_summary(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(GRAPHQL_SCHEMA_TITLE),
            )
            .child(
                div()
                    .id("graphql-schema-summary-scroll")
                    .min_w_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .h(px(BODY_PREVIEW_HEIGHT))
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(BODY_PREVIEW_LINE_HEIGHT))
                    .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(self.graphql_schema_summary.clone()),
            )
    }

    fn render_graphql_schema_browser(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(GRAPHQL_SCHEMA_BROWSER_TITLE),
            )
            .child(
                div()
                    .id("graphql-schema-browser-scroll")
                    .min_w_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .h(px(GRAPHQL_SCHEMA_BROWSER_HEIGHT))
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(BODY_PREVIEW_LINE_HEIGHT))
                    .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(self.graphql_schema_browser.clone()),
            )
    }

    fn render_graphql_query_assistant(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = can_edit_request_configuration(self.busy);
        let mut rows = Vec::new();
        for template in self
            .graphql_query_templates
            .iter()
            .take(GRAPHQL_QUERY_TEMPLATE_LIMIT)
        {
            let operation = template.operation.clone();
            let variables = template.variables.clone();
            let operation_preview = preview_text(&template.operation);
            let variables_preview = preview_text(&template.variables);
            let has_variables = template.variables.trim() != "{}";

            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .overflow_hidden()
                    .gap_1()
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .min_w_0()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_family(PLATFORM_MONOSPACE_FONT)
                                    .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                                    .text_color(ui_text_primary())
                                    .child(template.field_name.clone()),
                            )
                            .child(
                                self.sized_action_button(
                                    "Use",
                                    GRAPHQL_QUERY_TEMPLATE_USE_BUTTON_WIDTH,
                                    enabled,
                                    ButtonTone::Neutral,
                                    move |app, _event, _window, cx| {
                                        app.apply_graphql_query_template(
                                            operation.clone(),
                                            variables.clone(),
                                            cx,
                                        )
                                    },
                                    cx,
                                )
                                .flex_shrink_0(),
                            ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .line_height(px(BODY_PREVIEW_LINE_HEIGHT))
                            .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                            .text_color(ui_text_body())
                            .whitespace_normal()
                            .child(operation_preview),
                    )
                    .when(has_variables, |row| {
                        row.child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .font_family(PLATFORM_MONOSPACE_FONT)
                                .line_height(px(BODY_PREVIEW_LINE_HEIGHT))
                                .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                                .text_color(ui_text_body())
                                .whitespace_normal()
                                .child(format!("variables {variables_preview}")),
                        )
                    }),
            );
        }

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(GRAPHQL_QUERY_ASSISTANT_TITLE),
            )
            .children(rows)
    }

    fn render_raw_body_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.request_body.read(cx).text();
        let preview = preview_text(&body);
        let highlights = syntax_highlights_for_gpui(&preview, self.raw_body_format);

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                div()
                    .text_size(px(PANEL_TITLE_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_primary())
                    .child(RAW_PREVIEW_TITLE),
            )
            .child(
                div()
                    .id("raw-body-preview-scroll")
                    .min_w_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .h(px(BODY_PREVIEW_HEIGHT))
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_2()
                    .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(BODY_PREVIEW_LINE_HEIGHT))
                    .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                    .text_color(ui_text_body())
                    .whitespace_normal()
                    .child(
                        StyledText::new(if preview.is_empty() {
                            RAW_EMPTY_PREVIEW_LABEL.to_string()
                        } else {
                            preview
                        })
                        .with_highlights(highlights),
                    ),
            )
    }

    fn render_codegen_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (snippet, snippet_for_copy) = self.codegen_snippet_state(cx);
        let can_copy = can_copy_codegen_snippet(self.busy, snippet_for_copy.is_some());
        let copy_colors = ButtonTone::Neutral.colors(can_copy);

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(CODEGEN_PANEL_TITLE),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .gap_2()
                            .child(self.codegen_language_selector(cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .h(px(COMPACT_CONTROL_HEIGHT))
                                    .w(px(CODEGEN_COPY_BUTTON_WIDTH))
                                    .rounded(px(UI_RADIUS_CONTROL))
                                    .border_1()
                                    .border_color(copy_colors.border)
                                    .bg(copy_colors.background)
                                    .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(copy_colors.text)
                                    .opacity(control_opacity(can_copy))
                                    .when(can_copy, |button| button.cursor_pointer())
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(
                                            move |app, _event: &MouseUpEvent, _window, cx| {
                                                if let Some(snippet) =
                                                    app.codegen_snippet_for_copy(cx)
                                                {
                                                    cx.write_to_clipboard(
                                                        ClipboardItem::new_string(snippet),
                                                    );
                                                    app.codegen_menu_open = false;
                                                    cx.notify();
                                                }
                                            },
                                        ),
                                    )
                                    .child(CODEGEN_COPY_LABEL),
                            ),
                    ),
            )
            .child(
                div()
                    .id("codegen-snippet-scroll")
                    .min_w_0()
                    .overflow_x_hidden()
                    .overflow_y_scroll()
                    .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                    .h(px(CODEGEN_SNIPPET_HEIGHT))
                    .rounded(px(UI_RADIUS_TIGHT))
                    .border_1()
                    .border_color(ui_border())
                    .bg(ui_surface())
                    .p_3()
                    .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                    .font_family(PLATFORM_MONOSPACE_FONT)
                    .line_height(px(CODEGEN_SNIPPET_LINE_HEIGHT))
                    .text_size(px(PANEL_CONTENT_TEXT_SIZE))
                    .text_color(ui_text_primary())
                    .whitespace_normal()
                    .child(snippet),
            )
    }

    fn codegen_language_selector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = can_use_codegen_language_selector(self.busy);

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_1()
            .child(
                compact_toggle_enabled(
                    snippet_language_label(self.codegen_language),
                    true,
                    enabled,
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                        if can_use_codegen_language_selector(app.busy) {
                            app.codegen_menu_open = !app.codegen_menu_open;
                            app.import_popover_open = false;
                            app.method_menu_open = false;
                            app.collection_context_menu = None;
                            cx.notify();
                        }
                    }),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(snippet_language_label(self.codegen_language)),
                ),
            )
            .when(self.codegen_menu_open && enabled, |menu| {
                menu.child(
                    div()
                        .flex()
                        .flex_col()
                        .rounded(px(UI_RADIUS_CONTROL))
                        .border_1()
                        .border_color(ui_border_strong())
                        .bg(ui_surface())
                        .children(vec![
                            self.codegen_language_menu_item(SnippetLanguage::Curl, cx),
                            self.codegen_language_menu_item(SnippetLanguage::PythonRequests, cx),
                            self.codegen_language_menu_item(SnippetLanguage::JavaScriptFetch, cx),
                            self.codegen_language_menu_item(SnippetLanguage::RustReqwest, cx),
                            self.codegen_language_menu_item(SnippetLanguage::GoNetHttp, cx),
                        ]),
                )
            })
    }

    fn codegen_language_menu_item(
        &self,
        language: SnippetLanguage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + 'static + use<> {
        let active = self.codegen_language == language;
        div()
            .flex()
            .items_center()
            .h(px(COMPACT_CONTROL_HEIGHT))
            .w(px(CODEGEN_MENU_WIDTH))
            .px_2()
            .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
            .font_weight(if active {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            })
            .text_color(if active { ui_accent() } else { ui_text_body() })
            .hover(|row| row.bg(ui_hover()))
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if can_use_codegen_language_selector(app.busy) {
                        app.codegen_language = language;
                        app.codegen_menu_open = false;
                        cx.notify();
                    }
                }),
            )
            .child(snippet_language_label(language))
    }

    fn codegen_snippet_state(&self, cx: &mut Context<Self>) -> (String, Option<String>) {
        match self.current_codegen_request(cx) {
            Ok(request) if can_copy_codegen_request(&request) => {
                let snippet = generate_snippet(&request, self.codegen_language);
                (snippet.clone(), Some(snippet))
            }
            Ok(_) => (CODEGEN_EMPTY_SNIPPET_LABEL.to_string(), None),
            Err(error) => (format!("Request build failed: {error}"), None),
        }
    }

    fn codegen_snippet_for_copy(&self, cx: &mut Context<Self>) -> Option<String> {
        if self.busy {
            return None;
        }

        match self.current_codegen_request(cx) {
            Ok(request) if can_copy_codegen_request(&request) => {
                Some(generate_snippet(&request, self.codegen_language))
            }
            _ => None,
        }
    }

    fn body_mode_button(
        &self,
        label: &'static str,
        mode: RequestBodyMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.request_body_mode == mode;
        let enabled = can_edit_request_configuration(self.busy);

        flexible_toggle_enabled(label, active, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_edit_request_configuration(app.busy) {
                    app.request_body_mode = mode;
                    cx.notify();
                }
            }),
        )
    }

    fn raw_format_button(
        &self,
        label: &'static str,
        format: RawBodyFormat,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.raw_body_format == format;
        let enabled = can_edit_request_configuration(self.busy);

        flexible_toggle_enabled(label, active, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_edit_request_configuration(app.busy) {
                    app.raw_body_format = format;
                    cx.notify();
                }
            }),
        )
    }

    fn websocket_message_mode_button(
        &self,
        label: &'static str,
        mode: WebSocketMessageMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.websocket_message_mode == mode;
        let enabled = can_edit_request_configuration(self.busy);

        flexible_toggle_enabled(label, active, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_edit_request_configuration(app.busy) {
                    app.websocket_message_mode = mode;
                    cx.notify();
                }
            }),
        )
    }

    fn auth_mode_button(
        &self,
        label: &'static str,
        mode: AuthMode,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.auth_mode == mode;
        let enabled = can_edit_request_configuration(self.busy);

        flexible_toggle_enabled(label, active, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_edit_request_configuration(app.busy) {
                    app.auth_mode = mode;
                    cx.notify();
                }
            }),
        )
    }

    fn api_key_placement_button(
        &self,
        label: &'static str,
        placement: ApiKeyPlacement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active = self.api_key_placement == placement;
        let enabled = can_edit_request_configuration(self.busy);

        flexible_toggle_enabled(label, active, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_edit_request_configuration(app.busy) {
                    app.api_key_placement = placement;
                    cx.notify();
                }
            }),
        )
    }

    fn render_response_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let meta = response_header_meta(&self.response_status, &self.response_meta);
        let body = self.response_body_for_view();
        self.response_body_viewer.update(cx, |viewer, _cx| {
            viewer.set_text_from_parent(body);
        });

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(ui_response_pane())
            .child(panel_header(
                "Response",
                meta.as_deref(),
                self.response_tone,
            ))
            .child(self.render_response_tabs(cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("response-body-scroll")
                            .h_full()
                            .min_w_0()
                            .bg(ui_response_pane())
                            .p_3()
                            .pr(px(SCROLLBAR_CONTENT_RIGHT_PADDING))
                            .overflow_y_scroll()
                            .scrollbar_width(px(SCROLLBAR_GUTTER_WIDTH))
                            .track_scroll(&self.response_scroll)
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .line_height(px(RESPONSE_BODY_LINE_HEIGHT))
                            .text_size(px(RESPONSE_BODY_TEXT_SIZE))
                            .text_color(ui_text_primary())
                            .whitespace_normal()
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(self.response_body_viewer.clone()),
                            ),
                    )
                    .child(self.render_vertical_scrollbar(
                        ScrollbarKind::Response,
                        &self.response_scroll,
                        cx,
                    )),
            )
    }

    fn render_response_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = response_tabs()
            .into_iter()
            .map(|(label, view)| self.response_tab(label, view, cx))
            .collect::<Vec<_>>();

        div()
            .flex()
            .items_center()
            .h(px(RESPONSE_TAB_BAR_HEIGHT))
            .min_w_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(ui_border())
            .bg(ui_response_tab_bar())
            .px_3()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .gap_1()
                    .children(tabs),
            )
            .when(self.response_view == ResponseView::Pretty, |tabs| {
                tabs.child(self.response_fold_button(cx))
            })
            .child(self.response_copy_button(cx))
    }

    fn response_fold_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if self.response_pretty_collapsed {
            RESPONSE_OPEN_LABEL
        } else {
            RESPONSE_FOLD_LABEL
        };
        let enabled = can_toggle_response_collapse(self.busy, &self.response_raw_body);

        response_toolbar_button(label, RESPONSE_FOLD_BUTTON_WIDTH, enabled).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_toggle_response_collapse(app.busy, &app.response_raw_body) {
                    app.response_pretty_collapsed = !app.response_pretty_collapsed;
                    reset_scroll_handle(&app.response_scroll);
                    cx.notify();
                }
            }),
        )
    }

    fn response_copy_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let enabled = can_copy_response_view(
            self.busy,
            self.response_view,
            self.response_pretty_collapsed,
            &self.response_body,
            &self.response_raw_body,
            &self.response_headers,
        );

        response_toolbar_button(RESPONSE_COPY_LABEL, RESPONSE_COPY_BUTTON_WIDTH, enabled)
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.copy_response_body(cx);
                }),
            )
    }

    fn response_tab(
        &self,
        label: &'static str,
        view: ResponseView,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let active = self.response_view == view;
        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(COMPACT_CONTROL_HEIGHT))
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .max_w(px(RESPONSE_TAB_WIDTH))
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(if active {
                ui_accent()
            } else {
                ui_border_strong()
            })
            .bg(if active {
                ui_surface()
            } else {
                ui_response_tab_bar()
            })
            .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(if active { ui_accent() } else { ui_text_body() })
            .cursor_pointer()
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.select_response_tab(view, cx);
                }),
            )
            .child(div().min_w_0().truncate().child(label))
    }

    fn response_body_for_view(&self) -> String {
        response_body_for_view(
            self.response_view,
            self.response_pretty_collapsed,
            &self.response_body,
            &self.response_raw_body,
            &self.response_headers,
        )
    }
}

impl Render for ZenApiApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .key_context("ZenApiApp")
            .on_action(cx.listener(Self::send_current_request_shortcut))
            .on_action(cx.listener(Self::save_current_request_shortcut))
            .on_action(cx.listener(Self::focus_active_sidebar_input))
            .on_action(cx.listener(Self::focus_request_url))
            .on_action(cx.listener(Self::select_request_tab_params))
            .on_action(cx.listener(Self::select_request_tab_headers))
            .on_action(cx.listener(Self::select_request_tab_auth))
            .on_action(cx.listener(Self::select_request_tab_body))
            .on_action(cx.listener(Self::select_request_tab_scripts))
            .on_action(cx.listener(Self::select_request_tab_realtime))
            .on_action(cx.listener(Self::select_request_tab_tools))
            .on_action(cx.listener(Self::select_response_tab_pretty))
            .on_action(cx.listener(Self::select_response_tab_raw))
            .on_action(cx.listener(Self::select_response_tab_headers))
            .on_action(cx.listener(Self::close_transient_ui))
            .font_family(PLATFORM_UI_FONT)
            .text_size(px(APP_BASE_TEXT_SIZE))
            .text_color(ui_text_primary())
            .bg(ui_surface())
            .child(self.render_top_bar(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(self.render_workspace(window, cx)),
            )
            .child(self.render_status_bar())
            .when(self.import_popover_open, |root| {
                root.child(self.render_import_popover(cx))
            })
    }
}

impl Render for CollectionDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .h(px(COLLECTION_DRAG_PREVIEW_HEIGHT))
            .max_w(px(COLLECTION_DRAG_PREVIEW_MAX_WIDTH))
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(ui_accent())
            .bg(collection_drag_over_background())
            .px_2()
            .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(ui_accent_text())
            .min_w_0()
            .overflow_hidden()
            .child(div().min_w_0().truncate().child(self.label.clone()))
    }
}

impl Render for WorkspaceSplitDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .w(px(WORKSPACE_SPLIT_HANDLE_WIDTH))
            .h(px(APP_WINDOW_HEIGHT * 2.))
            .child(
                div()
                    .absolute()
                    .left(px((WORKSPACE_SPLIT_HANDLE_WIDTH
                        - WORKSPACE_SPLIT_DIVIDER_WIDTH)
                        / 2.))
                    .w(px(WORKSPACE_SPLIT_DIVIDER_WIDTH))
                    .h_full()
                    .bg(ui_border_strong()),
            )
    }
}

impl ZenApiApp {
    fn render_status_bar(&self) -> impl IntoElement {
        let route_status = route_status_label(self.routes.len(), self.visible_routes.len());
        let busy_status = status_bar_busy_label(self.busy);
        let mock_status = status_bar_mock_label(self.server_running, &self.server_status);
        let response_status = response_status_label(&self.response_status);
        let show_trailing_status =
            status_bar_trailing_visible(response_status.as_deref(), busy_status);

        div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(STATUS_BAR_HEIGHT))
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .border_t_1()
            .border_color(ui_border())
            .bg(ui_app_chrome())
            .px_3()
            .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
            .text_color(ui_text_secondary())
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .gap_3()
                    .child(div().flex_shrink_0().truncate().child(route_status))
                    .when_some(mock_status, |bar, status| {
                        bar.child(div().flex_1().min_w_0().truncate().child(status))
                    }),
            )
            .when(show_trailing_status, |bar| {
                bar.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .max_w(px(STATUS_BAR_TRAILING_MAX_WIDTH))
                        .flex_shrink_1()
                        .min_w_0()
                        .overflow_hidden()
                        .gap_3()
                        .when_some(response_status, |bar, status| {
                            bar.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_right()
                                    .font_family(PLATFORM_MONOSPACE_FONT)
                                    .text_color(self.response_tone.color())
                                    .child(status),
                            )
                        })
                        .when_some(busy_status, |bar, status| {
                            bar.child(div().flex_shrink_0().child(status))
                        }),
                )
            })
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
                background: ui_disabled_surface(),
                border: ui_disabled_border(),
                text: ui_disabled_text(),
            };
        }

        match self {
            Self::Neutral => ButtonColors {
                background: ui_surface(),
                border: ui_border_strong(),
                text: ui_text_body(),
            },
            Self::Primary => ButtonColors {
                background: ui_surface(),
                border: ui_accent(),
                text: ui_accent_text(),
            },
            Self::Warning => ButtonColors {
                background: ui_surface(),
                border: ui_warning_strong(),
                text: ui_warning_strong(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResponseView {
    Pretty,
    Raw,
    Headers,
}

fn response_tabs() -> [(&'static str, ResponseView); RESPONSE_TAB_COUNT] {
    [
        (RESPONSE_PRETTY_LABEL, ResponseView::Pretty),
        (RESPONSE_RAW_LABEL, ResponseView::Raw),
        (RESPONSE_HEADERS_LABEL, ResponseView::Headers),
    ]
}

#[cfg(test)]
fn response_tab_shortcuts() -> [(usize, ResponseView); RESPONSE_TAB_COUNT] {
    [
        (1, ResponseView::Pretty),
        (2, ResponseView::Raw),
        (3, ResponseView::Headers),
    ]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    None,
    Bearer,
    OAuth2,
    Basic,
    Jwt,
    ApiKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ApiKeyPlacement {
    Header,
    Query,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestBodyMode {
    None,
    FormData,
    UrlEncoded,
    Raw,
    GraphQL,
    Binary,
}

fn request_body_mode_rows() -> [[(&'static str, RequestBodyMode); 3]; 2] {
    [
        [
            (BODY_MODE_NONE_LABEL, RequestBodyMode::None),
            (BODY_MODE_FORM_LABEL, RequestBodyMode::FormData),
            (BODY_MODE_URL_ENCODED_LABEL, RequestBodyMode::UrlEncoded),
        ],
        [
            (BODY_MODE_RAW_LABEL, RequestBodyMode::Raw),
            (BODY_MODE_GRAPHQL_LABEL, RequestBodyMode::GraphQL),
            (BODY_MODE_BINARY_LABEL, RequestBodyMode::Binary),
        ],
    ]
}

fn request_body_editor_title(mode: RequestBodyMode) -> Option<&'static str> {
    match mode {
        RequestBodyMode::FormData => Some(BODY_FORM_FIELDS_TITLE),
        RequestBodyMode::UrlEncoded => Some(BODY_URL_ENCODED_TITLE),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RawBodyFormat {
    Json,
    Xml,
    Text,
    Html,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TestAssertionKind {
    StatusEquals,
    StatusInRange,
    HeaderExists,
    HeaderEquals,
    BodyContains,
    JsonPathEquals,
}

impl TestAssertionKind {
    fn label(self) -> &'static str {
        match self {
            Self::StatusEquals => TEST_ASSERTION_STATUS_EQUALS_LABEL,
            Self::StatusInRange => TEST_ASSERTION_STATUS_RANGE_LABEL,
            Self::HeaderExists => TEST_ASSERTION_HEADER_EXISTS_LABEL,
            Self::HeaderEquals => TEST_ASSERTION_HEADER_EQUALS_LABEL,
            Self::BodyContains => TEST_ASSERTION_BODY_CONTAINS_LABEL,
            Self::JsonPathEquals => TEST_ASSERTION_JSON_PATH_EQUALS_LABEL,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::StatusEquals => Self::StatusInRange,
            Self::StatusInRange => Self::HeaderExists,
            Self::HeaderExists => Self::HeaderEquals,
            Self::HeaderEquals => Self::BodyContains,
            Self::BodyContains => Self::JsonPathEquals,
            Self::JsonPathEquals => Self::StatusEquals,
        }
    }
}

impl RawBodyFormat {
    fn content_type(self) -> &'static str {
        match self {
            Self::Json => "application/json",
            Self::Xml => "application/xml",
            Self::Text => "text/plain",
            Self::Html => "text/html",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntaxTokenKind {
    String,
    Number,
    Keyword,
    Punctuation,
    Tag,
    Attribute,
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
            Self::Neutral => ui_text_body(),
            Self::Busy => ui_status_busy(),
            Self::Success => ui_status_success(),
            Self::Error => ui_status_error(),
        }
    }
}

fn key_value_rows(cx: &mut Context<ZenApiApp>, specs: &[(&str, &str)]) -> Vec<KeyValueRow> {
    specs
        .iter()
        .map(|(key_placeholder, value_placeholder)| {
            key_value_row_entity(cx, *key_placeholder, *value_placeholder)
        })
        .collect()
}

fn key_value_row_entity(
    cx: &mut Context<ZenApiApp>,
    key_placeholder: impl Into<SharedString>,
    value_placeholder: impl Into<SharedString>,
) -> KeyValueRow {
    let key_placeholder = key_placeholder.into();
    let value_placeholder = value_placeholder.into();
    KeyValueRow {
        key: cx.new(|cx| TextInput::new(cx, key_placeholder, true)),
        value: cx.new(|cx| TextInput::new(cx, value_placeholder, true)),
    }
}

fn environment_config(
    cx: &mut Context<ZenApiApp>,
    name: impl Into<String>,
    specs: &[(&str, &str)],
) -> EnvironmentConfig {
    EnvironmentConfig {
        name: name.into(),
        variables: key_value_rows(cx, specs),
    }
}

fn read_key_value_rows(rows: &[KeyValueRow], cx: &mut Context<ZenApiApp>) -> Vec<(String, String)> {
    rows.iter()
        .filter_map(|row| {
            let key = row.key.read(cx).text().trim().to_string();
            if key.is_empty() {
                return None;
            }

            Some((key, row.value.read(cx).text().trim().to_string()))
        })
        .collect()
}

fn has_key_value_rows(rows: &[KeyValueRow], cx: &mut Context<ZenApiApp>) -> bool {
    rows.iter()
        .any(|row| key_value_key_is_present(&row.key.read(cx).text()))
}

fn key_value_key_is_present(key: &str) -> bool {
    !key.trim().is_empty()
}

fn set_key_value_rows(rows: &[KeyValueRow], values: Vec<NameValue>, cx: &mut Context<ZenApiApp>) {
    for (index, row) in rows.iter().enumerate() {
        let name = values
            .get(index)
            .map(|pair| pair.name.clone())
            .unwrap_or_default();
        let value = values
            .get(index)
            .map(|pair| pair.value.clone())
            .unwrap_or_default();

        row.key.update(cx, |input, cx| input.set_text(name, cx));
        row.value.update(cx, |input, cx| input.set_text(value, cx));
    }
}

fn assertion_rows_from_assertions(
    cx: &mut Context<ZenApiApp>,
    assertions: &[ResponseAssertion],
) -> Vec<TestAssertionRow> {
    let mut rows = assertions
        .iter()
        .map(|assertion| assertion_row_from_assertion(cx, assertion))
        .collect::<Vec<_>>();

    if rows.is_empty() {
        rows.push(blank_assertion_row(cx));
        rows.push(blank_assertion_row(cx));
    } else {
        rows.push(blank_assertion_row(cx));
    }

    rows
}

fn blank_assertion_row(cx: &mut Context<ZenApiApp>) -> TestAssertionRow {
    assertion_row_entity(cx, "", TestAssertionKind::StatusEquals, "", "")
}

fn assertion_row_from_assertion(
    cx: &mut Context<ZenApiApp>,
    assertion: &ResponseAssertion,
) -> TestAssertionRow {
    let (kind, target, expected) = assertion_fields(assertion);
    assertion_row_entity(cx, &assertion.name, kind, &target, &expected)
}

fn assertion_row_entity(
    cx: &mut Context<ZenApiApp>,
    name: impl Into<SharedString>,
    kind: TestAssertionKind,
    target: impl Into<SharedString>,
    expected: impl Into<SharedString>,
) -> TestAssertionRow {
    let row = TestAssertionRow {
        name: cx.new(|cx| TextInput::new(cx, PLACEHOLDER_TEST_ASSERTION_NAME, false)),
        kind,
        target: cx.new(|cx| TextInput::new(cx, PLACEHOLDER_TEST_ASSERTION_TARGET, true)),
        expected: cx.new(|cx| TextInput::new(cx, PLACEHOLDER_TEST_ASSERTION_EXPECTED, true)),
    };
    let name = name.into();
    let target = target.into();
    let expected = expected.into();
    row.name.update(cx, |input, cx| input.set_text(name, cx));
    row.target
        .update(cx, |input, cx| input.set_text(target, cx));
    row.expected
        .update(cx, |input, cx| input.set_text(expected, cx));
    row
}

fn assertion_fields(assertion: &ResponseAssertion) -> (TestAssertionKind, String, String) {
    match &assertion.kind {
        ResponseAssertionKind::StatusEquals { status } => (
            TestAssertionKind::StatusEquals,
            status.to_string(),
            String::new(),
        ),
        ResponseAssertionKind::StatusInRange { min, max } => (
            TestAssertionKind::StatusInRange,
            min.to_string(),
            max.to_string(),
        ),
        ResponseAssertionKind::HeaderExists { name } => {
            (TestAssertionKind::HeaderExists, name.clone(), String::new())
        }
        ResponseAssertionKind::HeaderEquals { name, value } => {
            (TestAssertionKind::HeaderEquals, name.clone(), value.clone())
        }
        ResponseAssertionKind::BodyContains { text } => {
            (TestAssertionKind::BodyContains, text.clone(), String::new())
        }
        ResponseAssertionKind::JsonPathEquals { path, value } => (
            TestAssertionKind::JsonPathEquals,
            path.clone(),
            value.to_string(),
        ),
    }
}

fn response_assertion_from_fields(
    kind: TestAssertionKind,
    name: &str,
    target: &str,
    expected: &str,
) -> Result<Option<ResponseAssertion>> {
    let name = name.trim();
    let target = target.trim();
    let expected = expected.trim();
    if name.is_empty() && target.is_empty() && expected.is_empty() {
        return Ok(None);
    }
    if target.is_empty() {
        return Err(anyhow!("Target is empty."));
    }

    let assertion_name = if name.is_empty() {
        format!("{} {target}", kind.label())
    } else {
        name.to_string()
    };
    let kind = match kind {
        TestAssertionKind::StatusEquals => ResponseAssertionKind::StatusEquals {
            status: parse_u16_field(target, "status")?,
        },
        TestAssertionKind::StatusInRange => ResponseAssertionKind::StatusInRange {
            min: parse_u16_field(target, "min status")?,
            max: parse_u16_field(expected, "max status")?,
        },
        TestAssertionKind::HeaderExists => ResponseAssertionKind::HeaderExists {
            name: target.to_string(),
        },
        TestAssertionKind::HeaderEquals => ResponseAssertionKind::HeaderEquals {
            name: target.to_string(),
            value: expected.to_string(),
        },
        TestAssertionKind::BodyContains => ResponseAssertionKind::BodyContains {
            text: target.to_string(),
        },
        TestAssertionKind::JsonPathEquals => ResponseAssertionKind::JsonPathEquals {
            path: target.to_string(),
            value: parse_json_value_field(expected)?,
        },
    };

    if let ResponseAssertionKind::StatusInRange { min, max } = &kind {
        if min > max {
            return Err(anyhow!("Min > max status."));
        }
    }

    Ok(Some(ResponseAssertion {
        name: assertion_name,
        kind,
    }))
}

fn parse_u16_field(input: &str, label: &str) -> Result<u16> {
    input.parse::<u16>().map_err(|_| anyhow!("Bad {label}."))
}

fn parse_json_value_field(input: &str) -> Result<serde_json::Value> {
    if input.trim().is_empty() {
        return Err(anyhow!("Expected is empty."));
    }

    serde_json::from_str(input).or_else(|_| Ok(serde_json::Value::String(input.to_string())))
}

fn configured_assertion_count(rows: &[TestAssertionRow], cx: &mut Context<ZenApiApp>) -> usize {
    rows.iter()
        .filter(|row| {
            !row.name.read(cx).text().trim().is_empty()
                || !row.target.read(cx).text().trim().is_empty()
                || !row.expected.read(cx).text().trim().is_empty()
        })
        .count()
}

fn assertion_meta(results: &[ResponseAssertionResult]) -> Option<String> {
    if results.is_empty() {
        return None;
    }

    let passed = results.iter().filter(|result| result.passed).count();
    Some(format!("{passed}/{} tests", results.len()))
}

fn response_panel_meta(results: &[ResponseAssertionResult]) -> String {
    assertion_meta(results).unwrap_or_default()
}

fn pre_request_status_label(actions: usize) -> String {
    match actions {
        0 => "idle".to_string(),
        count => format!("{count} act"),
    }
}

fn pre_request_error_label(error: &str) -> String {
    format!("Err {}", preview_text(error))
}

fn clean_error_message(error: &str) -> &str {
    let error = error.trim();
    if error.is_empty() {
        "Unknown error"
    } else {
        error
    }
}

fn file_operation_error(action: &str, path: &str, error: &str) -> String {
    format!(
        "{action}\n\nPath\n{}\n\nError\n{}",
        path.trim(),
        clean_error_message(error)
    )
}

fn editor_error(action: &str, error: &str) -> String {
    format!("{action}\n\nError\n{}", clean_error_message(error))
}

fn request_transport_error(method: &str, url: &str, error: &str) -> String {
    format!(
        "Request failed.\n\nRequest\n{} {}\n\nError\n{}",
        method.trim(),
        url.trim(),
        clean_error_message(error)
    )
}

fn mock_server_error(port: u16, error: &str) -> String {
    format!(
        "Mock start failed.\n\nPort\n{port}\n\nError\n{}",
        clean_error_message(error)
    )
}

fn realtime_operation_error(action: &str, target_label: &str, target: &str, error: &str) -> String {
    format!(
        "{action}\n\n{target_label}\n{}\n\nError\n{}",
        target.trim(),
        clean_error_message(error)
    )
}

fn runner_failure_text(summary: &CollectionRunSummary) -> String {
    runner_summary_text(summary)
}

fn runner_worker_stopped_message() -> &'static str {
    "Collection runner stopped."
}

fn set_key_value_pairs(
    rows: &mut Vec<KeyValueRow>,
    values: Vec<(String, String)>,
    cx: &mut Context<ZenApiApp>,
) {
    while rows.len() < values.len() {
        rows.push(key_value_row_entity(cx, "", ""));
    }

    for (index, row) in rows.iter().enumerate() {
        let (name, value) = values
            .get(index)
            .cloned()
            .unwrap_or_else(|| (String::new(), String::new()));
        row.key.update(cx, |input, cx| input.set_text(name, cx));
        row.value.update(cx, |input, cx| input.set_text(value, cx));
    }
}

fn upsert_header_pair(
    headers: &[(String, String)],
    name: &str,
    value: &str,
) -> Vec<(String, String)> {
    let mut headers = headers.to_vec();
    if let Some((existing_name, existing_value)) = headers
        .iter_mut()
        .find(|(existing_name, _)| existing_name.eq_ignore_ascii_case(name))
    {
        *existing_name = name.to_string();
        *existing_value = value.to_string();
    } else {
        headers.push((name.to_string(), value.to_string()));
    }
    headers
}

fn parse_header_bulk(input: &str) -> Vec<(String, String)> {
    input.lines().filter_map(parse_header_bulk_line).collect()
}

fn parse_header_bulk_line(line: &str) -> Option<(String, String)> {
    let line = normalize_header_bulk_line(line)?;
    let (name, value) = line.split_once(':').or_else(|| line.split_once('='))?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    Some((name.to_string(), value.trim().to_string()))
}

fn normalize_header_bulk_line(line: &str) -> Option<&str> {
    let mut line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    if let Some(rest) = line.strip_prefix("-H ") {
        line = rest.trim();
    } else if let Some(rest) = line.strip_prefix("--header ") {
        line = rest.trim();
    }

    line = line
        .strip_prefix('\'')
        .and_then(|line| line.strip_suffix('\''))
        .or_else(|| {
            line.strip_prefix('"')
                .and_then(|line| line.strip_suffix('"'))
        })
        .unwrap_or(line)
        .trim();

    (!line.is_empty()).then_some(line)
}

fn format_header_bulk(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn variable_store_from_pairs(
    global_variables: Vec<(String, String)>,
    active_environment: Option<&str>,
    environment_variables: Vec<(String, String)>,
) -> VariableStore {
    let mut store = VariableStore::new();

    for (name, value) in global_variables {
        store.upsert(Variable::global(name, value));
    }

    if let Some(environment) = active_environment {
        for (name, value) in environment_variables {
            store.upsert(Variable::environment(environment, name, value));
        }
    }

    store
}

fn normalized_environment_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join("-")
}

#[cfg(test)]
fn resolve_template(
    input: &str,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<String> {
    replace_variables(input, store, active_environment)
}

#[cfg(test)]
fn resolve_key_value_pairs(
    pairs: Vec<(String, String)>,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<Vec<(String, String)>> {
    pairs
        .into_iter()
        .map(|(name, value)| {
            Ok((
                resolve_template(&name, store, active_environment)?,
                resolve_template(&value, store, active_environment)?,
            ))
        })
        .collect()
}

#[cfg(test)]
fn resolve_request_body(
    body: RequestBody,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<RequestBody> {
    Ok(match body {
        RequestBody::None => RequestBody::None,
        RequestBody::Raw { content_type, body } => RequestBody::Raw {
            content_type,
            body: resolve_template(&body, store, active_environment)?,
        },
        RequestBody::FormUrlEncoded(fields) => {
            RequestBody::FormUrlEncoded(resolve_key_value_pairs(fields, store, active_environment)?)
        }
        RequestBody::Multipart(fields) => {
            RequestBody::Multipart(resolve_key_value_pairs(fields, store, active_environment)?)
        }
        RequestBody::BinaryFile { path, content_type } => RequestBody::BinaryFile {
            path: resolve_template(&path, store, active_environment)?,
            content_type,
        },
    })
}

fn history_request_from_body(method: &str, url: &str, body: &RequestBody) -> HistoryRequest {
    let (body_kind, body_preview) = match body {
        RequestBody::None => ("none", String::new()),
        RequestBody::Raw { body, .. } => ("raw", preview_text(body)),
        RequestBody::FormUrlEncoded(fields) => ("x-www-form-urlencoded", preview_pairs(fields)),
        RequestBody::Multipart(fields) => ("form-data", preview_pairs(fields)),
        RequestBody::BinaryFile { path, .. } => ("binary", path.clone()),
    };

    HistoryRequest {
        method: method.to_string(),
        url: url.to_string(),
        body_kind: body_kind.to_string(),
        body_preview,
    }
}

fn collection_request_from_codegen(request: &CodegenRequest) -> CollectionRequest {
    CollectionRequest {
        name: collection_request_name(&request.method, &request.url),
        method: request.method.clone(),
        url: request.url.clone(),
        headers: name_values_from_pairs(&request.headers),
        query_params: name_values_from_pairs(&request.query_params),
        body: collection_body_from_request_body(&request.body),
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

fn collection_request_for_save(
    request: &CodegenRequest,
    pre_request_script: String,
    tests: Vec<ResponseAssertion>,
) -> CollectionRequest {
    let mut collection_request = collection_request_from_codegen(request);
    collection_request.pre_request_script = pre_request_script;
    collection_request.tests = tests;
    collection_request
}

fn collection_request_name(method: &str, url: &str) -> String {
    let path = url.split('?').next().unwrap_or(url).trim_end_matches('/');
    let tail = path
        .rsplit('/')
        .find(|segment| !segment.trim().is_empty())
        .unwrap_or("request");
    format!("{} {}", method.to_ascii_uppercase(), tail)
}

fn name_values_from_pairs(pairs: &[(String, String)]) -> Vec<NameValue> {
    pairs
        .iter()
        .filter(|(name, _value)| !name.trim().is_empty())
        .map(|(name, value)| NameValue {
            name: name.trim().to_string(),
            value: value.trim().to_string(),
        })
        .collect()
}

fn collection_body_from_request_body(body: &RequestBody) -> CollectionBody {
    match body {
        RequestBody::None => CollectionBody::None,
        RequestBody::Raw { content_type, body } => CollectionBody::Raw {
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "text/plain".to_string()),
            body: body.clone(),
        },
        RequestBody::FormUrlEncoded(fields) => CollectionBody::UrlEncoded {
            fields: name_values_from_pairs(fields),
        },
        RequestBody::Multipart(fields) => CollectionBody::FormData {
            fields: name_values_from_pairs(fields),
        },
        RequestBody::BinaryFile { path, content_type } => CollectionBody::Binary {
            path: path.clone(),
            content_type: content_type
                .clone()
                .unwrap_or_else(|| "application/octet-stream".to_string()),
        },
    }
}

fn raw_format_from_content_type(content_type: &str) -> RawBodyFormat {
    let content_type = content_type.to_ascii_lowercase();
    if content_type.contains("json") {
        RawBodyFormat::Json
    } else if content_type.contains("xml") {
        RawBodyFormat::Xml
    } else if content_type.contains("html") {
        RawBodyFormat::Html
    } else {
        RawBodyFormat::Text
    }
}

fn syntax_highlights_for_gpui(
    input: &str,
    format: RawBodyFormat,
) -> Vec<(Range<usize>, HighlightStyle)> {
    syntax_highlights(input, format)
        .into_iter()
        .map(|(range, kind)| (range, syntax_highlight_style(kind)))
        .collect()
}

fn syntax_highlights(input: &str, format: RawBodyFormat) -> Vec<(Range<usize>, SyntaxTokenKind)> {
    match format {
        RawBodyFormat::Json => json_syntax_highlights(input),
        RawBodyFormat::Xml | RawBodyFormat::Html => markup_syntax_highlights(input),
        RawBodyFormat::Text => Vec::new(),
    }
}

fn syntax_highlight_style(kind: SyntaxTokenKind) -> HighlightStyle {
    let color = match kind {
        SyntaxTokenKind::String => rgb(UI_COLOR_SYNTAX_STRING).into(),
        SyntaxTokenKind::Number => rgb(UI_COLOR_SYNTAX_NUMBER).into(),
        SyntaxTokenKind::Keyword => rgb(UI_COLOR_SYNTAX_KEYWORD).into(),
        SyntaxTokenKind::Punctuation => rgb(UI_COLOR_SYNTAX_PUNCTUATION).into(),
        SyntaxTokenKind::Tag => rgb(UI_COLOR_SYNTAX_TAG).into(),
        SyntaxTokenKind::Attribute => rgb(UI_COLOR_SYNTAX_ATTRIBUTE).into(),
    };

    HighlightStyle {
        color: Some(color),
        font_weight: matches!(kind, SyntaxTokenKind::Keyword | SyntaxTokenKind::Tag)
            .then_some(FontWeight::BOLD),
        ..Default::default()
    }
}

fn json_syntax_highlights(input: &str) -> Vec<(Range<usize>, SyntaxTokenKind)> {
    let bytes = input.as_bytes();
    let mut highlights = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                let end = string_literal_end(bytes, index);
                highlights.push((index..end, SyntaxTokenKind::String));
                index = end;
            }
            b'-' | b'0'..=b'9' => {
                let end = json_number_end(bytes, index);
                if end > index {
                    highlights.push((index..end, SyntaxTokenKind::Number));
                    index = end;
                } else {
                    index += 1;
                }
            }
            b't' | b'f' | b'n' => {
                if let Some(end) = json_keyword_end(input, index) {
                    highlights.push((index..end, SyntaxTokenKind::Keyword));
                    index = end;
                } else {
                    index += 1;
                }
            }
            b'{' | b'}' | b'[' | b']' | b':' | b',' => {
                highlights.push((index..index + 1, SyntaxTokenKind::Punctuation));
                index += 1;
            }
            _ => index += 1,
        }
    }

    highlights
}

fn markup_syntax_highlights(input: &str) -> Vec<(Range<usize>, SyntaxTokenKind)> {
    let bytes = input.as_bytes();
    let mut highlights = Vec::new();
    let mut index = 0;

    while let Some(relative_start) = input[index..].find('<') {
        let start = index + relative_start;
        let Some(relative_end) = input[start..].find('>') else {
            break;
        };
        let end = start + relative_end + 1;
        highlights.push((start..start + 1, SyntaxTokenKind::Punctuation));
        if end > start + 1 {
            highlights.push((end - 1..end, SyntaxTokenKind::Punctuation));
        }

        let mut cursor = start + 1;
        while cursor < end
            && matches!(
                bytes[cursor],
                b'/' | b'!' | b'?' | b'-' | b' ' | b'\t' | b'\n'
            )
        {
            if !bytes[cursor].is_ascii_whitespace() {
                highlights.push((cursor..cursor + 1, SyntaxTokenKind::Punctuation));
            }
            cursor += 1;
        }

        if cursor < end && matches!(bytes[cursor], b'a'..=b'z' | b'A'..=b'Z' | b'_' | b':') {
            let name_start = cursor;
            cursor += 1;
            while cursor < end
                && matches!(
                    bytes[cursor],
                    b'a'..=b'z'
                        | b'A'..=b'Z'
                        | b'0'..=b'9'
                        | b'_'
                        | b'-'
                        | b':'
                        | b'.'
                )
            {
                cursor += 1;
            }
            highlights.push((name_start..cursor, SyntaxTokenKind::Tag));
        }

        while cursor < end {
            match bytes[cursor] {
                b'"' | b'\'' => {
                    let quote = bytes[cursor];
                    let value_start = cursor;
                    cursor += 1;
                    while cursor < end && bytes[cursor] != quote {
                        cursor += 1;
                    }
                    if cursor < end {
                        cursor += 1;
                    }
                    highlights.push((value_start..cursor, SyntaxTokenKind::String));
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' | b':' => {
                    let name_start = cursor;
                    cursor += 1;
                    while cursor < end
                        && matches!(
                            bytes[cursor],
                            b'a'..=b'z'
                                | b'A'..=b'Z'
                                | b'0'..=b'9'
                                | b'_'
                                | b'-'
                                | b':'
                                | b'.'
                        )
                    {
                        cursor += 1;
                    }
                    if input[cursor..end].trim_start().starts_with('=') {
                        highlights.push((name_start..cursor, SyntaxTokenKind::Attribute));
                    }
                }
                b'/' | b'?' | b'!' | b'=' => {
                    highlights.push((cursor..cursor + 1, SyntaxTokenKind::Punctuation));
                    cursor += 1;
                }
                _ => cursor += 1,
            }
        }

        index = end;
    }

    highlights
}

fn string_literal_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    let mut escaped = false;
    while index < bytes.len() {
        if escaped {
            escaped = false;
        } else if bytes[index] == b'\\' {
            escaped = true;
        } else if bytes[index] == b'"' {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn json_number_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    if bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    let digits_start = index;
    while matches!(bytes.get(index), Some(b'0'..=b'9')) {
        index += 1;
    }
    if index == digits_start {
        return start;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        let exponent = index;
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exponent_digits = index;
        while matches!(bytes.get(index), Some(b'0'..=b'9')) {
            index += 1;
        }
        if index == exponent_digits {
            return exponent;
        }
    }
    index
}

fn json_keyword_end(input: &str, start: usize) -> Option<usize> {
    ["true", "false", "null"].iter().find_map(|keyword| {
        input[start..]
            .starts_with(keyword)
            .then(|| start + keyword.len())
    })
}

fn blank_collection_request() -> CollectionRequest {
    CollectionRequest {
        name: "New Request".to_string(),
        method: "GET".to_string(),
        url: "https://api.example.com/request".to_string(),
        headers: Vec::new(),
        query_params: Vec::new(),
        body: CollectionBody::None,
        pre_request_script: String::new(),
        tests: Vec::new(),
    }
}

fn insert_collection_item(
    items: &mut Vec<CollectionItem>,
    target_id: &str,
    item: CollectionItem,
) -> bool {
    let Some(indices) = collection_node_indices(target_id) else {
        return false;
    };
    let Some(target_items) = collection_insertion_items_mut(items, &indices) else {
        return false;
    };
    target_items.push(item);
    true
}

fn rename_collection_node(collection: &mut ApiCollection, node_id: &str, name: &str) -> bool {
    if node_id == "collection" {
        collection.name = name.to_string();
        return true;
    }

    let Some(indices) = collection_node_indices(node_id) else {
        return false;
    };
    let Some(item) = collection_item_mut(&mut collection.items, &indices) else {
        return false;
    };

    match item {
        CollectionItem::Folder(folder) => folder.name = name.to_string(),
        CollectionItem::Request(request) => request.name = name.to_string(),
    }
    true
}

fn remove_collection_item(
    items: &mut Vec<CollectionItem>,
    node_id: &str,
) -> Option<CollectionItem> {
    let indices = collection_node_indices(node_id)?;
    remove_collection_item_by_indices(items, &indices)
}

fn remove_collection_item_by_indices(
    items: &mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<CollectionItem> {
    if indices.is_empty() {
        return None;
    }

    let index = *indices.last()?;
    let parent = collection_parent_items_mut(items, &indices)?;
    (index < parent.len()).then(|| parent.remove(index))
}

fn duplicate_collection_item(items: &mut Vec<CollectionItem>, node_id: &str) -> bool {
    let Some(indices) = collection_node_indices(node_id) else {
        return false;
    };
    if indices.is_empty() {
        return false;
    }

    let Some(item) = collection_item_ref(items, &indices).cloned() else {
        return false;
    };
    let item = collection_item_copy(item);
    let Some(index) = indices.last().copied() else {
        return false;
    };
    let Some(parent) = collection_parent_items_mut(items, &indices) else {
        return false;
    };

    parent.insert(index + 1, item);
    true
}

fn move_collection_item(items: &mut Vec<CollectionItem>, source_id: &str, target_id: &str) -> bool {
    let Some(source_indices) = collection_node_indices(source_id) else {
        return false;
    };
    let Some(mut target_indices) = collection_node_indices(target_id) else {
        return false;
    };
    if source_indices.is_empty()
        || collection_path_contains(&source_indices, &target_indices)
        || (!target_indices.is_empty() && collection_item_ref(items, &target_indices).is_none())
    {
        return false;
    }

    let Some(item) = remove_collection_item_by_indices(items, &source_indices) else {
        return false;
    };
    adjust_collection_indices_after_removal(&source_indices, &mut target_indices);

    if insert_collection_item_for_drop(items, &target_indices, item) {
        true
    } else {
        false
    }
}

fn collection_path_contains(source: &[usize], target: &[usize]) -> bool {
    target.len() >= source.len() && target[..source.len()] == *source
}

fn adjust_collection_indices_after_removal(source: &[usize], target: &mut [usize]) {
    if source.is_empty() || target.len() < source.len() {
        return;
    }

    let source_parent_len = source.len() - 1;
    if target[..source_parent_len] == source[..source_parent_len]
        && target[source_parent_len] > source[source_parent_len]
    {
        target[source_parent_len] -= 1;
    }
}

fn insert_collection_item_for_drop(
    items: &mut Vec<CollectionItem>,
    target_indices: &[usize],
    item: CollectionItem,
) -> bool {
    if target_indices.is_empty() {
        items.push(item);
        return true;
    }

    let target_is_folder = matches!(
        collection_item_ref(items, target_indices),
        Some(CollectionItem::Folder(_))
    );
    if target_is_folder {
        let Some(CollectionItem::Folder(folder)) = collection_item_mut(items, target_indices)
        else {
            return false;
        };
        folder.items.push(item);
        return true;
    }

    let Some(index) = target_indices.last().copied() else {
        return false;
    };
    let Some(parent) = collection_parent_items_mut(items, target_indices) else {
        return false;
    };
    let insert_at = (index + 1).min(parent.len());
    parent.insert(insert_at, item);
    true
}

fn collection_item_copy(mut item: CollectionItem) -> CollectionItem {
    match &mut item {
        CollectionItem::Folder(folder) => folder.name = format!("{} Copy", folder.name),
        CollectionItem::Request(request) => request.name = format!("{} Copy", request.name),
    }
    item
}

fn collection_node_indices(node_id: &str) -> Option<Vec<usize>> {
    if node_id == "collection" {
        return Some(Vec::new());
    }

    let path = node_id.strip_prefix("collection/")?;
    path.split('/')
        .map(|segment| segment.parse::<usize>().ok())
        .collect()
}

fn collection_item_ref<'a>(
    items: &'a [CollectionItem],
    indices: &[usize],
) -> Option<&'a CollectionItem> {
    let (index, rest) = indices.split_first()?;
    let item = items.get(*index)?;
    if rest.is_empty() {
        return Some(item);
    }

    match item {
        CollectionItem::Folder(folder) => collection_item_ref(&folder.items, rest),
        CollectionItem::Request(_) => None,
    }
}

fn collection_item_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<&'a mut CollectionItem> {
    let (index, rest) = indices.split_first()?;
    let item = items.get_mut(*index)?;
    if rest.is_empty() {
        return Some(item);
    }

    match item {
        CollectionItem::Folder(folder) => collection_item_mut(&mut folder.items, rest),
        CollectionItem::Request(_) => None,
    }
}

fn collection_parent_items_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<&'a mut Vec<CollectionItem>> {
    if indices.is_empty() || indices.len() == 1 {
        return Some(items);
    }

    let index = indices[0];
    match items.get_mut(index)? {
        CollectionItem::Folder(folder) => {
            collection_parent_items_mut(&mut folder.items, &indices[1..])
        }
        CollectionItem::Request(_) => None,
    }
}

fn collection_insertion_items_mut<'a>(
    items: &'a mut Vec<CollectionItem>,
    indices: &[usize],
) -> Option<&'a mut Vec<CollectionItem>> {
    if indices.is_empty() {
        return Some(items);
    }

    if indices.len() == 1 {
        let index = indices[0];
        let target_is_folder = matches!(items.get(index)?, CollectionItem::Folder(_));
        if !target_is_folder {
            return Some(items);
        }

        return match items.get_mut(index)? {
            CollectionItem::Folder(folder) => Some(&mut folder.items),
            CollectionItem::Request(_) => None,
        };
    }

    let index = indices[0];
    match items.get_mut(index)? {
        CollectionItem::Folder(folder) => {
            collection_insertion_items_mut(&mut folder.items, &indices[1..])
        }
        CollectionItem::Request(_) => None,
    }
}

fn preview_pairs(pairs: &[(String, String)]) -> String {
    preview_text(
        &pairs
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("&"),
    )
}

#[cfg(test)]
fn websocket_log_entries(exchange: &client::WebSocketExchange) -> Vec<WebSocketLogEntry> {
    let mut entries = vec![WebSocketLogEntry {
        direction: WebSocketDirection::Sent,
        kind: "text".to_string(),
        data: exchange.sent.clone(),
    }];

    entries.extend(exchange.received.iter().map(|message| WebSocketLogEntry {
        direction: WebSocketDirection::Received,
        kind: websocket_message_kind_label(&message.kind).to_string(),
        data: message.data.clone(),
    }));

    entries
}

#[cfg(test)]
fn websocket_exchange_text(exchange: &client::WebSocketExchange) -> String {
    let mut lines = vec![
        format!("URL {}", exchange.url),
        format!("RX {}", exchange.received.len()),
        format!("TX text {}", exchange.sent),
    ];

    for message in &exchange.received {
        lines.push(format!(
            "RX {} {}",
            websocket_message_kind_label(&message.kind),
            message.data
        ));
    }

    lines.join("\n")
}

fn format_websocket_log(entries: &[WebSocketLogEntry]) -> String {
    entries
        .iter()
        .map(|entry| {
            let direction = match entry.direction {
                WebSocketDirection::Sent => "TX",
                WebSocketDirection::Received => "RX",
            };
            format!("{direction} {} {}", entry.kind, entry.data)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn websocket_message_kind_label(kind: &client::WebSocketMessageKind) -> &'static str {
    match kind {
        client::WebSocketMessageKind::Text => "text",
        client::WebSocketMessageKind::Binary => "binary",
        client::WebSocketMessageKind::Ping => "ping",
        client::WebSocketMessageKind::Pong => "pong",
        client::WebSocketMessageKind::Close => "close",
    }
}

fn websocket_hex_bytes(input: &str) -> std::result::Result<Vec<u8>, String> {
    let digits = input
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '_' && *ch != '-')
        .collect::<String>();
    if digits.is_empty() {
        return Err(WEBSOCKET_BINARY_EMPTY_BODY.to_string());
    }
    if digits.len() % 2 != 0 {
        return Err(WEBSOCKET_BINARY_ODD_DIGITS_BODY.to_string());
    }

    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for index in (0..digits.len()).step_by(2) {
        let byte = u8::from_str_radix(&digits[index..index + 2], 16)
            .map_err(|_| format!("Bad hex at {index}."))?;
        bytes.push(byte);
    }

    Ok(bytes)
}

fn websocket_protocol_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|protocol| !protocol.is_empty())
        .map(str::to_string)
        .collect()
}

fn sse_log_entries(exchange: &client::SseExchange) -> Vec<SseLogEntry> {
    exchange.events.iter().map(sse_log_entry).collect()
}

fn sse_log_entry(event: &client::SseEvent) -> SseLogEntry {
    SseLogEntry {
        event: sse_event_label(event).to_string(),
        data: event.data.clone(),
        id: event.id.clone(),
    }
}

fn format_sse_log(entries: &[SseLogEntry]) -> String {
    entries
        .iter()
        .map(|entry| match &entry.id {
            Some(id) => format!("{} #{} {}", entry.event, id, entry.data),
            None => format!("{} {}", entry.event, entry.data),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn sse_reconnect_status(attempt: usize, delay_ms: u64) -> String {
    format!("r{attempt} {delay_ms}ms")
}

fn sse_exchange_text(exchange: &client::SseExchange) -> String {
    let mut lines = vec![
        format!("URL {}", exchange.url),
        sse_event_count_label(exchange.events.len()),
    ];

    for event in &exchange.events {
        let mut line = sse_event_label(event).to_string();
        if let Some(id) = &event.id {
            line.push_str(&format!(" #{id}"));
        }
        line.push(' ');
        line.push_str(&event.data);
        if let Some(retry) = event.retry {
            line.push_str(&format!(" r{retry}"));
        }
        lines.push(line);
    }

    lines.join("\n")
}

fn sse_event_label(event: &client::SseEvent) -> &str {
    event.event.as_deref().unwrap_or("message")
}

fn sse_event_count_label(count: usize) -> String {
    match count {
        1 => "1 event".to_string(),
        count => format!("{count} events"),
    }
}

fn graphql_body(query: &str, variables: &str) -> String {
    let variables = graphql_variables_value(variables);
    pretty_json(&serde_json::json!({
        "query": query,
        "variables": variables,
    }))
}

fn graphql_variables_value(input: &str) -> serde_json::Value {
    let input = input.trim();
    if input.is_empty() {
        return serde_json::json!({});
    }

    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        _ => serde_json::json!({}),
    }
}

fn graphql_fields_from_body(content_type: &str, body: &str) -> Option<(String, String)> {
    if !content_type.to_ascii_lowercase().contains("json") {
        return None;
    }

    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let object = value.as_object()?;
    let query = object.get("query")?.as_str()?.to_string();
    let variables = object
        .get("variables")
        .and_then(|variables| serde_json::to_string_pretty(variables).ok())
        .unwrap_or_else(|| "{}".to_string());
    Some((query, variables))
}

fn graphql_schema_summary(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    let types = schema.get("types")?.as_array()?;
    let directives = schema
        .get("directives")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    let query_type = schema
        .get("queryType")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let mutation_type = schema
        .get("mutationType")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");
    let subscription_type = schema
        .get("subscriptionType")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("-");

    let object_count = graphql_type_kind_count(types, "OBJECT");
    let input_count = graphql_type_kind_count(types, "INPUT_OBJECT");
    let enum_count = graphql_type_kind_count(types, "ENUM");
    let scalar_count = graphql_type_kind_count(types, "SCALAR");
    let query_fields = graphql_type_field_count(types, query_type);
    let mutation_fields = graphql_type_field_count(types, mutation_type);
    let subscription_fields = graphql_type_field_count(types, subscription_type);

    Some(
        [
            format!("Roots Q={query_type} M={mutation_type} S={subscription_type}"),
            format!(
                "Types {} | O{object_count} I{input_count} E{enum_count} S{scalar_count}",
                types.len()
            ),
            format!("Fields Q{query_fields} M{mutation_fields} S{subscription_fields}"),
            format!("Dirs {directives}"),
        ]
        .join("\n"),
    )
}

fn graphql_schema_browser(body: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    let types = schema.get("types")?.as_array()?;

    let mut sections = Vec::new();
    for (label, key) in [
        ("Q", "queryType"),
        ("M", "mutationType"),
        ("S", "subscriptionType"),
    ] {
        if let Some(type_name) = graphql_schema_root_type_name(schema, key) {
            if let Some(section) = graphql_root_fields_section(types, label, type_name) {
                sections.push(section);
            }
        }
    }

    if let Some(section) = graphql_type_index_section(types) {
        sections.push(section);
    }
    if let Some(section) = graphql_directives_section(schema) {
        sections.push(section);
    }

    (!sections.is_empty()).then(|| sections.join("\n\n"))
}

fn graphql_query_templates(body: &str) -> Option<Vec<GraphqlQueryTemplate>> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let schema = value.get("data")?.get("__schema")?;
    let types = schema.get("types")?.as_array()?;
    let query_type = graphql_schema_root_type_name(schema, "queryType")?;
    let fields = graphql_type_by_name(types, query_type)?
        .get("fields")?
        .as_array()?;
    let templates = fields
        .iter()
        .filter_map(graphql_query_template_from_field)
        .take(GRAPHQL_QUERY_TEMPLATE_LIMIT)
        .collect::<Vec<_>>();

    (!templates.is_empty()).then_some(templates)
}

fn graphql_query_template_from_field(field: &serde_json::Value) -> Option<GraphqlQueryTemplate> {
    let field_name = field.get("name")?.as_str()?.to_string();
    let args = graphql_operation_args(field);
    let operation_name = format!("{}Query", graphql_pascal_case(&field_name));
    let variable_definitions = graphql_operation_variable_definitions(&args);
    let field_arguments = graphql_operation_field_arguments(&args);
    let selection = if field.get("type").is_some_and(graphql_type_ref_is_leaf) {
        String::new()
    } else {
        " {\n    __typename\n  }".to_string()
    };
    let operation = format!(
        "query {operation_name}{variable_definitions} {{\n  {field_name}{field_arguments}{selection}\n}}"
    );

    Some(GraphqlQueryTemplate {
        field_name,
        operation,
        variables: graphql_operation_variables(&args),
    })
}

fn graphql_operation_args(field: &serde_json::Value) -> Vec<GraphqlOperationArg> {
    field
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(graphql_operation_arg)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn graphql_operation_arg(arg: &serde_json::Value) -> Option<GraphqlOperationArg> {
    let name = arg.get("name")?.as_str()?.to_string();
    let type_ref_value = arg.get("type");
    let type_ref = type_ref_value
        .map(graphql_type_ref)
        .unwrap_or_else(|| "?".to_string());
    let default_value = arg
        .get("defaultValue")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let placeholder = type_ref_value
        .map(|value| graphql_variable_placeholder(&name, value))
        .unwrap_or_else(|| serde_json::json!(format!("<{name}>")));

    Some(GraphqlOperationArg {
        name,
        type_ref,
        default_value,
        placeholder,
    })
}

fn graphql_operation_variable_definitions(args: &[GraphqlOperationArg]) -> String {
    if args.is_empty() {
        return String::new();
    }

    format!(
        "({})",
        args.iter()
            .map(|arg| {
                let default_value = arg
                    .default_value
                    .as_ref()
                    .map(|value| format!(" = {value}"))
                    .unwrap_or_default();
                format!("${}: {}{}", arg.name, arg.type_ref, default_value)
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn graphql_operation_field_arguments(args: &[GraphqlOperationArg]) -> String {
    if args.is_empty() {
        return String::new();
    }

    format!(
        "({})",
        args.iter()
            .map(|arg| format!("{}: ${}", arg.name, arg.name))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn graphql_operation_variables(args: &[GraphqlOperationArg]) -> String {
    let variables = args
        .iter()
        .map(|arg| (arg.name.clone(), arg.placeholder.clone()))
        .collect::<serde_json::Map<_, _>>();
    pretty_json(&serde_json::Value::Object(variables))
}

fn graphql_variable_placeholder(name: &str, type_ref: &serde_json::Value) -> serde_json::Value {
    match graphql_type_ref_base_name(type_ref) {
        Some("Int") => serde_json::json!(0),
        Some("Float") => serde_json::json!(0.0),
        Some("Boolean") => serde_json::json!(false),
        _ => serde_json::json!(format!("<{name}>")),
    }
}

fn graphql_type_ref_is_leaf(type_ref: &serde_json::Value) -> bool {
    matches!(
        graphql_type_ref_base_kind(type_ref),
        Some("SCALAR" | "ENUM")
    )
}

fn graphql_type_ref_base_kind(type_ref: &serde_json::Value) -> Option<&str> {
    let kind = type_ref
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .filter(|kind| !kind.is_empty())?;

    match kind {
        "NON_NULL" | "LIST" => type_ref
            .get("ofType")
            .filter(|value| !value.is_null())
            .and_then(graphql_type_ref_base_kind),
        _ => Some(kind),
    }
}

fn graphql_type_ref_base_name(type_ref: &serde_json::Value) -> Option<&str> {
    let kind = type_ref
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .filter(|kind| !kind.is_empty())?;

    match kind {
        "NON_NULL" | "LIST" => type_ref
            .get("ofType")
            .filter(|value| !value.is_null())
            .and_then(graphql_type_ref_base_name),
        _ => type_ref.get("name").and_then(serde_json::Value::as_str),
    }
}

fn graphql_pascal_case(input: &str) -> String {
    let mut output = String::new();
    let mut uppercase_next = true;

    for ch in input.chars() {
        if !ch.is_ascii_alphanumeric() {
            uppercase_next = true;
            continue;
        }

        if uppercase_next {
            output.push(ch.to_ascii_uppercase());
        } else {
            output.push(ch);
        }
        uppercase_next = false;
    }

    if output.is_empty() {
        "Operation".to_string()
    } else {
        output
    }
}

fn graphql_schema_root_type_name<'a>(schema: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    schema
        .get(key)?
        .get("name")?
        .as_str()
        .filter(|name| !name.is_empty())
}

fn graphql_root_fields_section(
    types: &[serde_json::Value],
    label: &str,
    type_name: &str,
) -> Option<String> {
    let fields = graphql_type_by_name(types, type_name)?
        .get("fields")?
        .as_array()?;
    let signatures = fields
        .iter()
        .filter_map(graphql_field_signature)
        .collect::<Vec<_>>();

    let mut lines = vec![format!("{label} ({type_name})")];
    if signatures.is_empty() {
        lines.push("  -".to_string());
    } else {
        lines.extend(
            signatures
                .iter()
                .take(GRAPHQL_SCHEMA_FIELD_LIMIT)
                .map(|signature| format!("  {signature}")),
        );
        if signatures.len() > GRAPHQL_SCHEMA_FIELD_LIMIT {
            lines.push(format!(
                "  ... {} more",
                signatures.len() - GRAPHQL_SCHEMA_FIELD_LIMIT
            ));
        }
    }

    Some(lines.join("\n"))
}

fn graphql_field_signature(field: &serde_json::Value) -> Option<String> {
    let name = field.get("name")?.as_str()?;
    let args = field
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(graphql_arg_signature)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let return_type = field
        .get("type")
        .map(graphql_type_ref)
        .unwrap_or_else(|| "?".to_string());
    let call = if args.is_empty() {
        name.to_string()
    } else {
        format!("{name}({}):", args.join(", "))
    };
    let deprecated = field
        .get("isDeprecated")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
        .then_some(" @deprecated")
        .unwrap_or_default();

    if args.is_empty() {
        Some(format!("{call}: {return_type}{deprecated}"))
    } else {
        Some(format!("{call} {return_type}{deprecated}"))
    }
}

fn graphql_arg_signature(arg: &serde_json::Value) -> Option<String> {
    let name = arg.get("name")?.as_str()?;
    let arg_type = arg
        .get("type")
        .map(graphql_type_ref)
        .unwrap_or_else(|| "?".to_string());
    let default_value = arg
        .get("defaultValue")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| format!(" = {value}"))
        .unwrap_or_default();

    Some(format!("{name}: {arg_type}{default_value}"))
}

fn graphql_type_ref(type_ref: &serde_json::Value) -> String {
    let kind = type_ref
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let name = type_ref
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.is_empty());

    match kind {
        "NON_NULL" => {
            let inner = type_ref
                .get("ofType")
                .filter(|value| !value.is_null())
                .map(graphql_type_ref)
                .unwrap_or_else(|| "?".to_string());
            format!("{inner}!")
        }
        "LIST" => {
            let inner = type_ref
                .get("ofType")
                .filter(|value| !value.is_null())
                .map(graphql_type_ref)
                .unwrap_or_else(|| "?".to_string());
            format!("[{inner}]")
        }
        _ => name.map(str::to_string).unwrap_or_else(|| {
            (!kind.is_empty())
                .then(|| kind.to_string())
                .unwrap_or("?".to_string())
        }),
    }
}

fn graphql_type_index_section(types: &[serde_json::Value]) -> Option<String> {
    let mut lines = vec!["Types".to_string()];
    for (label, kind) in [
        ("Obj", "OBJECT"),
        ("In", "INPUT_OBJECT"),
        ("Enum", "ENUM"),
        ("Scalar", "SCALAR"),
    ] {
        let names = graphql_type_names_by_kind(types, kind);
        if !names.is_empty() {
            lines.push(format!(
                "  {label}: {}",
                graphql_limited_list(&names, GRAPHQL_SCHEMA_TYPE_LIMIT)
            ));
        }
    }

    (lines.len() > 1).then(|| lines.join("\n"))
}

fn graphql_type_names_by_kind(types: &[serde_json::Value], kind: &str) -> Vec<String> {
    let mut names = types
        .iter()
        .filter(|value| {
            value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == kind)
        })
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .filter(|name| !name.starts_with("__"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn graphql_directives_section(schema: &serde_json::Value) -> Option<String> {
    let mut names = schema
        .get("directives")?
        .as_array()?
        .iter()
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(|name| format!("@{name}"))
        .collect::<Vec<_>>();
    names.sort_unstable();

    (!names.is_empty()).then(|| {
        format!(
            "Dirs\n  {}",
            graphql_limited_list(&names, GRAPHQL_SCHEMA_TYPE_LIMIT)
        )
    })
}

fn graphql_limited_list(names: &[String], limit: usize) -> String {
    let rendered = names
        .iter()
        .take(limit)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > limit {
        format!("{rendered} (+{} more)", names.len() - limit)
    } else {
        rendered
    }
}

fn graphql_type_by_name<'a>(
    types: &'a [serde_json::Value],
    name: &str,
) -> Option<&'a serde_json::Value> {
    types.iter().find(|value| {
        value
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == name)
    })
}

fn graphql_type_kind_count(types: &[serde_json::Value], kind: &str) -> usize {
    types
        .iter()
        .filter(|value| {
            value
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == kind)
        })
        .count()
}

fn graphql_type_field_count(types: &[serde_json::Value], name: &str) -> usize {
    if name == "-" {
        return 0;
    }

    types
        .iter()
        .find(|value| {
            value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|value| value == name)
        })
        .and_then(|value| value.get("fields"))
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len)
}

fn preview_text(text: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 240;

    let mut preview = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= MAX_PREVIEW_CHARS {
            preview.push_str("...");
            break;
        }
        preview.push(ch);
    }
    preview
}

impl ZenApiApp {
    fn key_value_editor(
        &self,
        title: &'static str,
        rows: &[KeyValueRow],
        target: KeyValueEditorTarget,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let key_column_width = key_value_editor_key_column_width(title);
        let (key_label, value_label) = key_value_editor_column_labels(title);
        let row_count = rows.len();
        let rendered_rows = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                self.key_value_row(row, target, index, row_count, key_column_width, cx)
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(PANEL_TITLE_TEXT_SIZE))
                            .font_weight(FontWeight::BOLD)
                            .text_color(ui_text_primary())
                            .child(title),
                    )
                    .child(self.key_value_add_button(target, cx)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .min_w_0()
                    .overflow_hidden()
                    .text_size(px(TABLE_HEADER_TEXT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui_text_body())
                    .child(
                        div()
                            .w(px(key_column_width))
                            .flex_shrink_0()
                            .truncate()
                            .child(key_label),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(value_label))
                    .child(
                        div()
                            .w(px(KEY_VALUE_ROW_ACTION_BUTTON_WIDTH))
                            .flex_shrink_0(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap_1()
                    .children(rendered_rows),
            )
    }

    fn key_value_add_button(
        &self,
        target: KeyValueEditorTarget,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = can_add_key_value_row(self.busy, target, self.active_environment.is_some());
        let colors = ButtonTone::Neutral.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(COMPACT_CONTROL_HEIGHT))
            .w(px(KEY_VALUE_ROW_ACTION_BUTTON_WIDTH))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(COMPACT_SYMBOL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.add_key_value_row(target, cx);
                }),
            )
            .child(div().min_w_0().truncate().child("+"))
    }

    fn key_value_row(
        &self,
        row: &KeyValueRow,
        target: KeyValueEditorTarget,
        index: usize,
        row_count: usize,
        key_column_width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        div()
            .flex()
            .items_center()
            .min_w_0()
            .overflow_hidden()
            .gap_2()
            .child(
                div()
                    .w(px(key_column_width))
                    .flex_shrink_0()
                    .child(bounded_text_input(row.key.clone())),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(bounded_text_input(row.value.clone())),
            )
            .child(self.key_value_remove_button(target, index, row_count, cx))
    }

    fn key_value_remove_button(
        &self,
        target: KeyValueEditorTarget,
        index: usize,
        row_count: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let enabled = can_remove_key_value_row(
            self.busy,
            target,
            self.active_environment.is_some(),
            row_count,
            index,
        );
        let colors = ButtonTone::Neutral.colors(enabled);

        div()
            .flex()
            .items_center()
            .justify_center()
            .h(px(COMPACT_CONTROL_HEIGHT))
            .w(px(KEY_VALUE_ROW_ACTION_BUTTON_WIDTH))
            .flex_shrink_0()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(UI_RADIUS_CONTROL))
            .border_1()
            .border_color(colors.border)
            .bg(colors.background)
            .text_size(px(COMPACT_SYMBOL_TEXT_SIZE))
            .font_weight(FontWeight::BOLD)
            .text_color(colors.text)
            .opacity(control_opacity(enabled))
            .when(enabled, |button| button.cursor_pointer())
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    app.remove_key_value_row(target, index, cx);
                }),
            )
            .child(div().min_w_0().truncate().child("x"))
    }
}

fn key_value_editor_key_column_width(title: &str) -> f32 {
    match title {
        BODY_FORM_FIELDS_TITLE | BODY_URL_ENCODED_TITLE => {
            KEY_VALUE_EDITOR_COMPACT_KEY_COLUMN_WIDTH
        }
        _ => KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH,
    }
}

fn key_value_editor_column_labels(title: &str) -> (&'static str, &'static str) {
    match title {
        HEADERS_PANEL_TITLE | REALTIME_WEBSOCKET_HEADERS_TITLE | REALTIME_SSE_HEADERS_TITLE => {
            ("Header", "Value")
        }
        PARAMS_PANEL_TITLE => ("Param", "Value"),
        VARIABLES_GLOBAL_TITLE | VARIABLES_ENV_TITLE => ("Var", "Value"),
        BODY_FORM_FIELDS_TITLE => ("Field", "Value"),
        BODY_URL_ENCODED_TITLE => ("Name", "Value"),
        _ => ("Key", "Value"),
    }
}

fn key_value_editor_add_placeholders(target: KeyValueEditorTarget) -> (&'static str, &'static str) {
    match target {
        KeyValueEditorTarget::QueryParams => ("param", "value"),
        KeyValueEditorTarget::RequestHeaders
        | KeyValueEditorTarget::WebSocketHeaders
        | KeyValueEditorTarget::SseHeaders => ("Header", "Value"),
        KeyValueEditorTarget::GlobalVariables
        | KeyValueEditorTarget::ActiveEnvironmentVariables => ("var", "value"),
        KeyValueEditorTarget::FormDataBody => ("field", "value"),
        KeyValueEditorTarget::UrlEncodedBody => ("name", "value"),
    }
}

fn bounded_text_input(input: Entity<TextInput>) -> gpui::Div {
    div().min_w_0().overflow_hidden().child(input)
}

fn panel_button_row() -> gpui::Div {
    div()
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .gap_2()
}

fn empty_state_row(label: impl Into<SharedString>, height: f32) -> gpui::Div {
    div()
        .h(px(height))
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .text_color(ui_text_body())
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(div().min_w_0().truncate().child(label.into()))
}

fn fixed_row_cell(
    label: impl Into<SharedString>,
    width: f32,
    color: Hsla,
    bold: bool,
    align_right: bool,
) -> gpui::Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .truncate()
        .text_color(color)
        .when(bold, |cell| cell.font_weight(FontWeight::BOLD))
        .when(align_right, |cell| cell.text_right())
        .child(label.into())
}

fn sidebar_section_header(title: &'static str, trailing: gpui::Div) -> gpui::Div {
    div()
        .flex()
        .justify_between()
        .items_center()
        .min_w_0()
        .gap_2()
        .h(px(SECTION_HEADER_HEIGHT))
        .text_size(px(SIDEBAR_NAV_TEXT_SIZE))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_primary())
                .child(title),
        )
        .child(trailing)
}

fn sidebar_count_text(count: impl Into<SharedString>) -> gpui::Div {
    div()
        .min_w_0()
        .flex_shrink_1()
        .truncate()
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_color(ui_text_secondary())
        .child(count.into())
}

fn sidebar_status_text(status: impl Into<SharedString>) -> gpui::Div {
    div()
        .min_w_0()
        .flex_shrink_1()
        .truncate()
        .text_color(ui_text_secondary())
        .child(status.into())
}

fn sidebar_small_button(
    label: &'static str,
    width: f32,
    height: f32,
    enabled: bool,
    tone: ButtonTone,
) -> gpui::Div {
    let colors = tone.colors(enabled);

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(height))
        .w(px(width))
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_TIGHT))
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .text_size(px(COMPACT_SYMBOL_TEXT_SIZE))
        .font_weight(FontWeight::BOLD)
        .text_color(colors.text)
        .opacity(control_opacity(enabled))
        .when(enabled, |button| button.cursor_pointer())
        .child(div().min_w_0().truncate().child(label))
}

fn assertion_editor_row(
    index: usize,
    row: &TestAssertionRow,
    row_count: usize,
    busy: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::Div {
    let kind = row.kind;
    let can_edit = can_edit_response_assertion_row(busy, row_count, index);
    div()
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .gap_2()
        .child(
            div()
                .w(px(TEST_ASSERTION_NAME_COLUMN_WIDTH))
                .flex_shrink_0()
                .child(bounded_text_input(row.name.clone())),
        )
        .child(
            compact_toggle_enabled(kind.label(), true, can_edit)
                .w(px(TEST_ASSERTION_KIND_COLUMN_WIDTH))
                .flex_shrink_0()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                        app.cycle_response_assertion_kind(index, cx);
                    }),
                )
                .child(div().min_w_0().truncate().child(kind.label())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(bounded_text_input(row.target.clone())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(bounded_text_input(row.expected.clone())),
        )
        .child(assertion_remove_button(index, row_count, busy, cx))
}

fn assertion_remove_button(
    index: usize,
    row_count: usize,
    busy: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::Div {
    let enabled = can_remove_response_assertion_row(busy, row_count, index);
    let colors = ButtonTone::Neutral.colors(enabled);

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(COMPACT_CONTROL_HEIGHT))
        .w(px(KEY_VALUE_ROW_ACTION_BUTTON_WIDTH))
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_CONTROL))
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .text_size(px(COMPACT_SYMBOL_TEXT_SIZE))
        .font_weight(FontWeight::BOLD)
        .text_color(colors.text)
        .opacity(control_opacity(enabled))
        .when(enabled, |button| button.cursor_pointer())
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                app.remove_response_assertion_row(index, cx);
            }),
        )
        .child(div().min_w_0().truncate().child("x"))
}

fn assertion_result_row(result: ResponseAssertionResult) -> gpui::Div {
    let tone = if result.passed {
        ResponseTone::Success
    } else {
        ResponseTone::Error
    };
    div()
        .flex()
        .items_center()
        .h(px(RESULT_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(
            fixed_row_cell(
                if result.passed { "PASS" } else { "FAIL" },
                TEST_RESULT_STATUS_COLUMN_WIDTH,
                tone.color(),
                true,
                false,
            )
            .font_family(PLATFORM_MONOSPACE_FONT)
            .text_size(px(PANEL_CONTENT_TEXT_SIZE)),
        )
        .child(
            fixed_row_cell(
                result.name,
                TEST_RESULT_NAME_COLUMN_WIDTH,
                ui_text_body(),
                true,
                false,
            )
            .text_size(px(PANEL_CONTENT_TEXT_SIZE)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(ui_text_body())
                .child(result.error.unwrap_or_else(|| "ok".to_string())),
        )
}

fn pre_request_action_row(action: String) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .h(px(RESULT_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(
            fixed_row_cell(
                "PASS",
                TEST_RESULT_STATUS_COLUMN_WIDTH,
                ResponseTone::Success.color(),
                true,
                false,
            )
            .font_family(PLATFORM_MONOSPACE_FONT)
            .text_size(px(PANEL_CONTENT_TEXT_SIZE)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_body())
                .child(action),
        )
}

fn compact_toggle_width(label: &str) -> f32 {
    if label.len() > COMPACT_TOGGLE_LONG_LABEL_THRESHOLD {
        COMPACT_TOGGLE_LONG_WIDTH
    } else {
        COMPACT_TOGGLE_SHORT_WIDTH
    }
}

fn compact_toggle_enabled(label: &str, active: bool, enabled: bool) -> gpui::Div {
    let width = px(compact_toggle_width(label));

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(COMPACT_CONTROL_HEIGHT))
        .w(width)
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .rounded(px(UI_RADIUS_CONTROL))
        .border_1()
        .border_color(if !enabled {
            ui_disabled_border()
        } else if active {
            ui_accent()
        } else {
            ui_border_strong()
        })
        .bg(if enabled {
            ui_surface()
        } else {
            ui_disabled_surface()
        })
        .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
        .font_weight(FontWeight::BOLD)
        .text_color(if !enabled {
            ui_disabled_text()
        } else if active {
            ui_accent()
        } else {
            ui_text_body()
        })
        .opacity(control_opacity(enabled))
        .when(enabled, |toggle| toggle.cursor_pointer())
}

fn panel_status_text(label: impl Into<SharedString>, color: Hsla) -> gpui::Div {
    div()
        .min_w_0()
        .flex_shrink_1()
        .truncate()
        .text_right()
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(PANEL_META_TEXT_SIZE))
        .text_color(color)
        .child(label.into())
}

fn response_toolbar_button(label: &'static str, width: f32, enabled: bool) -> gpui::Div {
    let colors = ButtonTone::Neutral.colors(enabled);

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(COMPACT_CONTROL_HEIGHT))
        .w(px(width))
        .flex_shrink_0()
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_CONTROL))
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
        .font_weight(FontWeight::BOLD)
        .text_color(colors.text)
        .opacity(control_opacity(enabled))
        .when(enabled, |button| button.cursor_pointer())
        .child(div().min_w_0().truncate().child(label))
}

fn flexible_toggle_enabled(
    label: impl Into<SharedString>,
    active: bool,
    enabled: bool,
) -> gpui::Div {
    let label = label.into();

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(COMPACT_CONTROL_HEIGHT))
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .rounded(px(UI_RADIUS_CONTROL))
        .border_1()
        .border_color(if !enabled {
            ui_disabled_border()
        } else if active {
            ui_accent()
        } else {
            ui_border_strong()
        })
        .bg(if !enabled {
            ui_disabled_surface()
        } else if active {
            ui_surface()
        } else {
            ui_surface()
        })
        .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
        .font_weight(FontWeight::BOLD)
        .text_color(if !enabled {
            ui_disabled_text()
        } else if active {
            ui_accent()
        } else {
            ui_text_body()
        })
        .opacity(control_opacity(enabled))
        .when(enabled, |toggle| toggle.cursor_pointer())
        .child(div().min_w_0().truncate().child(label))
}

fn full_width_toggle_enabled(
    label: impl Into<SharedString>,
    active: bool,
    enabled: bool,
) -> gpui::Div {
    let label = label.into();

    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(COMPACT_CONTROL_HEIGHT))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .rounded(px(UI_RADIUS_CONTROL))
        .border_1()
        .border_color(if !enabled {
            ui_disabled_border()
        } else if active {
            ui_accent()
        } else {
            ui_border_strong()
        })
        .bg(if enabled {
            ui_surface()
        } else {
            ui_disabled_surface()
        })
        .text_size(px(COMPACT_CONTROL_TEXT_SIZE))
        .font_weight(FontWeight::BOLD)
        .text_color(if !enabled {
            ui_disabled_text()
        } else if active {
            ui_accent()
        } else {
            ui_text_body()
        })
        .opacity(control_opacity(enabled))
        .when(enabled, |toggle| toggle.cursor_pointer())
        .child(div().min_w_0().truncate().child(label))
}

fn bearer_auth_pair(token: &str) -> Option<(String, String)> {
    let token = token.trim();
    (!token.is_empty()).then(|| ("Authorization".to_string(), format!("Bearer {token}")))
}

fn jwt_auth_pair(token: &str) -> Option<(String, String)> {
    bearer_auth_pair(token)
}

fn oauth2_access_token_pair(token: &str) -> Option<(String, String)> {
    bearer_auth_pair(token)
}

fn basic_auth_pair(username: &str, password: &str) -> Option<(String, String)> {
    let username = username.trim();
    if username.is_empty() {
        return None;
    }

    let credentials = format!("{username}:{}", password.trim());
    Some((
        "Authorization".to_string(),
        format!("Basic {}", BASE64_STANDARD.encode(credentials)),
    ))
}

fn api_key_pair(name: &str, value: &str) -> Option<(String, String)> {
    let name = name.trim();
    (!name.is_empty()).then(|| (name.to_string(), value.trim().to_string()))
}

fn snippet_language_label(language: SnippetLanguage) -> &'static str {
    match language {
        SnippetLanguage::Curl => "cURL",
        SnippetLanguage::PythonRequests => "Py",
        SnippetLanguage::JavaScriptFetch => "JS",
        SnippetLanguage::RustReqwest => "Rust",
        SnippetLanguage::GoNetHttp => "Go",
    }
}

fn panel_header(
    title: impl Into<SharedString>,
    meta: Option<&str>,
    tone: ResponseTone,
) -> impl IntoElement {
    let title = title.into();
    let meta = panel_header_meta_text(meta);
    let has_meta = meta.is_some();
    div()
        .flex()
        .flex_col()
        .h(px(PANEL_HEADER_HEIGHT))
        .border_b_1()
        .border_color(ui_border())
        .bg(ui_surface())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .flex_1()
                .min_w_0()
                .pl_3()
                .pr(px(PANEL_HEADER_RIGHT_PADDING))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(PANE_HEADER_TITLE_TEXT_SIZE))
                        .text_color(ui_text_primary())
                        .child(title.clone()),
                )
                .when(has_meta, move |row| {
                    row.child(
                        div()
                            .max_w(px(PANEL_HEADER_META_MAX_WIDTH))
                            .flex_shrink_1()
                            .min_w_0()
                            .truncate()
                            .text_right()
                            .font_family(PLATFORM_MONOSPACE_FONT)
                            .text_size(px(PANEL_META_TEXT_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(tone.color())
                            .child(meta.clone().unwrap_or_default()),
                    )
                }),
        )
        .child(
            div()
                .ml(px(PANEL_HEADER_UNDERLINE_LEFT_OFFSET))
                .h(px(PANEL_HEADER_UNDERLINE_HEIGHT))
                .w(px(PANEL_HEADER_UNDERLINE_WIDTH))
                .bg(ui_accent()),
        )
}

fn panel_header_meta_text(meta: Option<&str>) -> Option<String> {
    meta.map(str::trim)
        .filter(|meta| !meta.is_empty())
        .map(str::to_string)
}

fn runner_status_text(summary: &CollectionRunSummary) -> String {
    let suffix = if summary.stopped_early {
        " / stopped"
    } else {
        ""
    };
    format!(
        "P {} / F {} / T {}{suffix}",
        summary.passed, summary.failed, summary.total
    )
}

fn runner_summary_text(summary: &CollectionRunSummary) -> String {
    let mut lines = vec![format!(
        "{}\n{}",
        summary.collection_name,
        runner_status_text(summary)
    )];

    for result in &summary.results {
        let status = result
            .status
            .map(|status| status.to_string())
            .unwrap_or_else(|| "ERR".to_string());
        let outcome = runner_outcome_label(result.success);
        let mut line = format!("[{outcome}] {status} {} {}", result.method, result.url);
        if let Some(error) = &result.error {
            line.push_str(&format!(" - {error}"));
        }
        lines.push(line);
        for action in &result.pre_request_actions {
            lines.push(format!("  [OK] pre {action}"));
        }
        for assertion in &result.assertions {
            let outcome = runner_outcome_label(assertion.passed);
            let mut line = format!("  [{outcome}] test {}", assertion.name);
            if let Some(error) = &assertion.error {
                line.push_str(&format!(" - {error}"));
            }
            lines.push(line);
        }
    }

    lines.join("\n")
}

fn runner_outcome_label(success: bool) -> &'static str {
    if success { "OK" } else { "ERR" }
}

fn runner_result_row(result: CollectionRunResult) -> impl IntoElement {
    let status = result
        .status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "ERR".to_string());
    let tone = if result.success {
        ResponseTone::Success
    } else {
        ResponseTone::Error
    };
    let path = result.path.join(" / ");
    let method_color = method_color(&result.method);

    div()
        .flex()
        .items_center()
        .h(px(RESULT_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(fixed_row_cell(
            result.method,
            RUNNER_METHOD_COLUMN_WIDTH,
            method_color,
            true,
            false,
        ))
        .child(fixed_row_cell(
            status,
            RUNNER_STATUS_COLUMN_WIDTH,
            tone.color(),
            false,
            false,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(ui_text_body())
                .child(path),
        )
        .when(!result.pre_request_actions.is_empty(), |row| {
            row.child(fixed_row_cell(
                format!("pre {}", result.pre_request_actions.len()),
                RUNNER_PRE_REQUEST_COLUMN_WIDTH,
                ResponseTone::Success.color(),
                false,
                true,
            ))
        })
        .when(!result.assertions.is_empty(), |row| {
            let failed = result
                .assertions
                .iter()
                .filter(|assertion| !assertion.passed)
                .count();
            row.child(fixed_row_cell(
                format!(
                    "{}/{}",
                    result.assertions.len() - failed,
                    result.assertions.len()
                ),
                RUNNER_TESTS_COLUMN_WIDTH,
                if failed == 0 {
                    ResponseTone::Success.color()
                } else {
                    ResponseTone::Error.color()
                },
                false,
                true,
            ))
        })
}

fn mock_log_row(method: String, path: String, status: u16) -> impl IntoElement {
    let method_color = method_color(&method);

    div()
        .flex()
        .items_center()
        .h(px(RESULT_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(fixed_row_cell(
            method,
            RUNNER_METHOD_COLUMN_WIDTH,
            method_color,
            true,
            false,
        ))
        .child(fixed_row_cell(
            status.to_string(),
            RUNNER_STATUS_COLUMN_WIDTH,
            response_tone(status).color(),
            false,
            false,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(ui_text_body())
                .child(path),
        )
}

fn websocket_log_row(entry: WebSocketLogEntry) -> impl IntoElement {
    let (direction, direction_color) = match entry.direction {
        WebSocketDirection::Sent => ("sent", ui_accent_text()),
        WebSocketDirection::Received => ("recv", ResponseTone::Success.color()),
    };

    div()
        .flex()
        .items_center()
        .h(px(RESULT_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(fixed_row_cell(
            direction,
            WEBSOCKET_DIRECTION_COLUMN_WIDTH,
            direction_color,
            true,
            false,
        ))
        .child(fixed_row_cell(
            entry.kind,
            WEBSOCKET_KIND_COLUMN_WIDTH,
            ui_text_body(),
            false,
            false,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(ui_text_body())
                .child(entry.data),
        )
}

fn sse_log_row(entry: SseLogEntry) -> impl IntoElement {
    let id = entry.id.unwrap_or_else(|| "-".to_string());

    div()
        .flex()
        .items_center()
        .h(px(RESULT_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .gap_2()
        .border_b_1()
        .border_color(ui_hover())
        .font_family(PLATFORM_MONOSPACE_FONT)
        .text_size(px(PANEL_CONTENT_TEXT_SIZE))
        .child(fixed_row_cell(
            entry.event,
            SSE_EVENT_COLUMN_WIDTH,
            ui_accent_text(),
            true,
            false,
        ))
        .child(fixed_row_cell(
            id,
            SSE_ID_COLUMN_WIDTH,
            ui_text_body(),
            false,
            false,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(ui_text_body())
                .child(entry.data),
        )
}

fn history_row(
    id: u64,
    method: String,
    url: String,
    status: String,
    enabled: bool,
    delete_enabled: bool,
    cx: &mut Context<ZenApiApp>,
) -> impl IntoElement + 'static + use<> {
    let method_color = method_color(&method);

    div()
        .id(("history", id))
        .flex()
        .items_center()
        .h(px(HISTORY_ROW_HEIGHT))
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_TIGHT))
        .px_2()
        .py_1()
        .opacity(control_opacity(enabled))
        .hover(move |row| if enabled { row.bg(ui_hover()) } else { row })
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .when(enabled, |row| row.cursor_pointer())
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                        if can_restore_request_from_sidebar(app.busy) {
                            app.restore_history_entry(id, cx);
                        }
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap_2()
                        .child(
                            fixed_row_cell(
                                method,
                                HTTP_METHOD_LABEL_WIDTH,
                                method_color,
                                true,
                                false,
                            )
                            .text_size(px(SIDEBAR_METHOD_TEXT_SIZE)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .font_family(PLATFORM_MONOSPACE_FONT)
                                .text_size(px(SIDEBAR_PRIMARY_ROW_TEXT_SIZE))
                                .text_color(ui_text_primary())
                                .child(url),
                        ),
                )
                .child(
                    div()
                        .ml(px(SIDEBAR_SECONDARY_ROW_INDENT))
                        .min_w_0()
                        .truncate()
                        .font_family(PLATFORM_MONOSPACE_FONT)
                        .text_size(px(ROW_META_TEXT_SIZE))
                        .text_color(ui_sidebar_detail_text())
                        .child(status),
                ),
        )
        .child(
            sidebar_small_button(
                "Del",
                42.,
                SECTION_HEADER_HEIGHT,
                delete_enabled,
                ButtonTone::Neutral,
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                    if can_delete_history_entry(app.busy) {
                        app.history.remove(id);
                        cx.notify();
                    }
                }),
            ),
        )
}

fn append_collection_rows(
    rows: &mut Vec<gpui::AnyElement>,
    items: &[CollectionItem],
    parent_id: &str,
    depth: usize,
    expanded_nodes: &[String],
    restore_enabled: bool,
    mutate_enabled: bool,
    cx: &mut Context<ZenApiApp>,
) {
    for (index, item) in items.iter().enumerate() {
        let id = format!("{parent_id}/{index}");
        match item {
            CollectionItem::Folder(folder) => {
                let expanded = expanded_nodes.iter().any(|node| node == &id);
                rows.push(collection_folder_row(
                    &id,
                    folder,
                    depth,
                    collection_item_count(&folder.items),
                    expanded,
                    mutate_enabled,
                    cx,
                ));
                if expanded {
                    append_collection_rows(
                        rows,
                        &folder.items,
                        &id,
                        depth + 1,
                        expanded_nodes,
                        restore_enabled,
                        mutate_enabled,
                        cx,
                    );
                }
            }
            CollectionItem::Request(request) => {
                rows.push(collection_request_row(
                    &id,
                    request.clone(),
                    depth,
                    restore_enabled,
                    mutate_enabled,
                    cx,
                ));
            }
        }
    }
}

fn collection_tree_indent(depth: usize) -> f32 {
    (COLLECTION_TREE_INDENT_BASE + depth as f32 * COLLECTION_TREE_INDENT_STEP)
        .min(COLLECTION_TREE_INDENT_MAX)
}

fn collection_tree_row_height(kind: CollectionNodeKind) -> f32 {
    match kind {
        CollectionNodeKind::Root => COLLECTION_TREE_ROOT_ROW_HEIGHT,
        CollectionNodeKind::Folder => COLLECTION_TREE_FOLDER_ROW_HEIGHT,
        CollectionNodeKind::Request => COLLECTION_TREE_REQUEST_ROW_HEIGHT,
    }
}

fn collection_drag_over_background() -> Hsla {
    ui_hover()
}

fn collection_root_row(
    name: String,
    item_count: usize,
    expanded: bool,
    mutate_enabled: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::AnyElement {
    let marker = if expanded { "v" } else { ">" };
    let menu_label = name.clone();

    div()
        .flex()
        .items_center()
        .h(px(collection_tree_row_height(CollectionNodeKind::Root)))
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_TIGHT))
        .px_2()
        .gap_2()
        .text_size(px(SIDEBAR_PRIMARY_ROW_TEXT_SIZE))
        .cursor_pointer()
        .hover(|row| row.bg(ui_hover()))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _event: &MouseUpEvent, _window, cx| {
                app.toggle_collection_node("collection".to_string(), cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                app.open_collection_menu(
                    CollectionContextMenu {
                        node_id: "collection".to_string(),
                        label: menu_label.clone(),
                        kind: CollectionNodeKind::Root,
                    },
                    cx,
                );
            }),
        )
        .drag_over::<DraggedCollectionNode>(move |row, _dragged, _window, _cx| {
            if mutate_enabled {
                row.bg(collection_drag_over_background())
            } else {
                row
            }
        })
        .on_drop(
            cx.listener(|app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), "collection".to_string(), cx);
            }),
        )
        .child(
            div()
                .w(px(COLLECTION_TREE_MARKER_WIDTH))
                .flex_shrink_0()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_secondary())
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_primary())
                .child(name),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_muted())
                .child(item_count.to_string()),
        )
        .into_any()
}

fn collection_folder_row(
    id: &str,
    folder: &CollectionFolder,
    depth: usize,
    item_count: usize,
    expanded: bool,
    mutate_enabled: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::AnyElement {
    let id = id.to_string();
    let marker = if expanded { "v" } else { ">" };
    let element_id = format!("collection-folder:{id}");
    let toggle_id = id.clone();
    let menu_id = id.clone();
    let menu_label = folder.name.clone();
    let drop_id = id.clone();
    let drag_value = DraggedCollectionNode {
        node_id: id,
        label: folder.name.clone(),
    };

    div()
        .id(element_id)
        .flex()
        .items_center()
        .h(px(collection_tree_row_height(CollectionNodeKind::Folder)))
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_TIGHT))
        .pl(px(collection_tree_indent(depth)))
        .pr_2()
        .gap_2()
        .text_size(px(SIDEBAR_PRIMARY_ROW_TEXT_SIZE))
        .cursor_pointer()
        .hover(|row| row.bg(ui_hover()))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                app.toggle_collection_node(toggle_id.clone(), cx);
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                app.open_collection_menu(
                    CollectionContextMenu {
                        node_id: menu_id.clone(),
                        label: menu_label.clone(),
                        kind: CollectionNodeKind::Folder,
                    },
                    cx,
                );
            }),
        )
        .when(mutate_enabled, |row| {
            row.on_drag(drag_value, |dragged, _offset, _window, cx| {
                cx.new(|_| CollectionDragPreview {
                    label: dragged.label.clone(),
                })
            })
        })
        .drag_over::<DraggedCollectionNode>(move |row, _dragged, _window, _cx| {
            if mutate_enabled {
                row.bg(collection_drag_over_background())
            } else {
                row
            }
        })
        .on_drop(
            cx.listener(move |app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), drop_id.clone(), cx);
            }),
        )
        .child(
            div()
                .w(px(COLLECTION_TREE_MARKER_WIDTH))
                .flex_shrink_0()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_secondary())
                .child(marker),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .text_color(ui_text_body())
                .child(folder.name.clone()),
        )
        .child(
            div()
                .flex_shrink_0()
                .font_family(PLATFORM_MONOSPACE_FONT)
                .text_color(ui_text_muted())
                .child(item_count.to_string()),
        )
        .into_any()
}

fn collection_request_row(
    id: &str,
    request: CollectionRequest,
    depth: usize,
    enabled: bool,
    mutate_enabled: bool,
    cx: &mut Context<ZenApiApp>,
) -> gpui::AnyElement {
    let menu_id = id.to_string();
    let element_id = format!("collection-request:{id}");
    let method = request.method.clone();
    let name = request.name.clone();
    let url = request.url.clone();
    let method_color = method_color(&method);
    let menu_label = request.name.clone();
    let drop_id = menu_id.clone();
    let drag_value = DraggedCollectionNode {
        node_id: menu_id.clone(),
        label: request.name.clone(),
    };
    let restore_request = request.clone();

    div()
        .id(element_id)
        .flex()
        .items_center()
        .h(px(collection_tree_row_height(CollectionNodeKind::Request)))
        .min_w_0()
        .overflow_hidden()
        .rounded(px(UI_RADIUS_TIGHT))
        .pl(px(collection_tree_indent(depth)))
        .pr_2()
        .gap_2()
        .opacity(control_opacity(enabled))
        .when(enabled, |row| row.cursor_pointer())
        .hover(move |row| if enabled { row.bg(ui_hover()) } else { row })
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(move |app, _event: &MouseUpEvent, _window, cx| {
                if can_restore_request_from_sidebar(app.busy) {
                    app.restore_collection_request(restore_request.clone(), cx);
                }
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |app, _event: &MouseDownEvent, window, cx| {
                window.prevent_default();
                app.open_collection_menu(
                    CollectionContextMenu {
                        node_id: menu_id.clone(),
                        label: menu_label.clone(),
                        kind: CollectionNodeKind::Request,
                    },
                    cx,
                );
            }),
        )
        .when(mutate_enabled, |row| {
            row.on_drag(drag_value, |dragged, _offset, _window, cx| {
                cx.new(|_| CollectionDragPreview {
                    label: dragged.label.clone(),
                })
            })
        })
        .drag_over::<DraggedCollectionNode>(move |row, _dragged, _window, _cx| {
            if mutate_enabled {
                row.bg(collection_drag_over_background())
            } else {
                row
            }
        })
        .on_drop(
            cx.listener(move |app, dragged: &DraggedCollectionNode, _window, cx| {
                app.move_collection_target(dragged.node_id.clone(), drop_id.clone(), cx);
            }),
        )
        .child(
            fixed_row_cell(method, HTTP_METHOD_LABEL_WIDTH, method_color, true, false)
                .text_size(px(SIDEBAR_COMPACT_METHOD_TEXT_SIZE)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(SIDEBAR_PRIMARY_ROW_TEXT_SIZE))
                        .text_color(ui_text_primary())
                        .child(name),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(PLATFORM_MONOSPACE_FONT)
                        .text_size(px(ROW_META_TEXT_SIZE))
                        .text_color(ui_sidebar_detail_text())
                        .child(url),
                ),
        )
        .into_any()
}

fn collection_item_count(items: &[CollectionItem]) -> usize {
    items
        .iter()
        .map(|item| match item {
            CollectionItem::Folder(folder) => collection_item_count(&folder.items),
            CollectionItem::Request(_) => 1,
        })
        .sum()
}

fn can_export_collection(collection: &ApiCollection) -> bool {
    collection_item_count(&collection.items) > 0
}

fn method_color(method: &str) -> Hsla {
    match method {
        "GET" => ui_status_success(),
        "POST" => ui_status_busy(),
        "PUT" => ui_accent(),
        "PATCH" => rgb(UI_COLOR_METHOD_PATCH).into(),
        "DELETE" => ui_status_error(),
        "OPTIONS" => rgb(UI_COLOR_METHOD_OPTIONS).into(),
        "HEAD" => rgb(UI_COLOR_METHOD_HEAD).into(),
        _ => ui_text_secondary(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestAddressBorderTone {
    Disabled,
    Focused,
    Default,
}

fn request_address_border_tone(
    address_enabled: bool,
    url_focused: bool,
    _method_menu_open: bool,
) -> RequestAddressBorderTone {
    if !address_enabled {
        RequestAddressBorderTone::Disabled
    } else if url_focused {
        RequestAddressBorderTone::Focused
    } else {
        RequestAddressBorderTone::Default
    }
}

fn request_address_border_color(tone: RequestAddressBorderTone) -> Hsla {
    match tone {
        RequestAddressBorderTone::Disabled => ui_disabled_border(),
        RequestAddressBorderTone::Focused => ui_accent(),
        RequestAddressBorderTone::Default => ui_border_strong(),
    }
}

fn workspace_split_ratios_changed(
    current_sidebar: f32,
    current_request: f32,
    next_sidebar: f32,
    next_request: f32,
) -> bool {
    (current_sidebar - next_sidebar).abs() >= WORKSPACE_SPLIT_RATIO_EPSILON
        || (current_request - next_request).abs() >= WORKSPACE_SPLIT_RATIO_EPSILON
}

fn workspace_split_target_ratios(
    split: WorkspaceSplitDrag,
    pointer_ratio: f32,
    current_sidebar: f32,
    current_request: f32,
) -> (f32, f32) {
    match split {
        WorkspaceSplitDrag::SidebarRequest => {
            let max_sidebar = WORKSPACE_SIDEBAR_MAX_RATIO
                .min(1.0 - current_request - WORKSPACE_RESPONSE_MIN_RATIO)
                .max(WORKSPACE_SIDEBAR_MIN_RATIO);
            (
                pointer_ratio.clamp(WORKSPACE_SIDEBAR_MIN_RATIO, max_sidebar),
                current_request,
            )
        }
        WorkspaceSplitDrag::RequestResponse => {
            let request_ratio = pointer_ratio - current_sidebar;
            let max_request = WORKSPACE_REQUEST_MAX_RATIO
                .min(1.0 - current_sidebar - WORKSPACE_RESPONSE_MIN_RATIO)
                .max(WORKSPACE_REQUEST_MIN_RATIO);
            (
                current_sidebar,
                request_ratio.clamp(WORKSPACE_REQUEST_MIN_RATIO, max_request),
            )
        }
    }
}

fn workspace_split_preview(
    split: WorkspaceSplitDrag,
    pointer_ratio: f32,
    current_sidebar: f32,
    current_request: f32,
) -> Option<WorkspaceSplitPreview> {
    let (next_sidebar, next_request) =
        workspace_split_target_ratios(split, pointer_ratio, current_sidebar, current_request);
    workspace_split_ratios_changed(current_sidebar, current_request, next_sidebar, next_request)
        .then_some(WorkspaceSplitPreview {
            split,
            sidebar_ratio: next_sidebar,
            request_ratio: next_request,
        })
}

fn workspace_split_pending_changed(
    current: Option<WorkspaceSplitPreview>,
    next: Option<WorkspaceSplitPreview>,
) -> bool {
    current != next
}

fn quantize_workspace_split_ratio(pointer_ratio: f32, workspace_width_px: f32) -> f32 {
    if workspace_width_px <= 0.0 {
        return pointer_ratio.clamp(0.0, 1.0);
    }

    let step_ratio = WORKSPACE_SPLIT_UPDATE_STEP_PX / workspace_width_px;
    if step_ratio <= 0.0 {
        pointer_ratio.clamp(0.0, 1.0)
    } else {
        ((pointer_ratio / step_ratio).round() * step_ratio).clamp(0.0, 1.0)
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

fn import_success_body(spec_label: &str) -> String {
    let spec_label = spec_label.trim();
    if spec_label.is_empty() {
        "Spec loaded".to_string()
    } else {
        spec_label.to_string()
    }
}

fn mock_button_label(server_running: bool) -> &'static str {
    if server_running {
        MOCK_STOP_LABEL
    } else {
        MOCK_START_LABEL
    }
}

fn sidebar_focus_target(section: SidebarSection) -> SidebarFocusTarget {
    match section {
        SidebarSection::Endpoints => SidebarFocusTarget::RouteFilter,
        SidebarSection::Collections => SidebarFocusTarget::CollectionPath,
        SidebarSection::History => SidebarFocusTarget::HistoryFilter,
    }
}

fn close_transient_ui_state(state: &mut TransientUiState) -> bool {
    let changed = state.import_popover_open
        || state.method_menu_open
        || state.codegen_menu_open
        || state.collection_menu_open;

    if changed {
        *state = TransientUiState::default();
    }

    changed
}

fn reset_scroll_handle(scroll: &ScrollHandle) {
    scroll.set_offset(point(px(LAYOUT_ZERO), px(LAYOUT_ZERO)));
}

fn sending_response_body(method: &str, url: &str) -> String {
    format!(
        "{PENDING_RESPONSE_BODY_LABEL}\n\n{} {}",
        method,
        preview_text(url)
    )
}

fn request_worker_stopped_message() -> &'static str {
    "Request worker stopped."
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

fn filtered_count_label(total: usize, visible: usize) -> String {
    if total == 0 {
        "0".to_string()
    } else if visible >= total {
        total.to_string()
    } else {
        format!("{visible}/{total}")
    }
}

fn route_status_label(total: usize, visible: usize) -> String {
    filtered_count_label(total, visible)
}

fn route_count_label(count: usize) -> String {
    if count == 1 {
        "1 route".to_string()
    } else {
        format!("{count} routes")
    }
}

fn status_bar_busy_label(busy: bool) -> Option<&'static str> {
    busy.then_some("Busy")
}

fn status_bar_mock_label(server_running: bool, server_status: &str) -> Option<String> {
    let status = server_status.trim();
    if server_running {
        if status.is_empty() {
            Some("Mock".to_string())
        } else if let Ok(addr) = status.parse::<SocketAddr>() {
            Some(format!("Mock :{}", addr.port()))
        } else {
            Some(format!("Mock {status}"))
        }
    } else if is_idle_mock_status(status) {
        None
    } else {
        Some(status.to_string())
    }
}

fn is_idle_mock_status(status: &str) -> bool {
    matches!(
        status.trim(),
        "" | MOCK_STATUS_STOPPED | MOCK_STATUS_READY | MOCK_STATUS_NO_ROUTES
    )
}

fn status_bar_trailing_visible(response_status: Option<&str>, busy_status: Option<&str>) -> bool {
    response_status.is_some() || busy_status.is_some()
}

fn response_status_label(status: &str) -> Option<String> {
    let status = status.trim();
    if status.is_empty() || status == "Idle" {
        None
    } else {
        Some(status.to_string())
    }
}

fn response_header_meta(status: &str, meta: &str) -> Option<String> {
    let status = response_status_label(status);
    let meta = meta.trim();

    match (status, meta.is_empty()) {
        (None, true) => None,
        (None, false) => Some(meta.to_string()),
        (Some(status), true) => Some(status),
        (Some(status), false) => Some(format!("{status} | {meta}")),
    }
}

fn panel_header_status_label(status: &str) -> Option<String> {
    let status = status.trim();
    if status.is_empty() || status == "idle" {
        None
    } else if let Some(error) = status.strip_prefix("error:") {
        Some(format!("Err {}", error.trim()))
    } else {
        Some(status.to_string())
    }
}

fn realtime_header_status_label(status: &str) -> Option<String> {
    let status = status.trim();
    if status.is_empty() || status == "idle" {
        return None;
    }

    let label = match status {
        REALTIME_STATUS_NO_URL | REALTIME_STATUS_BAD_URL => status.to_string(),
        "connecting" => "Conn".to_string(),
        "connected" | "subscribed" => "Open".to_string(),
        "subscribing" => "Sub".to_string(),
        "closing" => "End".to_string(),
        "closed" => "Closed".to_string(),
        "stopped" => "Stopped".to_string(),
        "not connected" => "No conn".to_string(),
        "invalid binary" => "Bad hex".to_string(),
        "sending" | "sent" => "TX".to_string(),
        other if other.starts_with("received ") => {
            format!("RX {}", other.trim_start_matches("received ").trim())
        }
        other if other.starts_with("event ") => {
            format!("Evt {}", other.trim_start_matches("event ").trim())
        }
        "1 event" => "1 ev".to_string(),
        other if other.ends_with(" events") => {
            format!("{} ev", other.trim_end_matches(" events").trim())
        }
        other if other.starts_with("closed:") => "Closed".to_string(),
        other if other.starts_with("error:") => {
            let error = other.trim_start_matches("error:").trim();
            if error.is_empty() {
                "Err".to_string()
            } else {
                format!("Err {error}")
            }
        }
        other => other.to_string(),
    };

    Some(label)
}

fn runner_header_status_label(status: &str) -> Option<String> {
    let status = status.trim();
    if status.is_empty()
        || matches!(
            status,
            "Runner idle" | RUNNER_EMPTY_REQUESTS_LABEL | RUNNER_EMPTY_RESULTS_LABEL
        )
    {
        None
    } else {
        Some(status.to_string())
    }
}

fn tests_header_status_label(
    results: &[ResponseAssertionResult],
    configured_count: usize,
) -> Option<String> {
    assertion_meta(results).or_else(|| {
        if configured_count == 0 {
            None
        } else {
            Some(format!("{configured_count} cfg"))
        }
    })
}

fn collection_sidebar_status_label(status: &str) -> Option<String> {
    let status = status.trim();
    if status.is_empty() || status == "No collection file" {
        return None;
    }

    let label = if status.starts_with("Imported ") {
        "Imported"
    } else if status.starts_with("Exported ") {
        "Exported"
    } else {
        match status.split_once(':').map_or(status, |(prefix, _)| prefix) {
            "Request created" => "+ Req",
            "Folder created" => "+ Dir",
            "Item copied" => "Copied",
            "Item deleted" => "Deleted",
            "Item moved" => "Moved",
            "Item renamed" => "Renamed",
            "Import failed" => "Import fail",
            "Export failed" => "Export fail",
            "Create failed" => "Create fail",
            "Copy failed" => "Copy fail",
            "Delete failed" => "Delete fail",
            "Move failed" => "Move fail",
            "Rename failed" => "Rename fail",
            "Nothing to export" => "No export",
            "Name needed" => "Name needed",
            "Rename unavailable while busy" => "Busy",
            "Rename unavailable" => "Unavailable",
            other => other,
        }
    };

    Some(label.to_string())
}

fn has_trimmed_text(input: &str) -> bool {
    !input.trim().is_empty()
}

fn can_submit_path_action(busy: bool, path: &str) -> bool {
    !busy && has_trimmed_text(path)
}

fn can_rename_collection_target(kind: CollectionNodeKind, name: &str) -> bool {
    kind != CollectionNodeKind::Root && has_trimmed_text(name)
}

fn can_submit_collection_rename(busy: bool, kind: Option<CollectionNodeKind>, name: &str) -> bool {
    !busy && kind.is_some_and(|kind| can_rename_collection_target(kind, name))
}

fn can_save_current_request_to_collection(url: &str, pre_request_script: &str) -> bool {
    can_send_request(url, pre_request_script)
}

fn can_save_current_request_shortcut(busy: bool, url: &str, pre_request_script: &str) -> bool {
    !busy && can_save_current_request_to_collection(url, pre_request_script)
}

fn can_focus_request_url(busy: bool) -> bool {
    !busy
}

fn can_toggle_import_popover(busy: bool) -> bool {
    !busy
}

fn can_focus_sidebar_input(busy: bool, target: SidebarFocusTarget) -> bool {
    match target {
        SidebarFocusTarget::RouteFilter | SidebarFocusTarget::HistoryFilter => true,
        SidebarFocusTarget::CollectionPath => !busy,
    }
}

fn can_send_request_shortcut(busy: bool, url: &str, pre_request_script: &str) -> bool {
    !busy && can_send_request(url, pre_request_script)
}

fn can_send_request(url: &str, pre_request_script: &str) -> bool {
    has_trimmed_text(url) || pre_request_sets_url(pre_request_script)
}

fn pre_request_sets_url(script: &str) -> bool {
    script.split([';', '\n']).map(str::trim).any(|action| {
        if action.is_empty() || action.starts_with('#') || action.starts_with("//") {
            return false;
        }

        let Some((name, value)) = action.split_once(char::is_whitespace) else {
            return false;
        };

        matches!(name.trim(), "url" | "set_url") && has_trimmed_text(value)
    })
}

fn is_websocket_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("ws://") || url.starts_with("wss://")
}

fn websocket_url_validation_response(url: &str) -> (&'static str, &'static str) {
    if has_trimmed_text(url) {
        (WEBSOCKET_URL_INVALID_TITLE, WEBSOCKET_URL_INVALID_BODY)
    } else {
        (WEBSOCKET_URL_REQUIRED_TITLE, URL_REQUIRED_BODY)
    }
}

fn can_connect_websocket(busy: bool, running: bool, url: &str) -> bool {
    !busy && !running && is_websocket_url(url)
}

fn can_close_websocket(busy: bool, running: bool) -> bool {
    !busy && running
}

fn is_sse_url(url: &str) -> bool {
    let url = url.trim();
    url.starts_with("http://") || url.starts_with("https://")
}

fn sse_url_validation_response(url: &str) -> (&'static str, &'static str) {
    if has_trimmed_text(url) {
        (SSE_URL_INVALID_TITLE, SSE_URL_INVALID_BODY)
    } else {
        (SSE_URL_REQUIRED_TITLE, URL_REQUIRED_BODY)
    }
}

fn can_start_sse(busy: bool, running: bool, url: &str) -> bool {
    !busy && !running && is_sse_url(url)
}

fn can_stop_sse_subscription(busy: bool, has_subscription: bool) -> bool {
    !busy && has_subscription
}

fn can_use_realtime_log_actions(busy: bool, entry_count: usize) -> bool {
    !busy && entry_count > 0
}

fn can_copy_headers_bulk(busy: bool, has_headers: bool) -> bool {
    !busy && has_headers
}

fn can_use_codegen_language_selector(busy: bool) -> bool {
    !busy
}

fn can_select_request_method(busy: bool) -> bool {
    can_edit_request_configuration(busy)
}

fn can_edit_request_configuration(busy: bool) -> bool {
    !busy
}

fn can_add_key_value_row(
    busy: bool,
    target: KeyValueEditorTarget,
    has_active_environment: bool,
) -> bool {
    can_edit_request_configuration(busy)
        && (target != KeyValueEditorTarget::ActiveEnvironmentVariables || has_active_environment)
}

fn can_remove_key_value_row(
    busy: bool,
    target: KeyValueEditorTarget,
    has_active_environment: bool,
    row_count: usize,
    index: usize,
) -> bool {
    can_add_key_value_row(busy, target, has_active_environment) && index < row_count
}

fn can_restore_request_from_sidebar(busy: bool) -> bool {
    !busy
}

fn can_clear_history(busy: bool, entry_count: usize) -> bool {
    !busy && entry_count > 0
}

fn can_delete_history_entry(busy: bool) -> bool {
    !busy
}

fn can_mutate_collection(busy: bool) -> bool {
    !busy
}

fn can_open_collection_context_menu(busy: bool) -> bool {
    can_mutate_collection(busy)
}

fn can_use_collection_context_action(busy: bool, target_allows_action: bool) -> bool {
    can_mutate_collection(busy) && target_allows_action
}

fn can_submit_environment_add(busy: bool, name: &str) -> bool {
    can_edit_request_configuration(busy) && can_add_environment(name)
}

fn can_delete_environment(busy: bool, has_active_environment: bool) -> bool {
    can_edit_request_configuration(busy) && has_active_environment
}

fn can_format_request_raw_json(busy: bool, format: RawBodyFormat, body: &str) -> bool {
    can_edit_request_configuration(busy) && can_format_raw_json(format, body)
}

fn can_edit_response_assertion_row(busy: bool, row_count: usize, index: usize) -> bool {
    can_edit_request_configuration(busy) && index < row_count
}

fn can_remove_response_assertion_row(busy: bool, row_count: usize, index: usize) -> bool {
    can_edit_response_assertion_row(busy, row_count, index)
}

fn can_clear_response_assertion_results(busy: bool, result_count: usize) -> bool {
    !busy && result_count > 0
}

fn can_run_collection_runner(total: usize, runner_running: bool, busy: bool) -> bool {
    total > 0 && !runner_running && !busy
}

fn can_send_websocket_message(
    busy: bool,
    running: bool,
    mode: WebSocketMessageMode,
    message: &str,
) -> bool {
    if busy || !running {
        return false;
    }

    match mode {
        WebSocketMessageMode::Text => true,
        WebSocketMessageMode::BinaryHex => websocket_hex_bytes(message).is_ok(),
    }
}

fn can_add_environment(name: &str) -> bool {
    has_trimmed_text(&normalized_environment_name(name))
}

fn runner_stop_toggle_enabled(runner_running: bool, busy: bool) -> bool {
    !runner_running && !busy
}

fn can_copy_codegen_request(request: &CodegenRequest) -> bool {
    has_trimmed_text(&request.url)
}

fn can_copy_codegen_snippet(busy: bool, has_snippet: bool) -> bool {
    !busy && has_snippet
}

fn set_text_input_enabled(input: &Entity<TextInput>, enabled: bool, cx: &mut Context<ZenApiApp>) {
    input.update(cx, |input, cx| input.set_enabled(enabled, cx));
}

fn set_key_value_rows_enabled(rows: &[KeyValueRow], enabled: bool, cx: &mut Context<ZenApiApp>) {
    for row in rows {
        set_text_input_enabled(&row.key, enabled, cx);
        set_text_input_enabled(&row.value, enabled, cx);
    }
}

fn set_assertion_rows_enabled(
    rows: &[TestAssertionRow],
    enabled: bool,
    cx: &mut Context<ZenApiApp>,
) {
    for row in rows {
        set_text_input_enabled(&row.name, enabled, cx);
        set_text_input_enabled(&row.target, enabled, cx);
        set_text_input_enabled(&row.expected, enabled, cx);
    }
}

fn text_input_has_text(input: &Entity<TextInput>, cx: &mut Context<ZenApiApp>) -> bool {
    has_trimmed_text(&input.read(cx).text())
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

fn formatted_json_body(input: &str) -> Result<String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(anyhow!(RAW_JSON_EMPTY_FORMAT_BODY));
    }

    let value = serde_json::from_str::<serde_json::Value>(input)
        .map_err(|error| anyhow!("Invalid JSON: {error}"))?;
    Ok(pretty_json(&value))
}

fn can_format_raw_json(format: RawBodyFormat, body: &str) -> bool {
    format == RawBodyFormat::Json && has_trimmed_text(body)
}

fn collapsed_json_preview(input: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(input).ok()?;
    Some(collapsed_json_value(&value, 0))
}

fn response_can_collapse(raw_body: &str) -> bool {
    collapsed_json_preview(raw_body).is_some()
}

fn can_toggle_response_collapse(busy: bool, raw_body: &str) -> bool {
    !busy && response_can_collapse(raw_body)
}

fn response_body_for_view(
    view: ResponseView,
    pretty_collapsed: bool,
    pretty_body: &str,
    raw_body: &str,
    headers: &str,
) -> String {
    match view {
        ResponseView::Pretty => {
            if pretty_collapsed {
                collapsed_json_preview(raw_body).unwrap_or_else(|| pretty_body.to_string())
            } else {
                pretty_body.to_string()
            }
        }
        ResponseView::Raw => raw_body.to_string(),
        ResponseView::Headers => {
            if headers.trim().is_empty() {
                RESPONSE_HEADERS_EMPTY_LABEL.to_string()
            } else {
                headers.to_string()
            }
        }
    }
}

fn can_copy_response_view(
    busy: bool,
    view: ResponseView,
    pretty_collapsed: bool,
    pretty_body: &str,
    raw_body: &str,
    headers: &str,
) -> bool {
    !busy && response_copy_text(view, pretty_collapsed, pretty_body, raw_body, headers).is_some()
}

fn response_copy_text(
    view: ResponseView,
    pretty_collapsed: bool,
    pretty_body: &str,
    raw_body: &str,
    headers: &str,
) -> Option<String> {
    if view == ResponseView::Headers && headers.trim().is_empty() {
        return None;
    }

    let text = response_body_for_view(view, pretty_collapsed, pretty_body, raw_body, headers);
    (!text.trim().is_empty()).then_some(text)
}

fn collapsed_json_value(value: &serde_json::Value, depth: usize) -> String {
    const MAX_CHILDREN: usize = 24;

    match value {
        serde_json::Value::Object(object) => {
            if object.is_empty() {
                return "{}".to_string();
            }

            let indent = "  ".repeat(depth);
            let child_indent = "  ".repeat(depth + 1);
            let mut lines = vec![format!("{{ // {} keys", object.len())];
            for (index, (key, value)) in object.iter().enumerate() {
                if index >= MAX_CHILDREN {
                    lines.push(format!(
                        "{child_indent}... // {} more",
                        object.len() - MAX_CHILDREN
                    ));
                    break;
                }
                lines.push(format!(
                    "{child_indent}\"{key}\": {}",
                    collapsed_json_summary(value)
                ));
            }
            lines.push(format!("{indent}}}"));
            lines.join("\n")
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return "[]".to_string();
            }

            let indent = "  ".repeat(depth);
            let child_indent = "  ".repeat(depth + 1);
            let mut lines = vec![format!("[ // {} items", items.len())];
            for (index, value) in items.iter().enumerate().take(MAX_CHILDREN) {
                lines.push(format!(
                    "{child_indent}[{index}] {}",
                    collapsed_json_summary(value)
                ));
            }
            if items.len() > MAX_CHILDREN {
                lines.push(format!(
                    "{child_indent}... // {} more",
                    items.len() - MAX_CHILDREN
                ));
            }
            lines.push(format!("{indent}]"));
            lines.join("\n")
        }
        _ => collapsed_json_summary(value),
    }
}

fn collapsed_json_summary(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(object) => format!("{{...}} // {} keys", object.len()),
        serde_json::Value::Array(items) => format!("[...] // {} items", items.len()),
        serde_json::Value::String(value) => format!("{value:?}"),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => "null".to_string(),
    }
}

fn format_headers(headers: &[(String, String)]) -> String {
    headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect::<Vec<_>>()
        .join("\n")
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
    fn filtered_count_labels_show_visible_and_total() {
        assert_eq!(filtered_count_label(0, 0), "0");
        assert_eq!(filtered_count_label(4, 4), "4");
        assert_eq!(filtered_count_label(4, 2), "2/4");
        assert_eq!(filtered_count_label(4, 8), "4");
        assert_eq!(route_count_label(0), "0 routes");
        assert_eq!(route_count_label(1), "1 route");
        assert_eq!(route_count_label(4), "4 routes");
        assert_eq!(route_status_label(0, 0), "0");
        assert_eq!(route_status_label(1, 1), "1");
        assert_eq!(route_status_label(4, 4), "4");
        assert_eq!(route_status_label(4, 2), "2/4");
        assert_eq!(route_status_label(4, 8), "4");
        assert!(!route_status_label(4, 2).contains("routes"));
        assert_eq!(import_success_body(" petstore.yaml "), "petstore.yaml");
        assert_eq!(import_success_body(""), "Spec loaded");
        assert!(!import_success_body("petstore.yaml").contains("Ready"));
        assert!(!import_success_body("petstore.yaml").contains("routes parsed"));
    }

    #[test]
    fn status_bar_labels_skip_idle_noise() {
        assert_eq!(status_bar_busy_label(false), None);
        assert_eq!(status_bar_busy_label(true), Some("Busy"));
        assert_eq!(MOCK_STATUS_IMPORT_ROUTES_FIRST, "No routes");
        assert!(
            !MOCK_STATUS_IMPORT_ROUTES_FIRST.contains("Import")
                && !MOCK_STATUS_IMPORT_ROUTES_FIRST.contains("first")
        );
        assert_eq!(status_bar_mock_label(false, ""), None);
        assert_eq!(status_bar_mock_label(false, MOCK_STATUS_STOPPED), None);
        assert_eq!(status_bar_mock_label(false, MOCK_STATUS_READY), None);
        assert_eq!(status_bar_mock_label(false, MOCK_STATUS_NO_ROUTES), None);
        assert!(is_idle_mock_status(MOCK_STATUS_READY));
        assert!(is_idle_mock_status(MOCK_STATUS_NO_ROUTES));
        assert_eq!(
            status_bar_mock_label(false, " No routes ").as_deref(),
            Some(MOCK_STATUS_IMPORT_ROUTES_FIRST)
        );
        assert_eq!(
            status_bar_mock_label(false, MOCK_STATUS_STARTING).as_deref(),
            Some(MOCK_STATUS_STARTING)
        );
        assert_eq!(
            status_bar_mock_label(false, MOCK_STATUS_STOPPING).as_deref(),
            Some(MOCK_STATUS_STOPPING)
        );
        assert_eq!(
            status_bar_mock_label(false, MOCK_STATUS_FAILED).as_deref(),
            Some(MOCK_STATUS_FAILED)
        );
        assert!(
            [
                MOCK_STATUS_STARTING,
                MOCK_STATUS_STOPPING,
                MOCK_STATUS_FAILED
            ]
            .iter()
            .all(|status| status.len() <= 10
                && !status.contains("Starting")
                && !status.contains("Stopping")
                && !status.contains("failed"))
        );
        assert_eq!(status_bar_mock_label(true, "").as_deref(), Some("Mock"));
        assert_eq!(
            status_bar_mock_label(true, "127.0.0.1:8080").as_deref(),
            Some("Mock :8080")
        );
        assert_eq!(
            status_bar_mock_label(true, "[::1]:8081").as_deref(),
            Some("Mock :8081")
        );
        assert!(
            !status_bar_mock_label(true, "127.0.0.1:8080")
                .unwrap()
                .contains("127.0.0.1")
        );
        assert_eq!(response_status_label(""), None);
        assert_eq!(response_status_label(" Idle "), None);
        assert_eq!(
            response_status_label("HTTP 200").as_deref(),
            Some("HTTP 200")
        );
        assert_eq!(
            response_status_label("Request failed").as_deref(),
            Some("Request failed")
        );
        assert!(!status_bar_trailing_visible(None, None));
        assert!(status_bar_trailing_visible(Some("HTTP 200"), None));
        assert!(status_bar_trailing_visible(None, Some("Busy")));
        assert!(status_bar_trailing_visible(
            Some("Request failed"),
            Some("Busy")
        ));
    }

    #[test]
    fn panel_header_status_labels_skip_default_noise() {
        assert_eq!(panel_header_meta_text(None), None);
        assert_eq!(panel_header_meta_text(Some("")), None);
        assert_eq!(panel_header_meta_text(Some("   ")), None);
        assert_eq!(
            panel_header_meta_text(Some(" Tests 2/3 ")).as_deref(),
            Some("Tests 2/3")
        );
        assert_eq!(panel_header_status_label(""), None);
        assert_eq!(panel_header_status_label(" idle "), None);
        assert_eq!(
            panel_header_status_label("connected").as_deref(),
            Some("connected")
        );
        assert_eq!(
            panel_header_status_label("error: failed").as_deref(),
            Some("Err failed")
        );
        assert_eq!(realtime_header_status_label(""), None);
        assert_eq!(realtime_header_status_label(" idle "), None);
        assert_eq!(
            realtime_header_status_label(REALTIME_STATUS_NO_URL).as_deref(),
            Some("No URL")
        );
        assert_eq!(
            realtime_header_status_label(REALTIME_STATUS_BAD_URL).as_deref(),
            Some("Bad URL")
        );
        assert_eq!(
            realtime_header_status_label("connecting").as_deref(),
            Some("Conn")
        );
        assert_eq!(
            realtime_header_status_label("connected").as_deref(),
            Some("Open")
        );
        assert_eq!(
            realtime_header_status_label("subscribing").as_deref(),
            Some("Sub")
        );
        assert_eq!(
            realtime_header_status_label("received text").as_deref(),
            Some("RX text")
        );
        assert_eq!(
            realtime_header_status_label("event update").as_deref(),
            Some("Evt update")
        );
        assert_eq!(
            realtime_header_status_label("1 event").as_deref(),
            Some("1 ev")
        );
        assert_eq!(
            realtime_header_status_label("2 events").as_deref(),
            Some("2 ev")
        );
        assert_eq!(
            realtime_header_status_label("closed: normal").as_deref(),
            Some("Closed")
        );
        assert_eq!(
            realtime_header_status_label("error: socket reset").as_deref(),
            Some("Err socket reset")
        );

        assert_eq!(runner_header_status_label("Runner idle"), None);
        assert_eq!(
            runner_header_status_label(RUNNER_EMPTY_REQUESTS_LABEL),
            None
        );
        assert_eq!(runner_header_status_label(RUNNER_EMPTY_RESULTS_LABEL), None);
        assert_eq!(
            runner_header_status_label("Run 3").as_deref(),
            Some("Run 3")
        );

        assert_eq!(tests_header_status_label(&[], 0), None);
        assert_eq!(tests_header_status_label(&[], 2).as_deref(), Some("2 cfg"));
        let results = vec![
            ResponseAssertionResult {
                name: "status".to_string(),
                passed: true,
                error: None,
            },
            ResponseAssertionResult {
                name: "body".to_string(),
                passed: false,
                error: Some("missing".to_string()),
            },
        ];
        assert_eq!(
            tests_header_status_label(&results, 0).as_deref(),
            Some("1/2 tests")
        );
    }

    #[test]
    fn response_header_meta_keeps_title_stable() {
        assert_eq!(response_header_meta("", ""), None);
        assert_eq!(response_header_meta("Idle", ""), None);
        assert_eq!(
            response_header_meta("Idle", "1/2 tests").as_deref(),
            Some("1/2 tests")
        );
        assert_eq!(
            response_header_meta("HTTP 200", "").as_deref(),
            Some("HTTP 200")
        );
        assert_eq!(
            response_header_meta("HTTP 200", "1/2 tests").as_deref(),
            Some("HTTP 200 | 1/2 tests")
        );
        assert_eq!(
            response_header_meta(" Request failed ", " ").as_deref(),
            Some("Request failed")
        );
    }

    #[test]
    fn history_mutation_actions_require_history_and_idle_state() {
        assert!(!can_clear_history(false, 0));
        assert!(can_clear_history(false, 1));
        assert!(!can_clear_history(true, 1));
        assert!(can_delete_history_entry(false));
        assert!(!can_delete_history_entry(true));
    }

    #[test]
    fn trimmed_text_presence_ignores_whitespace() {
        assert!(!has_trimmed_text(""));
        assert!(!has_trimmed_text("   \n\t"));
        assert!(has_trimmed_text("api.yaml"));
        assert!(has_trimmed_text("  collection.json  "));
    }

    #[test]
    fn path_submit_requires_text_and_idle_state() {
        assert_eq!(PATH_REQUIRED_BODY, "Path is empty.");
        assert_eq!(IMPORT_PATH_REQUIRED_TITLE, "No path");
        assert_eq!(COLLECTION_PATH_REQUIRED_TITLE, "No path");
        assert_eq!(EXPORT_PATH_REQUIRED_TITLE, "No path");
        assert!(
            [
                PATH_REQUIRED_BODY,
                IMPORT_PATH_REQUIRED_TITLE,
                COLLECTION_PATH_REQUIRED_TITLE,
                EXPORT_PATH_REQUIRED_TITLE
            ]
            .iter()
            .all(|label| !label.contains("Enter ")
                && !label.contains(" before ")
                && !label.contains("needs"))
        );
        assert!(!can_submit_path_action(false, ""));
        assert!(!can_submit_path_action(false, "   "));
        assert!(!can_submit_path_action(true, "api.yaml"));
        assert!(can_submit_path_action(false, " api.yaml "));
        assert!(can_toggle_import_popover(false));
        assert!(!can_toggle_import_popover(true));
    }

    #[test]
    fn collection_rename_requires_non_root_and_name() {
        assert!(!can_rename_collection_target(
            CollectionNodeKind::Root,
            "Demo"
        ));
        assert!(!can_rename_collection_target(
            CollectionNodeKind::Folder,
            ""
        ));
        assert!(!can_rename_collection_target(
            CollectionNodeKind::Request,
            "   "
        ));
        assert!(can_rename_collection_target(
            CollectionNodeKind::Folder,
            "Accounts"
        ));
        assert!(can_rename_collection_target(
            CollectionNodeKind::Request,
            "List users"
        ));
    }

    #[test]
    fn collection_rename_submit_matches_button_preconditions() {
        assert!(!can_submit_collection_rename(false, None, "Demo"));
        assert!(!can_submit_collection_rename(
            true,
            Some(CollectionNodeKind::Folder),
            "Demo"
        ));
        assert!(!can_submit_collection_rename(
            false,
            Some(CollectionNodeKind::Root),
            "Demo"
        ));
        assert!(!can_submit_collection_rename(
            false,
            Some(CollectionNodeKind::Request),
            "   "
        ));
        assert!(can_submit_collection_rename(
            false,
            Some(CollectionNodeKind::Folder),
            "Users"
        ));
        assert!(can_submit_collection_rename(
            false,
            Some(CollectionNodeKind::Request),
            "List users"
        ));
    }

    #[test]
    fn collection_save_requires_url_or_pre_request_url_action() {
        assert!(!can_save_current_request_to_collection("", ""));
        assert!(!can_save_current_request_to_collection(
            "   ",
            "# set_url https://api.example.com"
        ));
        assert!(!can_save_current_request_to_collection(
            "   ",
            "// url https://api.example.com"
        ));
        assert!(!can_save_current_request_to_collection("   ", "set_url"));
        assert!(can_save_current_request_to_collection(
            "https://api.example.com/users",
            ""
        ));
        assert!(can_save_current_request_to_collection(
            "   ",
            "set_url https://api.example.com/users"
        ));
        assert!(can_save_current_request_to_collection(
            "   ",
            "method POST; url https://api.example.com/users"
        ));
    }

    #[test]
    fn save_shortcut_matches_collection_save_preconditions() {
        assert!(!can_save_current_request_shortcut(false, "", ""));
        assert!(!can_save_current_request_shortcut(
            true,
            "https://api.example.com/users",
            ""
        ));
        assert!(can_save_current_request_shortcut(
            false,
            "https://api.example.com/users",
            ""
        ));
        assert!(can_save_current_request_shortcut(
            false,
            "   ",
            "set_url https://api.example.com/users"
        ));
    }

    #[test]
    fn request_send_requires_url_or_pre_request_url_action() {
        assert_eq!(REQUEST_URL_REQUIRED_TITLE, "No URL");
        assert_eq!(SAVE_URL_REQUIRED_TITLE, "No URL");
        assert_eq!(URL_REQUIRED_BODY, "URL is empty.");
        assert!(
            [
                REQUEST_URL_REQUIRED_TITLE,
                SAVE_URL_REQUIRED_TITLE,
                URL_REQUIRED_BODY
            ]
            .iter()
            .all(|label| !label.contains("Enter ")
                && !label.contains("select")
                && !label.contains("needs"))
        );
        assert!(!can_send_request("", ""));
        assert!(!can_send_request(
            "   ",
            "# set_url https://api.example.com"
        ));
        assert!(!can_send_request("   ", "// url https://api.example.com"));
        assert!(!can_send_request("   ", "url"));
        assert!(can_send_request("https://api.example.com/users", ""));
        assert!(can_send_request(
            "   ",
            "set_url https://api.example.com/users"
        ));
        assert!(can_send_request(
            "   ",
            "method POST; url https://api.example.com/users"
        ));
    }

    #[test]
    fn request_send_shortcut_matches_button_preconditions() {
        assert!(!can_send_request_shortcut(false, "", ""));
        assert!(!can_send_request_shortcut(
            true,
            "https://api.example.com/users",
            ""
        ));
        assert!(can_send_request_shortcut(
            false,
            "https://api.example.com/users",
            ""
        ));
        assert!(can_send_request_shortcut(
            false,
            "   ",
            "set_url https://api.example.com/users"
        ));
    }

    #[test]
    fn runner_stop_toggle_disables_while_running_or_busy() {
        assert!(runner_stop_toggle_enabled(false, false));
        assert!(!runner_stop_toggle_enabled(true, false));
        assert!(!runner_stop_toggle_enabled(false, true));
        assert!(!runner_stop_toggle_enabled(true, true));
        assert!(can_run_collection_runner(1, false, false));
        assert!(!can_run_collection_runner(0, false, false));
        assert!(!can_run_collection_runner(1, true, false));
        assert!(!can_run_collection_runner(1, false, true));
    }

    #[test]
    fn clearing_test_results_requires_results_and_idle_state() {
        assert!(!can_clear_response_assertion_results(false, 0));
        assert!(can_clear_response_assertion_results(false, 1));
        assert!(!can_clear_response_assertion_results(true, 1));
    }

    #[test]
    fn response_assertion_row_actions_require_idle_state_and_valid_index() {
        assert!(can_edit_response_assertion_row(false, 2, 1));
        assert!(!can_edit_response_assertion_row(true, 2, 1));
        assert!(!can_edit_response_assertion_row(false, 2, 2));
        assert!(can_remove_response_assertion_row(false, 2, 0));
        assert!(!can_remove_response_assertion_row(true, 2, 0));
        assert!(!can_remove_response_assertion_row(false, 0, 0));
    }

    #[test]
    fn key_value_presence_uses_key_text_only() {
        assert!(!key_value_key_is_present(""));
        assert!(!key_value_key_is_present("   "));
        assert!(key_value_key_is_present("Accept"));
        assert!(key_value_key_is_present("  X-Trace-Id  "));
    }

    #[test]
    fn request_address_border_ignores_method_menu_state() {
        assert_eq!(
            request_address_border_tone(false, false, true),
            RequestAddressBorderTone::Disabled
        );
        assert_eq!(
            request_address_border_tone(true, true, false),
            RequestAddressBorderTone::Focused
        );
        assert_eq!(
            request_address_border_tone(true, true, true),
            RequestAddressBorderTone::Focused
        );
        assert_eq!(
            request_address_border_tone(true, false, false),
            RequestAddressBorderTone::Default
        );
        assert_eq!(
            request_address_border_tone(true, false, true),
            RequestAddressBorderTone::Default
        );
    }

    #[test]
    fn workspace_split_ratio_updates_ignore_sub_pixel_noise() {
        assert!(!workspace_split_ratios_changed(0.28, 0.37, 0.2805, 0.37));
        assert!(!workspace_split_ratios_changed(0.28, 0.37, 0.28, 0.3705));
        assert!(workspace_split_ratios_changed(0.28, 0.37, 0.2812, 0.37));
        assert!(workspace_split_ratios_changed(0.28, 0.37, 0.28, 0.3712));
    }

    #[test]
    fn workspace_split_ratio_quantizes_drag_updates() {
        assert_eq!(
            quantize_workspace_split_ratio(0.501, 1000.),
            quantize_workspace_split_ratio(0.5015, 1000.)
        );
        assert_ne!(
            quantize_workspace_split_ratio(0.501, 1000.),
            quantize_workspace_split_ratio(0.526, 1000.)
        );
        assert_eq!(quantize_workspace_split_ratio(-0.2, 1000.), 0.0);
        assert_eq!(quantize_workspace_split_ratio(1.2, 1000.), 1.0);
    }

    #[test]
    fn workspace_split_preview_tracks_only_meaningful_changes() {
        assert_eq!(
            workspace_split_preview(WorkspaceSplitDrag::SidebarRequest, 0.28, 0.28, 0.37),
            None
        );
        assert_eq!(
            workspace_split_preview(WorkspaceSplitDrag::SidebarRequest, 0.30, 0.28, 0.37),
            Some(WorkspaceSplitPreview {
                split: WorkspaceSplitDrag::SidebarRequest,
                sidebar_ratio: 0.30,
                request_ratio: 0.37,
            })
        );
        assert_eq!(
            workspace_split_preview(WorkspaceSplitDrag::RequestResponse, 0.75, 0.25, 0.37),
            Some(WorkspaceSplitPreview {
                split: WorkspaceSplitDrag::RequestResponse,
                sidebar_ratio: 0.25,
                request_ratio: 0.50,
            })
        );
    }

    #[test]
    fn workspace_split_pending_only_changes_for_new_preview() {
        let preview = Some(WorkspaceSplitPreview {
            split: WorkspaceSplitDrag::SidebarRequest,
            sidebar_ratio: 0.30,
            request_ratio: 0.37,
        });

        assert!(!workspace_split_pending_changed(preview, preview));
        assert!(workspace_split_pending_changed(None, preview));
        assert!(workspace_split_pending_changed(preview, None));
        assert!(workspace_split_pending_changed(
            preview,
            Some(WorkspaceSplitPreview {
                split: WorkspaceSplitDrag::SidebarRequest,
                sidebar_ratio: 0.33,
                request_ratio: 0.37,
            })
        ));
    }

    #[test]
    fn workspace_split_target_ratios_keep_all_panes_visible() {
        assert_eq!(
            workspace_split_target_ratios(WorkspaceSplitDrag::SidebarRequest, 0.9, 0.28, 0.37),
            (0.38, 0.37)
        );
        assert_eq!(
            workspace_split_target_ratios(WorkspaceSplitDrag::SidebarRequest, 0.1, 0.28, 0.37),
            (0.24, 0.37)
        );
        let (sidebar, request) =
            workspace_split_target_ratios(WorkspaceSplitDrag::RequestResponse, 0.95, 0.28, 0.37);
        assert_eq!(sidebar, 0.28);
        assert!((request - 0.48).abs() < 0.0001);
        assert_eq!(
            workspace_split_target_ratios(WorkspaceSplitDrag::RequestResponse, 0.40, 0.28, 0.37),
            (0.28, 0.32)
        );
    }

    #[test]
    fn request_body_mode_labels_stay_compact() {
        let rows = request_body_mode_rows();
        assert_eq!(
            rows.map(|row| row.map(|(label, _)| label)),
            [["None", "Form", "URL Enc"], ["Raw", "GraphQL", "Binary"]]
        );
        assert!(
            rows.into_iter()
                .flatten()
                .all(|(label, _)| label.len() <= BODY_MODE_URL_ENCODED_LABEL.len())
        );
    }

    #[test]
    fn request_body_field_editor_titles_stay_compact() {
        assert_eq!(
            request_body_editor_title(RequestBodyMode::FormData),
            Some("Form")
        );
        assert_eq!(
            request_body_editor_title(RequestBodyMode::UrlEncoded),
            Some("URL Enc")
        );
        assert_eq!(request_body_editor_title(RequestBodyMode::Raw), None);
        assert_eq!(request_body_editor_title(RequestBodyMode::GraphQL), None);
        assert_eq!(request_body_editor_title(RequestBodyMode::Binary), None);
        assert!(
            request_body_editor_title(RequestBodyMode::FormData)
                .unwrap()
                .len()
                <= 4
        );
        assert!(
            request_body_editor_title(RequestBodyMode::UrlEncoded)
                .unwrap()
                .len()
                <= 7
        );
    }

    #[test]
    fn key_value_editor_key_column_width_respects_dense_body_tables() {
        assert_eq!(
            key_value_editor_key_column_width(HEADERS_PANEL_TITLE),
            KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH
        );
        assert_eq!(
            key_value_editor_key_column_width(PARAMS_PANEL_TITLE),
            KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH
        );
        assert_eq!(
            key_value_editor_key_column_width(BODY_FORM_FIELDS_TITLE),
            KEY_VALUE_EDITOR_COMPACT_KEY_COLUMN_WIDTH
        );
        assert_eq!(
            key_value_editor_key_column_width(BODY_URL_ENCODED_TITLE),
            KEY_VALUE_EDITOR_COMPACT_KEY_COLUMN_WIDTH
        );
        assert!(KEY_VALUE_EDITOR_COMPACT_KEY_COLUMN_WIDTH < KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH);
        assert!(KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH < KEY_VALUE_KEY_COLUMN_WIDTH);
    }

    #[test]
    fn key_value_editor_column_labels_match_editor_context() {
        assert_eq!(
            key_value_editor_column_labels(HEADERS_PANEL_TITLE),
            ("Header", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(REALTIME_WEBSOCKET_HEADERS_TITLE),
            ("Header", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(REALTIME_SSE_HEADERS_TITLE),
            ("Header", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(PARAMS_PANEL_TITLE),
            ("Param", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(VARIABLES_GLOBAL_TITLE),
            ("Var", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(VARIABLES_ENV_TITLE),
            ("Var", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(BODY_FORM_FIELDS_TITLE),
            ("Field", "Value")
        );
        assert_eq!(
            key_value_editor_column_labels(BODY_URL_ENCODED_TITLE),
            ("Name", "Value")
        );
        assert_eq!(key_value_editor_column_labels("Custom"), ("Key", "Value"));
    }

    #[test]
    fn key_value_add_row_respects_busy_and_active_environment() {
        assert!(can_add_key_value_row(
            false,
            KeyValueEditorTarget::QueryParams,
            false
        ));
        assert!(!can_add_key_value_row(
            true,
            KeyValueEditorTarget::QueryParams,
            true
        ));
        assert!(!can_add_key_value_row(
            false,
            KeyValueEditorTarget::ActiveEnvironmentVariables,
            false
        ));
        assert!(can_add_key_value_row(
            false,
            KeyValueEditorTarget::ActiveEnvironmentVariables,
            true
        ));
    }

    #[test]
    fn key_value_remove_row_respects_busy_active_environment_and_index() {
        assert!(can_remove_key_value_row(
            false,
            KeyValueEditorTarget::QueryParams,
            false,
            3,
            2
        ));
        assert!(!can_remove_key_value_row(
            true,
            KeyValueEditorTarget::QueryParams,
            true,
            3,
            1
        ));
        assert!(!can_remove_key_value_row(
            false,
            KeyValueEditorTarget::QueryParams,
            false,
            3,
            3
        ));
        assert!(!can_remove_key_value_row(
            false,
            KeyValueEditorTarget::ActiveEnvironmentVariables,
            false,
            3,
            1
        ));
        assert!(can_remove_key_value_row(
            false,
            KeyValueEditorTarget::ActiveEnvironmentVariables,
            true,
            3,
            1
        ));
    }

    #[test]
    fn key_value_add_row_placeholders_match_editor_context() {
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::QueryParams),
            ("param", "value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::RequestHeaders),
            ("Header", "Value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::WebSocketHeaders),
            ("Header", "Value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::SseHeaders),
            ("Header", "Value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::GlobalVariables),
            ("var", "value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::ActiveEnvironmentVariables),
            ("var", "value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::FormDataBody),
            ("field", "value")
        );
        assert_eq!(
            key_value_editor_add_placeholders(KeyValueEditorTarget::UrlEncodedBody),
            ("name", "value")
        );
    }

    #[test]
    fn maps_http_status_to_response_tone() {
        assert!(matches!(response_tone(200), ResponseTone::Success));
        assert!(matches!(response_tone(302), ResponseTone::Success));
        assert!(matches!(response_tone(100), ResponseTone::Neutral));
        assert!(matches!(response_tone(404), ResponseTone::Error));
        assert!(matches!(response_tone(500), ResponseTone::Error));
    }

    #[test]
    fn formats_assertion_summary_meta() {
        assert_eq!(assertion_meta(&[]), None);
        assert_eq!(response_panel_meta(&[]), "");

        let results = vec![
            ResponseAssertionResult {
                name: "status".to_string(),
                passed: true,
                error: None,
            },
            ResponseAssertionResult {
                name: "body".to_string(),
                passed: false,
                error: Some("missing body text".to_string()),
            },
        ];

        assert_eq!(assertion_meta(&results).as_deref(), Some("1/2 tests"));
        assert_eq!(response_panel_meta(&results), "1/2 tests");
    }

    #[test]
    fn runner_summary_includes_pre_request_action_log() {
        let summary = CollectionRunSummary {
            collection_name: "Demo".to_string(),
            total: 1,
            passed: 1,
            failed: 0,
            stopped_early: false,
            elapsed_ms: 7,
            results: vec![CollectionRunResult {
                index: 0,
                path: vec!["Demo".to_string(), "Health".to_string()],
                name: "Health".to_string(),
                method: "GET".to_string(),
                url: "http://localhost/health".to_string(),
                status: Some(200),
                success: true,
                elapsed_ms: 3,
                body_bytes: 2,
                pre_request_actions: vec![
                    "set_var token".to_string(),
                    "set_header Authorization".to_string(),
                ],
                assertions: Vec::new(),
                error: None,
            }],
        };

        let text = runner_summary_text(&summary);

        assert_eq!(runner_status_text(&summary), "P 1 / F 0 / T 1");
        assert!(text.contains("P 1 / F 0 / T 1"));
        assert_eq!(runner_outcome_label(true), "OK");
        assert_eq!(runner_outcome_label(false), "ERR");
        assert!(text.contains("[OK] pre set_var token"));
        assert!(text.contains("[OK] pre set_header Authorization"));
        assert!(!text.contains("[PASS]"));
        assert!(!text.contains("[FAIL]"));
        assert!(!text.contains("pre-request"));
        assert!(!text.contains("passed,"));
        assert!(!text.contains("failed,"));
        assert!(!text.contains(" ms"));
        assert!(!text.contains(" B"));
    }

    #[test]
    fn formats_response_headers() {
        let headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("x-request-id".to_string(), "abc".to_string()),
        ];

        assert_eq!(
            format_headers(&headers),
            "content-type: application/json\nx-request-id: abc"
        );
    }

    #[test]
    fn collapses_json_response_preview() {
        let collapsed = collapsed_json_preview(
            r#"{"users":[{"id":1,"name":"Zen"}],"ok":true,"meta":{"page":1}}"#,
        )
        .expect("json");

        assert!(collapsed.contains("{ // 3 keys"));
        assert!(collapsed.contains("\"users\": [...] // 1 items"));
        assert!(collapsed.contains("\"ok\": true"));
        assert!(collapsed.contains("\"meta\": {...} // 1 keys"));
    }

    #[test]
    fn collapsed_json_preview_rejects_non_json() {
        assert_eq!(collapsed_json_preview("not-json"), None);
        assert!(!response_can_collapse("not-json"));
        assert!(!response_can_collapse("   "));
        assert!(response_can_collapse(r#"{"ok":true}"#));
        assert!(can_toggle_response_collapse(false, r#"{"ok":true}"#));
        assert!(!can_toggle_response_collapse(true, r#"{"ok":true}"#));
        assert!(!can_toggle_response_collapse(false, "not-json"));
    }

    #[test]
    fn formats_raw_json_body_text() {
        assert_eq!(
            formatted_json_body(r#"{"ok":true,"items":[1,2]}"#).expect("formatted"),
            "{\n  \"ok\": true,\n  \"items\": [\n    1,\n    2\n  ]\n}"
        );
        assert_eq!(
            formatted_json_body("   ")
                .expect_err("empty body")
                .to_string(),
            RAW_JSON_EMPTY_FORMAT_BODY
        );
        assert!(
            !RAW_JSON_EMPTY_FORMAT_BODY.contains("Enter ")
                && !RAW_JSON_EMPTY_FORMAT_BODY.contains(" before ")
        );
        assert!(formatted_json_body("{not-json").is_err());
    }

    #[test]
    fn raw_json_format_action_requires_json_mode_and_body() {
        assert_eq!(RAW_FORMATTED_TITLE, "Formatted");
        assert_eq!(RAW_FORMATTED_BODY, "Body.");
        assert!(
            [RAW_FORMATTED_TITLE, RAW_FORMATTED_BODY]
                .iter()
                .all(|label| label.len() <= 9
                    && !label.contains("Body formatted")
                    && !label.contains("Formatted."))
        );
        assert!(can_format_raw_json(RawBodyFormat::Json, r#"{"ok":true}"#));
        assert!(!can_format_raw_json(RawBodyFormat::Json, "   "));
        assert!(!can_format_raw_json(RawBodyFormat::Text, r#"{"ok":true}"#));
        assert!(!can_format_raw_json(RawBodyFormat::Xml, "<ok />"));
        assert!(can_format_request_raw_json(
            false,
            RawBodyFormat::Json,
            r#"{"ok":true}"#
        ));
        assert!(!can_format_request_raw_json(
            true,
            RawBodyFormat::Json,
            r#"{"ok":true}"#
        ));
    }

    #[test]
    fn response_view_text_matches_active_tab_for_copy() {
        let pretty = "{\n  \"ok\": true\n}";
        let raw = r#"{"ok":true}"#;
        let headers = "content-type: application/json";

        assert_eq!(INITIAL_RESPONSE_BODY, "No response");
        assert!(!INITIAL_RESPONSE_BODY.contains("Import"));
        assert!(INITIAL_RESPONSE_BODY.len() <= 12);
        assert_eq!(RESPONSE_HEADERS_EMPTY_LABEL, "No headers");
        assert!(!RESPONSE_HEADERS_EMPTY_LABEL.contains("response"));
        assert!(RESPONSE_HEADERS_EMPTY_LABEL.len() <= 10);
        assert_eq!(
            response_body_for_view(ResponseView::Pretty, false, pretty, raw, headers),
            pretty
        );
        assert!(
            response_body_for_view(ResponseView::Pretty, true, pretty, raw, headers)
                .contains("\"ok\": true")
        );
        assert_eq!(
            response_body_for_view(ResponseView::Raw, false, pretty, raw, headers),
            raw
        );
        assert_eq!(
            response_body_for_view(ResponseView::Headers, false, pretty, raw, headers),
            headers
        );
        assert_eq!(
            response_body_for_view(ResponseView::Headers, false, pretty, raw, ""),
            RESPONSE_HEADERS_EMPTY_LABEL
        );
        assert_eq!(
            response_body_for_view(ResponseView::Headers, false, pretty, raw, "   \n\t"),
            RESPONSE_HEADERS_EMPTY_LABEL
        );
        assert_eq!(
            response_copy_text(ResponseView::Pretty, false, pretty, raw, headers).as_deref(),
            Some(pretty)
        );
        assert_eq!(
            response_copy_text(ResponseView::Raw, false, pretty, raw, headers).as_deref(),
            Some(raw)
        );
        assert_eq!(
            response_copy_text(ResponseView::Headers, false, pretty, raw, headers).as_deref(),
            Some(headers)
        );
        assert_eq!(
            response_copy_text(ResponseView::Headers, false, pretty, raw, ""),
            None
        );
        assert_eq!(
            response_copy_text(ResponseView::Headers, false, pretty, raw, "   \n\t"),
            None
        );
        assert_eq!(
            response_copy_text(ResponseView::Pretty, false, "   ", raw, headers),
            None
        );
        assert!(can_copy_response_view(
            false,
            ResponseView::Pretty,
            false,
            pretty,
            raw,
            headers
        ));
        assert!(!can_copy_response_view(
            true,
            ResponseView::Pretty,
            false,
            pretty,
            raw,
            headers
        ));
        assert!(!can_copy_response_view(
            false,
            ResponseView::Headers,
            false,
            pretty,
            raw,
            ""
        ));
    }

    #[test]
    fn response_toolbar_labels_stay_compact() {
        assert_eq!(
            response_tabs().map(|(label, _)| label),
            [
                RESPONSE_PRETTY_LABEL,
                RESPONSE_RAW_LABEL,
                RESPONSE_HEADERS_LABEL
            ]
        );

        let labels = [
            RESPONSE_PRETTY_LABEL,
            RESPONSE_RAW_LABEL,
            RESPONSE_HEADERS_LABEL,
            RESPONSE_FOLD_LABEL,
            RESPONSE_OPEN_LABEL,
            RESPONSE_COPY_LABEL,
        ];
        assert_eq!(labels, ["Pretty", "Raw", "Hdrs", "Fold", "Open", "Copy"]);
        assert!(labels.iter().all(|label| label.len() <= 6));
    }

    #[test]
    fn parses_bulk_header_text() {
        let headers = parse_header_bulk(
            r#"
Accept: application/json
Authorization=Bearer token
-H 'X-Trace-Id: abc'
--header "X-Mode: test"
# ignored
Cookie: a=b; c=d
"#,
        );

        assert_eq!(
            headers,
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("Authorization".to_string(), "Bearer token".to_string()),
                ("X-Trace-Id".to_string(), "abc".to_string()),
                ("X-Mode".to_string(), "test".to_string()),
                ("Cookie".to_string(), "a=b; c=d".to_string()),
            ]
        );
    }

    #[test]
    fn formats_bulk_headers_for_clipboard() {
        let headers = vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("X-Trace-Id".to_string(), "abc".to_string()),
        ];

        assert_eq!(
            format_header_bulk(&headers),
            "Accept: application/json\nX-Trace-Id: abc"
        );
        assert!(!can_copy_headers_bulk(false, false));
        assert!(can_copy_headers_bulk(false, true));
        assert!(!can_copy_headers_bulk(true, true));
        assert_eq!(HEADER_COPY_EMPTY_TITLE, "No copy");
        assert_eq!(HEADER_COPIED_TITLE, "Copied");
        assert_eq!(HEADER_APPLIED_TITLE, "Applied");
        assert_eq!(HEADER_BULK_HEADERS_BODY, "Headers.");
        assert!(
            [
                HEADER_COPY_EMPTY_TITLE,
                HEADER_COPIED_TITLE,
                HEADER_APPLIED_TITLE,
                HEADER_BULK_HEADERS_BODY
            ]
            .iter()
            .all(|label| label.len() <= 8
                && !label.contains("Headers copied")
                && !label.contains("Headers pasted")
                && !label.contains("copied."))
        );
        assert_eq!(HEADER_BULK_CLIPBOARD_EMPTY_TITLE, "No paste");
        assert_eq!(HEADER_BULK_CLIPBOARD_EMPTY_BODY, "Clipboard empty.");
        assert_eq!(HEADER_BULK_PARSE_EMPTY_TITLE, "No paste");
        assert_eq!(HEADER_BULK_PARSE_EMPTY_BODY, "No headers.");
        assert!(
            [
                HEADER_BULK_CLIPBOARD_EMPTY_TITLE,
                HEADER_BULK_CLIPBOARD_EMPTY_BODY,
                HEADER_BULK_PARSE_EMPTY_TITLE,
                HEADER_BULK_PARSE_EMPTY_BODY
            ]
            .iter()
            .all(|label| label.len() <= 16
                && !label.contains("Use ")
                && !label.contains("for example")
                && !label.contains("headers parsed")
                && !label.contains("clipboard text"))
        );
    }

    #[test]
    fn header_presets_upsert_without_duplicate_names() {
        assert_eq!(HEADER_APPLIED_TITLE, "Applied");
        assert_eq!(HEADER_PRESET_BODY, "Header.");
        assert!(
            [HEADER_APPLIED_TITLE, HEADER_PRESET_BODY]
                .iter()
                .all(|label| label.len() <= 8
                    && !label.contains("Header preset applied")
                    && !label.contains(": "))
        );

        let headers = vec![
            ("accept".to_string(), "text/plain".to_string()),
            ("X-Trace-Id".to_string(), "abc".to_string()),
        ];

        let headers = upsert_header_pair(&headers, "Accept", "application/json");
        assert_eq!(
            headers,
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("X-Trace-Id".to_string(), "abc".to_string()),
            ]
        );

        let headers = upsert_header_pair(&headers, "Authorization", "Bearer {{token}}");
        assert_eq!(
            headers,
            vec![
                ("Accept".to_string(), "application/json".to_string()),
                ("X-Trace-Id".to_string(), "abc".to_string()),
                ("Authorization".to_string(), "Bearer {{token}}".to_string()),
            ]
        );
    }

    #[test]
    fn builds_response_assertions_from_editor_fields() {
        let assertion =
            response_assertion_from_fields(TestAssertionKind::StatusInRange, "2xx", "200", "299")
                .expect("assertion")
                .expect("configured");

        assert_eq!(assertion.name, "2xx");
        assert!(matches!(
            assertion.kind,
            ResponseAssertionKind::StatusInRange { min: 200, max: 299 }
        ));

        let json =
            response_assertion_from_fields(TestAssertionKind::JsonPathEquals, "", "ok", "true")
                .expect("json assertion")
                .expect("configured");

        assert_eq!(json.name, "JSON = ok");
        assert_eq!(
            json.kind,
            ResponseAssertionKind::JsonPathEquals {
                path: "ok".to_string(),
                value: serde_json::Value::Bool(true),
            }
        );
    }

    #[test]
    fn rejects_invalid_response_assertion_fields() {
        let invalid_status =
            response_assertion_from_fields(TestAssertionKind::StatusEquals, "", "abc", "")
                .expect_err("invalid status");
        assert_eq!(invalid_status.to_string(), "Bad status.");

        let invalid_range =
            response_assertion_from_fields(TestAssertionKind::StatusInRange, "", "500", "200")
                .expect_err("invalid range");
        assert_eq!(invalid_range.to_string(), "Min > max status.");

        let missing_target =
            response_assertion_from_fields(TestAssertionKind::HeaderExists, "", "", "ignored")
                .expect_err("missing target");
        assert_eq!(missing_target.to_string(), "Target is empty.");

        let missing_expected =
            response_assertion_from_fields(TestAssertionKind::JsonPathEquals, "", "$.ok", "")
                .expect_err("missing expected");
        assert_eq!(missing_expected.to_string(), "Expected is empty.");

        assert!(
            response_assertion_from_fields(TestAssertionKind::HeaderExists, "", "", "")
                .expect("empty row")
                .is_none()
        );
    }

    #[test]
    fn formats_pre_request_status_labels() {
        assert_eq!(pre_request_status_label(0), "idle");
        assert_eq!(pre_request_status_label(1), "1 act");
        assert_eq!(pre_request_status_label(3), "3 act");
        assert_eq!(
            pre_request_error_label("set_header expects name=value"),
            "Err set_header expects name=value"
        );
    }

    #[test]
    fn user_visible_errors_keep_context_without_guidance_blocks() {
        let file_error =
            file_operation_error("OpenAPI import failed.", "/tmp/api.yaml", "No such file");
        assert!(file_error.starts_with("OpenAPI import failed."));
        assert!(file_error.contains("Path\n/tmp/api.yaml"));
        assert!(file_error.contains("Error\nNo such file"));
        assert!(!file_error.contains("Next step"));

        let request_error = request_transport_error(
            "POST",
            "https://api.example.com/users",
            "connection refused",
        );
        assert!(request_error.starts_with("Request failed."));
        assert!(request_error.contains("Request\nPOST https://api.example.com/users"));
        assert!(request_error.contains("Error\nconnection refused"));
        assert!(!request_error.contains("Could not"));
        assert!(!request_error.contains("TLS settings"));
        assert!(!request_error.contains("Next step"));

        let mock_error = mock_server_error(MOCK_SERVER_PORT, "");
        assert!(mock_error.starts_with("Mock start failed."));
        assert!(mock_error.contains("Port\n8080"));
        assert!(mock_error.contains("Unknown error"));
        assert!(!mock_error.contains("Could not"));
        assert!(!mock_error.contains("Next step"));

        let editor = editor_error("Request build failed.", "bad input");
        assert!(editor.starts_with("Request build failed."));
        assert!(editor.contains("Error\nbad input"));
        assert!(!editor.contains("Could not"));
        assert!(!editor.contains("Next step"));

        let realtime = realtime_operation_error(
            "SSE subscription failed.",
            "SSE URL",
            "https://api.example.com/events",
            "wrong content-type",
        );
        assert!(realtime.starts_with("SSE subscription failed."));
        assert!(realtime.contains("SSE URL\nhttps://api.example.com/events"));
        assert!(realtime.contains("Error\nwrong content-type"));
        assert!(!realtime.contains("Could not"));
        assert!(!realtime.contains("lost connection"));
        assert!(!realtime.contains("Next step"));

        assert_eq!(
            runner_worker_stopped_message(),
            "Collection runner stopped."
        );
        assert!(!runner_worker_stopped_message().contains("Next step"));
    }

    #[test]
    fn response_status_bodies_stay_short() {
        let success_titles = [
            RESPONSE_TITLE_IMPORTED,
            RESPONSE_TITLE_EXPORTED,
            RESPONSE_TITLE_SAVED,
            RESPONSE_TITLE_RESTORED,
        ];
        assert_eq!(
            success_titles,
            ["Imported", "Exported", "Saved", "Restored"]
        );
        assert!(success_titles.iter().all(|title| title.len() <= 8));
        assert!(success_titles.iter().all(|title| {
            !title.contains("Collection")
                && !title.contains("Request")
                && !title.contains("current")
        }));
        assert_eq!(RESPONSE_BODY_REQUEST, "Request.");
        assert!(
            [RESPONSE_TITLE_RESTORED, RESPONSE_BODY_REQUEST]
                .iter()
                .all(|label| !label.contains("Collection request") && !label.contains("Restored."))
        );
        assert_eq!(RUNNER_ACTIVE_TITLE, "Running");
        assert_eq!(RESPONSE_BODY_RUNNER, "Runner.");
        assert!(
            [RUNNER_ACTIVE_TITLE, RESPONSE_BODY_RUNNER]
                .iter()
                .all(|label| !label.contains("Runner active") && !label.contains("Running."))
        );
        let failure_titles = [
            RESPONSE_TITLE_SELECTED,
            RESPONSE_TITLE_IMPORT_FAIL,
            RESPONSE_TITLE_EXPORT_FAIL,
            RESPONSE_TITLE_SAVE_FAIL,
            RESPONSE_TITLE_BUILD_FAIL,
            RESPONSE_TITLE_BAD_TESTS,
            RESPONSE_TITLE_REQUEST_FAIL,
            RESPONSE_TITLE_RUNNER_FAIL,
            RESPONSE_TITLE_RUN_PASSED,
            RESPONSE_TITLE_RUN_STOPPED,
            RESPONSE_TITLE_RUN_FAILED,
            RESPONSE_TITLE_FORMAT_FAIL,
        ];
        assert_eq!(
            failure_titles,
            [
                "Selected",
                "Import fail",
                "Export fail",
                "Save fail",
                "Build fail",
                "Bad tests",
                "Request fail",
                "Runner fail",
                "Run passed",
                "Run stopped",
                "Run fail",
                "Format fail"
            ]
        );
        assert!(failure_titles.iter().all(|title| title.len() <= 12
            && !title.contains(" failed")
            && !title.contains(" invalid")
            && !title.contains("Collection")
            && !title.contains("Route selected")
            && !title.contains("Body format")));

        let bodies = [
            VARIABLES_ENV_RESPONSE_BODY,
            RESPONSE_BODY_REQUEST,
            RESPONSE_BODY_RUNNER,
            SSE_STOPPED_BODY,
            NO_HEADERS_BODY,
            NO_MESSAGES_BODY,
            NO_EVENTS_BODY,
        ];

        assert_eq!(
            bodies,
            [
                "Env.",
                "Request.",
                "Runner.",
                "SSE.",
                "No headers.",
                "No messages.",
                "No events."
            ]
        );
        assert!(bodies.iter().all(|body| body.len() <= 12));
        assert!(
            [
                VARIABLES_ENV_SELECTED_TITLE,
                VARIABLES_ENV_CREATED_TITLE,
                VARIABLES_ENV_DELETED_TITLE,
                VARIABLES_ENV_RESPONSE_BODY
            ]
            .iter()
            .all(|label| !label.contains("Env active")
                && !label.contains("Env created")
                && !label.contains("Env deleted")
                && !label.contains("Active."))
        );
        assert!(bodies.iter().all(|body| {
            !body.contains(" was ")
                && !body.contains(" were ")
                && !body.contains("There are")
                && !body.contains(" is now ")
                && !body.contains("executing")
        }));
    }

    #[test]
    fn pre_request_action_labels_do_not_include_values() {
        let execution = execute_pre_request_actions(
            "set_var token=secret; set_header Authorization=Bearer {{token}}",
            CodegenRequest {
                method: "GET".to_string(),
                url: "https://api.example.com".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: RequestBody::None,
            },
            VariableStore::new(),
            None,
        )
        .expect("pre-request execution");

        let labels = pre_request_action_labels(&execution.actions);

        assert_eq!(
            labels,
            vec![
                "set_var token".to_string(),
                "set_header Authorization".to_string(),
            ]
        );
        assert!(!labels.join("\n").contains("secret"));
    }

    #[test]
    fn builds_bearer_basic_oauth2_and_api_key_pairs() {
        assert_eq!(
            bearer_auth_pair(" token "),
            Some(("Authorization".to_string(), "Bearer token".to_string()))
        );
        assert_eq!(
            oauth2_access_token_pair(" oauth-token "),
            Some((
                "Authorization".to_string(),
                "Bearer oauth-token".to_string()
            ))
        );
        assert_eq!(
            basic_auth_pair("dev", "secret"),
            Some((
                "Authorization".to_string(),
                "Basic ZGV2OnNlY3JldA==".to_string()
            ))
        );
        assert_eq!(
            jwt_auth_pair(" ey.jwt.token "),
            Some((
                "Authorization".to_string(),
                "Bearer ey.jwt.token".to_string()
            ))
        );
        assert_eq!(
            api_key_pair("X-API-Key", " key "),
            Some(("X-API-Key".to_string(), "key".to_string()))
        );
        assert_eq!(bearer_auth_pair(" "), None);
        assert_eq!(oauth2_access_token_pair(" "), None);
        assert_eq!(jwt_auth_pair(" "), None);
        assert_eq!(basic_auth_pair("", "secret"), None);
        assert_eq!(api_key_pair("", "key"), None);
    }

    #[test]
    fn builds_variable_store_and_resolves_request_templates() {
        let store = variable_store_from_pairs(
            vec![
                (
                    "baseUrl".to_string(),
                    "https://prod.example.com".to_string(),
                ),
                ("token".to_string(), "prod-token".to_string()),
            ],
            Some("dev"),
            vec![
                ("baseUrl".to_string(), "http://localhost:8080".to_string()),
                ("token".to_string(), "dev-token".to_string()),
            ],
        );

        assert_eq!(
            resolve_template("{{baseUrl}}/users", &store, Some("dev")).expect("url"),
            "http://localhost:8080/users"
        );
        assert_eq!(
            resolve_key_value_pairs(
                vec![("Authorization".to_string(), "Bearer {{token}}".to_string())],
                &store,
                Some("dev"),
            )
            .expect("headers"),
            vec![("Authorization".to_string(), "Bearer dev-token".to_string())]
        );
    }

    #[test]
    fn normalizes_custom_environment_names() {
        assert_eq!(normalized_environment_name(" staging "), "staging");
        assert_eq!(
            normalized_environment_name(" qa team  west "),
            "qa-team-west"
        );
        assert_eq!(normalized_environment_name("   "), "");
    }

    #[test]
    fn add_environment_requires_normalized_name() {
        assert_eq!(VARIABLES_ENV_NAME_REQUIRED_BODY, "Env name is empty.");
        assert!(
            !VARIABLES_ENV_NAME_REQUIRED_BODY.contains("Enter ")
                && !VARIABLES_ENV_NAME_REQUIRED_BODY.contains(" before ")
        );
        assert!(!can_add_environment(""));
        assert!(!can_add_environment("   \n\t"));
        assert!(can_add_environment("staging"));
        assert!(can_add_environment(" qa team  west "));
        assert!(can_submit_environment_add(false, "staging"));
        assert!(!can_submit_environment_add(true, "staging"));
        assert!(!can_submit_environment_add(false, "   "));
        assert!(can_delete_environment(false, true));
        assert!(!can_delete_environment(false, false));
        assert!(!can_delete_environment(true, true));
    }

    #[test]
    fn resolves_variables_for_custom_environments() {
        let store = variable_store_from_pairs(
            vec![("baseUrl".to_string(), "https://api.example.com".to_string())],
            Some("qa-team-west"),
            vec![(
                "baseUrl".to_string(),
                "https://qa-west.example.com".to_string(),
            )],
        );

        assert_eq!(
            resolve_template("{{baseUrl}}/health", &store, Some("qa-team-west"))
                .expect("custom environment"),
            "https://qa-west.example.com/health"
        );
    }

    #[test]
    fn resolves_variables_in_all_request_body_modes() {
        let store = variable_store_from_pairs(
            vec![
                ("name".to_string(), "Zen".to_string()),
                ("file".to_string(), "/tmp/upload.bin".to_string()),
            ],
            None,
            Vec::new(),
        );

        assert_eq!(
            resolve_request_body(
                RequestBody::Raw {
                    content_type: Some("application/json".to_string()),
                    body: "{\"name\":\"{{name}}\"}".to_string(),
                },
                &store,
                None,
            )
            .expect("raw"),
            RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            }
        );
        assert_eq!(
            resolve_request_body(
                RequestBody::FormUrlEncoded(vec![("name".to_string(), "{{name}}".to_string(),)]),
                &store,
                None,
            )
            .expect("urlencoded"),
            RequestBody::FormUrlEncoded(vec![("name".to_string(), "Zen".to_string())])
        );
        assert_eq!(
            resolve_request_body(
                RequestBody::Multipart(vec![("file".to_string(), "@{{file}}".to_string(),)]),
                &store,
                None,
            )
            .expect("multipart"),
            RequestBody::Multipart(vec![("file".to_string(), "@/tmp/upload.bin".to_string())])
        );
        assert_eq!(
            resolve_request_body(
                RequestBody::BinaryFile {
                    path: "{{file}}".to_string(),
                    content_type: Some("application/octet-stream".to_string()),
                },
                &store,
                None,
            )
            .expect("binary"),
            RequestBody::BinaryFile {
                path: "/tmp/upload.bin".to_string(),
                content_type: Some("application/octet-stream".to_string()),
            }
        );
    }

    #[test]
    fn builds_graphql_body_and_extracts_saved_graphql_fields() {
        let body = graphql_body(
            "query User($id: ID!) { user(id: $id) { name } }",
            r#"{"id":"42"}"#,
        );
        let value = serde_json::from_str::<serde_json::Value>(&body).expect("graphql json");

        assert_eq!(
            value["query"],
            "query User($id: ID!) { user(id: $id) { name } }"
        );
        assert_eq!(value["variables"]["id"], "42");

        let (query, variables) =
            graphql_fields_from_body("application/json", &body).expect("graphql fields");
        assert_eq!(query, "query User($id: ID!) { user(id: $id) { name } }");
        assert!(variables.contains("\"id\": \"42\""));

        let empty_variables = graphql_body("{ viewer { login } }", "not-json");
        let value = serde_json::from_str::<serde_json::Value>(&empty_variables).expect("json");
        assert_eq!(value["variables"], serde_json::json!({}));
    }

    #[test]
    fn graphql_introspection_query_builds_schema_request_body() {
        let body = graphql_body(GRAPHQL_INTROSPECTION_QUERY, "{}");
        let value = serde_json::from_str::<serde_json::Value>(&body).expect("introspection json");
        let query = value["query"].as_str().expect("query");

        assert!(query.contains("__schema"));
        assert!(query.contains("queryType"));
        assert!(query.contains("directives"));
        assert_eq!(value["variables"], serde_json::json!({}));
    }

    #[test]
    fn summarizes_graphql_introspection_response() {
        let response = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": { "name": "Mutation" },
                    "subscriptionType": null,
                    "types": [
                        { "kind": "OBJECT", "name": "Query", "fields": [
                            { "name": "viewer" },
                            { "name": "node" }
                        ] },
                        { "kind": "OBJECT", "name": "Mutation", "fields": [
                            { "name": "createUser" }
                        ] },
                        { "kind": "SCALAR", "name": "String" },
                        { "kind": "ENUM", "name": "Role" },
                        { "kind": "INPUT_OBJECT", "name": "UserInput" }
                    ],
                    "directives": [
                        { "name": "include" },
                        { "name": "skip" }
                    ]
                }
            }
        })
        .to_string();

        let summary = graphql_schema_summary(&response).expect("schema");

        assert!(summary.contains("Roots Q=Query M=Mutation S=-"));
        assert!(summary.contains("Types 5 | O2 I1 E1 S1"));
        assert!(summary.contains("Fields Q2 M1 S0"));
        assert!(summary.contains("Dirs 2"));
        assert!(summary.lines().all(|line| line.len() <= 30));
        assert!(!summary.contains("roots:"));
        assert!(!summary.contains("types:"));
        assert!(!summary.contains("fields:"));
        assert!(!summary.contains("directives:"));
        assert!(!summary.contains("Directives"));
        assert!(!summary.contains("mutation "));
        assert!(!summary.contains("subscription "));
        assert_eq!(graphql_schema_summary(r#"{"data":{"ok":true}}"#), None);
    }

    #[test]
    fn browses_graphql_introspection_response() {
        let response = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": { "name": "Mutation" },
                    "subscriptionType": null,
                    "types": [
                        { "kind": "OBJECT", "name": "Query", "fields": [
                            {
                                "name": "viewer",
                                "args": [],
                                "type": {
                                    "kind": "NON_NULL",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" }
                                }
                            },
                            {
                                "name": "search",
                                "args": [
                                    {
                                        "name": "term",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": null,
                                            "ofType": { "kind": "SCALAR", "name": "String" }
                                        }
                                    },
                                    {
                                        "name": "limit",
                                        "type": { "kind": "SCALAR", "name": "Int" },
                                        "defaultValue": "10"
                                    }
                                ],
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" }
                                }
                            },
                            {
                                "name": "legacy",
                                "args": [],
                                "type": { "kind": "SCALAR", "name": "String" },
                                "isDeprecated": true
                            }
                        ] },
                        { "kind": "OBJECT", "name": "Mutation", "fields": [
                            {
                                "name": "createUser",
                                "args": [],
                                "type": { "kind": "OBJECT", "name": "User" }
                            }
                        ] },
                        { "kind": "OBJECT", "name": "User", "fields": [] },
                        { "kind": "OBJECT", "name": "__Schema", "fields": [] },
                        { "kind": "SCALAR", "name": "String" },
                        { "kind": "SCALAR", "name": "Int" },
                        { "kind": "ENUM", "name": "Role" },
                        { "kind": "INPUT_OBJECT", "name": "UserInput" }
                    ],
                    "directives": [
                        { "name": "skip" },
                        { "name": "include" }
                    ]
                }
            }
        })
        .to_string();

        let browser = graphql_schema_browser(&response).expect("schema browser");

        assert!(browser.contains("Q (Query)"));
        assert!(browser.contains("viewer: User!"));
        assert!(browser.contains("search(term: String!, limit: Int = 10): [User]"));
        assert!(browser.contains("legacy: String @deprecated"));
        assert!(browser.contains("M (Mutation)"));
        assert!(browser.contains("createUser: User"));
        assert!(browser.contains("Types"));
        assert!(browser.contains("Obj: Mutation, Query, User"));
        assert!(browser.contains("In: UserInput"));
        assert!(browser.contains("Enum: Role"));
        assert!(browser.contains("Scalar: Int, String"));
        assert!(browser.contains("Dirs\n  @include, @skip"));
        assert!(!browser.contains("query fields"));
        assert!(!browser.contains("mutation fields"));
        assert!(!browser.contains("type index"));
        assert!(!browser.contains("objects:"));
        assert!(!browser.contains("inputs:"));
        assert!(!browser.contains("directives\n"));
        assert!(!browser.contains("__Schema"));
        assert_eq!(graphql_schema_browser(r#"{"data":{"ok":true}}"#), None);
    }

    #[test]
    fn generates_graphql_query_templates_from_introspection_response() {
        let response = serde_json::json!({
            "data": {
                "__schema": {
                    "queryType": { "name": "Query" },
                    "mutationType": null,
                    "subscriptionType": null,
                    "types": [
                        { "kind": "OBJECT", "name": "Query", "fields": [
                            {
                                "name": "viewer",
                                "args": [
                                    {
                                        "name": "id",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": null,
                                            "ofType": { "kind": "SCALAR", "name": "ID" }
                                        }
                                    }
                                ],
                                "type": { "kind": "OBJECT", "name": "User" }
                            },
                            {
                                "name": "search",
                                "args": [
                                    {
                                        "name": "term",
                                        "type": {
                                            "kind": "NON_NULL",
                                            "name": null,
                                            "ofType": { "kind": "SCALAR", "name": "String" }
                                        }
                                    },
                                    {
                                        "name": "limit",
                                        "type": { "kind": "SCALAR", "name": "Int" },
                                        "defaultValue": "10"
                                    },
                                    {
                                        "name": "active",
                                        "type": { "kind": "SCALAR", "name": "Boolean" }
                                    }
                                ],
                                "type": {
                                    "kind": "LIST",
                                    "name": null,
                                    "ofType": { "kind": "OBJECT", "name": "User" }
                                }
                            },
                            {
                                "name": "version",
                                "args": [],
                                "type": { "kind": "SCALAR", "name": "String" }
                            }
                        ] },
                        { "kind": "OBJECT", "name": "User", "fields": [] },
                        { "kind": "SCALAR", "name": "ID" },
                        { "kind": "SCALAR", "name": "String" },
                        { "kind": "SCALAR", "name": "Int" },
                        { "kind": "SCALAR", "name": "Boolean" }
                    ],
                    "directives": []
                }
            }
        })
        .to_string();

        let templates = graphql_query_templates(&response).expect("query templates");

        assert_eq!(templates.len(), 3);
        assert_eq!(templates[0].field_name, "viewer");
        assert!(
            templates[0]
                .operation
                .contains("query ViewerQuery($id: ID!)")
        );
        assert!(templates[0].operation.contains("viewer(id: $id) {"));
        assert!(templates[0].operation.contains("__typename"));

        let viewer_variables =
            serde_json::from_str::<serde_json::Value>(&templates[0].variables).expect("variables");
        assert_eq!(viewer_variables["id"], "<id>");

        assert_eq!(templates[1].field_name, "search");
        assert!(
            templates[1]
                .operation
                .contains("$term: String!, $limit: Int = 10, $active: Boolean")
        );
        assert!(
            templates[1]
                .operation
                .contains("search(term: $term, limit: $limit, active: $active)")
        );
        let search_variables =
            serde_json::from_str::<serde_json::Value>(&templates[1].variables).expect("variables");
        assert_eq!(search_variables["term"], "<term>");
        assert_eq!(search_variables["limit"], 0);
        assert_eq!(search_variables["active"], false);

        assert_eq!(templates[2].field_name, "version");
        assert!(templates[2].operation.contains("query VersionQuery"));
        assert!(templates[2].operation.contains("version"));
        assert!(!templates[2].operation.contains("__typename"));
        assert_eq!(templates[2].variables, "{}");
        assert_eq!(graphql_query_templates(r#"{"data":{"ok":true}}"#), None);
    }

    #[test]
    fn builds_websocket_log_entries_and_response_text() {
        let exchange = client::WebSocketExchange {
            url: "ws://localhost/socket".to_string(),
            sent: "hello".to_string(),
            received: vec![
                client::WebSocketMessage {
                    kind: client::WebSocketMessageKind::Text,
                    data: "echo:hello".to_string(),
                },
                client::WebSocketMessage {
                    kind: client::WebSocketMessageKind::Pong,
                    data: "0 bytes".to_string(),
                },
            ],
            elapsed_ms: 12,
        };

        let entries = websocket_log_entries(&exchange);
        assert_eq!(
            entries,
            vec![
                WebSocketLogEntry {
                    direction: WebSocketDirection::Sent,
                    kind: "text".to_string(),
                    data: "hello".to_string(),
                },
                WebSocketLogEntry {
                    direction: WebSocketDirection::Received,
                    kind: "text".to_string(),
                    data: "echo:hello".to_string(),
                },
                WebSocketLogEntry {
                    direction: WebSocketDirection::Received,
                    kind: "pong".to_string(),
                    data: "0 bytes".to_string(),
                },
            ]
        );

        let text = websocket_exchange_text(&exchange);
        assert!(text.contains("URL ws://localhost/socket"));
        assert!(text.contains("RX 2"));
        assert!(text.contains("TX text hello"));
        assert!(text.contains("RX text echo:hello"));
        assert!(text.contains("RX pong 0 bytes"));
        assert!(!text.contains("elapsed:"));
        assert!(!text.contains("url:"));
        assert!(!text.contains("received:"));
        assert!(!text.contains("sent text:"));
        assert_eq!(
            format_websocket_log(&entries),
            "TX text hello\nRX text echo:hello\nRX pong 0 bytes"
        );
    }

    #[test]
    fn parses_websocket_binary_hex_input() {
        assert_eq!(
            websocket_hex_bytes("00 ff 7A").expect("hex"),
            vec![0, 255, 122]
        );
        assert_eq!(
            websocket_hex_bytes("00-01_ff").expect("hex"),
            vec![0, 1, 255]
        );

        assert_eq!(
            websocket_hex_bytes("").expect_err("empty"),
            WEBSOCKET_BINARY_EMPTY_BODY
        );
        assert_eq!(
            websocket_hex_bytes("0").expect_err("odd"),
            WEBSOCKET_BINARY_ODD_DIGITS_BODY
        );
        assert_eq!(
            websocket_hex_bytes("zz").expect_err("invalid"),
            "Bad hex at 0."
        );
    }

    #[test]
    fn websocket_send_requires_running_and_valid_message_mode_input() {
        let websocket_response_titles = [
            WEBSOCKET_ACTIVE_TITLE,
            WEBSOCKET_CONNECTED_TITLE,
            WEBSOCKET_MESSAGE_TITLE,
            WEBSOCKET_CLOSED_TITLE,
            WEBSOCKET_BINARY_INVALID_TITLE,
            WEBSOCKET_SEND_FAILED_TITLE,
            WEBSOCKET_FAILED_TITLE,
        ];
        assert_eq!(
            websocket_response_titles,
            [
                "WS active",
                "WS open",
                "WS msg",
                "WS closed",
                "WS invalid",
                "WS send fail",
                "WS failed"
            ]
        );
        assert!(
            websocket_response_titles
                .iter()
                .all(|title| title.len() <= 12 && !title.contains("WebSocket"))
        );
        assert_eq!(WEBSOCKET_NOT_OPEN_TITLE, "WS not open");
        assert_eq!(WEBSOCKET_NOT_OPEN_BODY, "No active session.");
        assert!(
            [
                WEBSOCKET_NOT_OPEN_BODY,
                WEBSOCKET_BINARY_EMPTY_BODY,
                WEBSOCKET_BINARY_ODD_DIGITS_BODY
            ]
            .iter()
            .all(|label| label.len() <= 18
                && !label.contains("Enter ")
                && !label.contains(" before ")
                && !label.contains("for example"))
        );
        assert!(!can_send_websocket_message(
            false,
            false,
            WebSocketMessageMode::Text,
            "hello"
        ));
        assert!(can_send_websocket_message(
            false,
            true,
            WebSocketMessageMode::Text,
            ""
        ));
        assert!(can_send_websocket_message(
            false,
            true,
            WebSocketMessageMode::BinaryHex,
            "00 ff 7a"
        ));
        assert!(!can_send_websocket_message(
            false,
            true,
            WebSocketMessageMode::BinaryHex,
            ""
        ));
        assert!(!can_send_websocket_message(
            false,
            true,
            WebSocketMessageMode::BinaryHex,
            "0"
        ));
        assert!(!can_send_websocket_message(
            false,
            true,
            WebSocketMessageMode::BinaryHex,
            "zz"
        ));
        assert!(!can_send_websocket_message(
            true,
            true,
            WebSocketMessageMode::Text,
            "hello"
        ));
    }

    #[test]
    fn websocket_connect_requires_valid_scheme_and_idle_state() {
        assert_eq!(WEBSOCKET_URL_REQUIRED_TITLE, "WS no URL");
        assert_eq!(WEBSOCKET_URL_INVALID_TITLE, "Bad WS URL");
        assert_eq!(WEBSOCKET_URL_INVALID_BODY, "Expected WS(S).");
        assert_eq!(REALTIME_STATUS_NO_URL, "No URL");
        assert_eq!(REALTIME_STATUS_BAD_URL, "Bad URL");
        assert_eq!(
            websocket_url_validation_response(""),
            (WEBSOCKET_URL_REQUIRED_TITLE, URL_REQUIRED_BODY)
        );
        assert_eq!(
            websocket_url_validation_response("http://localhost/socket"),
            (WEBSOCKET_URL_INVALID_TITLE, WEBSOCKET_URL_INVALID_BODY)
        );
        assert!(
            [
                WEBSOCKET_URL_REQUIRED_TITLE,
                WEBSOCKET_URL_INVALID_TITLE,
                URL_REQUIRED_BODY,
                WEBSOCKET_URL_INVALID_BODY
            ]
            .iter()
            .all(|label| label.len() <= 15
                && !label.contains("Enter ")
                && !label.contains(" before ")
                && !label.contains("for example")
                && !label.contains("WebSocket")
                && !label.contains("needs")
                && !label.contains("invalid"))
        );
        assert!(!can_connect_websocket(false, false, ""));
        assert!(!can_connect_websocket(false, false, "   "));
        assert!(!can_connect_websocket(
            false,
            false,
            "http://localhost/socket"
        ));
        assert!(!can_connect_websocket(
            false,
            false,
            "https://localhost/socket"
        ));
        assert!(can_connect_websocket(false, false, "ws://localhost/socket"));
        assert!(can_connect_websocket(
            false,
            false,
            " wss://localhost/socket "
        ));
        assert!(!can_connect_websocket(false, true, "ws://localhost/socket"));
        assert!(!can_connect_websocket(true, false, "ws://localhost/socket"));
        assert!(can_close_websocket(false, true));
        assert!(!can_close_websocket(false, false));
        assert!(!can_close_websocket(true, true));
    }

    #[test]
    fn realtime_log_actions_require_entries() {
        assert!(!can_use_realtime_log_actions(false, 0));
        assert!(can_use_realtime_log_actions(false, 1));
        assert!(can_use_realtime_log_actions(false, 24));
        assert!(!can_use_realtime_log_actions(true, 1));
        assert_eq!(REALTIME_LOG_EMPTY_TITLE, "No log");
        assert_eq!(REALTIME_LOG_COPIED_TITLE, "Copied");
        assert_eq!(REALTIME_LOG_CLEARED_TITLE, "Cleared");
        assert_eq!(REALTIME_LOG_BODY, "Log.");
        assert!(
            [
                REALTIME_LOG_EMPTY_TITLE,
                REALTIME_LOG_COPIED_TITLE,
                REALTIME_LOG_CLEARED_TITLE,
                REALTIME_LOG_BODY
            ]
            .iter()
            .all(|label| label.len() <= 7
                && !label.contains("WebSocket")
                && !label.contains("SSE")
                && !label.contains("messages")
                && !label.contains("events")
                && !label.contains("Log copied")
                && !label.contains("Log cleared")
                && !label.contains("Copied."))
        );
    }

    #[test]
    fn parses_websocket_subprotocol_list() {
        assert_eq!(
            websocket_protocol_list("chat, superchat, , json.v2 "),
            vec![
                "chat".to_string(),
                "superchat".to_string(),
                "json.v2".to_string()
            ]
        );
        assert!(websocket_protocol_list(" , ").is_empty());
    }

    #[test]
    fn builds_sse_log_entries_and_response_text() {
        let exchange = client::SseExchange {
            url: "http://localhost/events".to_string(),
            events: vec![
                client::SseEvent {
                    event: Some("ready".to_string()),
                    data: "connected".to_string(),
                    id: Some("1".to_string()),
                    retry: Some(3000),
                },
                client::SseEvent {
                    event: None,
                    data: "plain".to_string(),
                    id: None,
                    retry: None,
                },
            ],
            elapsed_ms: 9,
        };

        assert_eq!(
            sse_log_entries(&exchange),
            vec![
                SseLogEntry {
                    event: "ready".to_string(),
                    data: "connected".to_string(),
                    id: Some("1".to_string()),
                },
                SseLogEntry {
                    event: "message".to_string(),
                    data: "plain".to_string(),
                    id: None,
                },
            ]
        );

        let text = sse_exchange_text(&exchange);
        assert!(text.contains("URL http://localhost/events"));
        assert!(text.contains("2 events"));
        assert!(text.contains("ready #1 connected r3000"));
        assert!(text.contains("message plain"));
        assert!(!text.contains("elapsed:"));
        assert!(!text.contains("url:"));
        assert!(!text.contains("[id "));
        assert!(!text.contains("[retry "));
        assert_eq!(
            format_sse_log(&sse_log_entries(&exchange)),
            "ready #1 connected\nmessage plain"
        );
        assert_eq!(sse_reconnect_status(3, 2500), "r3 2500ms");
        assert!(!sse_reconnect_status(3, 2500).contains(" in "));
        assert!(!sse_reconnect_status(3, 2500).contains("reconnect"));
        assert!(!sse_reconnect_status(3, 2500).contains("retry "));
    }

    #[test]
    fn sse_start_requires_http_scheme_and_idle_state() {
        let sse_response_titles = [
            SSE_ACTIVE_TITLE,
            SSE_OK_TITLE,
            SSE_SUBSCRIBING_TITLE,
            SSE_STOPPED_TITLE,
            SSE_SUBSCRIBED_TITLE,
            SSE_EVENT_TITLE,
            SSE_RETRY_TITLE,
            SSE_CLOSED_TITLE,
            SSE_FAILED_TITLE,
        ];
        assert_eq!(
            sse_response_titles,
            [
                "SSE active",
                "SSE OK",
                "SSE sub",
                "Stopped",
                "SSE open",
                "SSE event",
                "SSE retry",
                "SSE closed",
                "SSE failed"
            ]
        );
        assert!(sse_response_titles.iter().all(|title| title.len() <= 10
            && !title.contains("subscribing")
            && !title.contains("subscribed")));
        assert_eq!(SSE_STOPPED_BODY, "SSE.");
        assert!(!SSE_STOPPED_BODY.contains("Stopped."));
        assert_eq!(SSE_URL_REQUIRED_TITLE, "SSE no URL");
        assert_eq!(SSE_URL_INVALID_TITLE, "Bad SSE URL");
        assert_eq!(SSE_URL_INVALID_BODY, "Expected HTTP(S).");
        assert_eq!(REALTIME_STATUS_NO_URL, "No URL");
        assert_eq!(REALTIME_STATUS_BAD_URL, "Bad URL");
        assert_eq!(
            sse_url_validation_response(""),
            (SSE_URL_REQUIRED_TITLE, URL_REQUIRED_BODY)
        );
        assert_eq!(
            sse_url_validation_response("ws://localhost/events"),
            (SSE_URL_INVALID_TITLE, SSE_URL_INVALID_BODY)
        );
        assert!(
            [
                SSE_URL_REQUIRED_TITLE,
                SSE_URL_INVALID_TITLE,
                URL_REQUIRED_BODY,
                SSE_URL_INVALID_BODY
            ]
            .iter()
            .all(|label| label.len() <= 17
                && !label.contains("Enter ")
                && !label.contains(" before ")
                && !label.contains("for example")
                && !label.contains("http://")
                && !label.contains("https://")
                && !label.contains("needs")
                && !label.contains("invalid"))
        );
        assert!(!can_start_sse(false, false, ""));
        assert!(!can_start_sse(false, false, "   "));
        assert!(!can_start_sse(false, false, "ws://localhost/events"));
        assert!(!can_start_sse(false, false, "ftp://localhost/events"));
        assert!(can_start_sse(false, false, "http://localhost/events"));
        assert!(can_start_sse(false, false, " https://localhost/events "));
        assert!(!can_start_sse(false, true, "http://localhost/events"));
        assert!(!can_start_sse(true, false, "http://localhost/events"));
        assert!(can_stop_sse_subscription(false, true));
        assert!(!can_stop_sse_subscription(false, false));
        assert!(!can_stop_sse_subscription(true, true));
    }

    #[test]
    fn resolves_variables_inside_graphql_body_json() {
        let store =
            variable_store_from_pairs(vec![("id".to_string(), "42".to_string())], None, Vec::new());
        let body = RequestBody::Raw {
            content_type: Some("application/json".to_string()),
            body: graphql_body(
                "query User { user(id: \"{{id}}\") { name } }",
                r#"{"id":"{{id}}"}"#,
            ),
        };

        let resolved = resolve_request_body(body, &store, None).expect("graphql variables");

        let RequestBody::Raw { body, .. } = resolved else {
            panic!("expected raw GraphQL payload body");
        };
        let value = serde_json::from_str::<serde_json::Value>(&body).expect("graphql json");

        assert_eq!(value["query"], "query User { user(id: \"42\") { name } }");
        assert_eq!(value["variables"]["id"], "42");
    }

    #[test]
    fn builds_history_request_summaries_from_body_modes() {
        let raw = history_request_from_body(
            "POST",
            "https://api.example.com/users",
            &RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            },
        );
        assert_eq!(raw.method, "POST");
        assert_eq!(raw.body_kind, "raw");
        assert_eq!(raw.body_preview, "{\"name\":\"Zen\"}");

        let form = history_request_from_body(
            "POST",
            "https://api.example.com/login",
            &RequestBody::FormUrlEncoded(vec![("username".to_string(), "dev".to_string())]),
        );
        assert_eq!(form.body_kind, "x-www-form-urlencoded");
        assert_eq!(form.body_preview, "username=dev");

        let binary = history_request_from_body(
            "POST",
            "https://api.example.com/upload",
            &RequestBody::BinaryFile {
                path: "/tmp/upload.bin".to_string(),
                content_type: Some("application/octet-stream".to_string()),
            },
        );
        assert_eq!(binary.body_kind, "binary");
        assert_eq!(binary.body_preview, "/tmp/upload.bin");
    }

    #[derive(Debug, PartialEq, Eq)]
    struct VisibleHistoryRow {
        id: u64,
        method: String,
        url: String,
        status: String,
    }

    fn visible_history_rows(history: &RequestHistory, query: &str) -> Vec<VisibleHistoryRow> {
        history
            .filtered(query)
            .into_iter()
            .map(|entry| VisibleHistoryRow {
                id: entry.id,
                method: entry.request.method.clone(),
                url: entry.request.url.clone(),
                status: entry.response.status.clone(),
            })
            .collect()
    }

    #[test]
    fn history_sidebar_visible_rows_follow_filter_without_display_limit() {
        let mut history = RequestHistory::new();
        for index in 0..10 {
            history.record_at(
                index,
                HistoryRequest {
                    method: if index % 2 == 0 { "GET" } else { "POST" }.to_string(),
                    url: format!("https://api.example.com/items/{index}"),
                    body_kind: "none".to_string(),
                    body_preview: String::new(),
                },
                HistoryResponse {
                    status: if index == 9 { "HTTP 201" } else { "HTTP 200" }.to_string(),
                    meta: "2/2 tests".to_string(),
                    body_preview: "{}".to_string(),
                },
            );
        }

        let rows = visible_history_rows(&history, "");
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].url, "https://api.example.com/items/9");
        assert_eq!(rows[0].status, "HTTP 201");

        let filtered = visible_history_rows(&history, "items/4");
        assert_eq!(
            filtered,
            vec![VisibleHistoryRow {
                id: 4,
                method: "GET".to_string(),
                url: "https://api.example.com/items/4".to_string(),
                status: "HTTP 200".to_string(),
            }]
        );

        assert!(visible_history_rows(&history, "missing").is_empty());
    }

    #[test]
    fn converts_codegen_request_to_collection_request() {
        let request = CodegenRequest {
            method: "POST".to_string(),
            url: "https://api.example.com/users?debug=true".to_string(),
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            query_params: vec![("debug".to_string(), "true".to_string())],
            body: RequestBody::Raw {
                content_type: Some("application/json".to_string()),
                body: "{\"name\":\"Zen\"}".to_string(),
            },
        };

        let collection_request = collection_request_from_codegen(&request);

        assert_eq!(collection_request.name, "POST users");
        assert_eq!(collection_request.method, "POST");
        assert_eq!(collection_request.headers[0].name, "Content-Type");
        assert_eq!(collection_request.query_params[0].value, "true");
        assert!(matches!(
            collection_request.body,
            CollectionBody::Raw {
                ref content_type,
                ref body
            } if content_type == "application/json" && body == "{\"name\":\"Zen\"}"
        ));
    }

    #[test]
    fn codegen_copy_requires_request_url() {
        let mut request = CodegenRequest {
            method: "GET".to_string(),
            url: String::new(),
            headers: Vec::new(),
            query_params: Vec::new(),
            body: RequestBody::None,
        };

        assert!(!can_copy_codegen_request(&request));
        request.url = "   ".to_string();
        assert!(!can_copy_codegen_request(&request));
        request.url = "https://api.example.com/users".to_string();
        assert!(can_copy_codegen_request(&request));
        assert!(!can_copy_codegen_snippet(false, false));
        assert!(can_copy_codegen_snippet(false, true));
        assert!(!can_copy_codegen_snippet(true, true));
    }

    #[test]
    fn codegen_language_selector_respects_busy_state() {
        assert!(can_use_codegen_language_selector(false));
        assert!(!can_use_codegen_language_selector(true));
    }

    #[test]
    fn codegen_language_labels_stay_compact() {
        let labels = [
            snippet_language_label(SnippetLanguage::Curl),
            snippet_language_label(SnippetLanguage::PythonRequests),
            snippet_language_label(SnippetLanguage::JavaScriptFetch),
            snippet_language_label(SnippetLanguage::RustReqwest),
            snippet_language_label(SnippetLanguage::GoNetHttp),
        ];

        assert_eq!(labels, ["cURL", "Py", "JS", "Rust", "Go"]);
        assert!(labels.iter().all(|label| label.len() <= 4));
        assert_eq!(CODEGEN_EMPTY_SNIPPET_LABEL, "No URL");
        assert!(!CODEGEN_EMPTY_SNIPPET_LABEL.contains("Enter"));
        assert!(CODEGEN_EMPTY_SNIPPET_LABEL.len() <= 6);
    }

    #[test]
    fn request_configuration_editing_respects_busy_state() {
        assert!(can_edit_request_configuration(false));
        assert!(!can_edit_request_configuration(true));
        assert!(can_select_request_method(false));
        assert!(!can_select_request_method(true));
        assert!(can_use_codegen_language_selector(false));
        assert!(!can_use_codegen_language_selector(true));
        assert!(can_restore_request_from_sidebar(false));
        assert!(!can_restore_request_from_sidebar(true));
        assert!(can_mutate_collection(false));
        assert!(!can_mutate_collection(true));
        assert!(can_open_collection_context_menu(false));
        assert!(!can_open_collection_context_menu(true));
        assert!(can_use_collection_context_action(false, true));
        assert!(!can_use_collection_context_action(false, false));
        assert!(!can_use_collection_context_action(true, true));
        assert!(!can_use_collection_context_action(true, false));
    }

    #[test]
    fn render_enabled_controls_recheck_current_busy_state() {
        assert!(can_activate_render_enabled_control(true, false));
        assert!(!can_activate_render_enabled_control(false, false));
        assert!(!can_activate_render_enabled_control(true, true));
        assert!(!can_activate_render_enabled_control(false, true));
    }

    #[test]
    fn collection_save_preserves_raw_request_and_pre_request_script() {
        let request = CodegenRequest {
            method: "GET".to_string(),
            url: "{{baseUrl}}/users".to_string(),
            headers: vec![("Accept".to_string(), "application/json".to_string())],
            query_params: Vec::new(),
            body: RequestBody::None,
        };
        let script =
            "set_method POST; set_header Authorization=Bearer {{token}}; set_query debug=true"
                .to_string();
        let tests = vec![ResponseAssertion {
            name: "status".to_string(),
            kind: ResponseAssertionKind::StatusEquals { status: 201 },
        }];

        let collection_request = collection_request_for_save(&request, script.clone(), tests);

        assert_eq!(collection_request.method, "GET");
        assert_eq!(collection_request.url, "{{baseUrl}}/users");
        assert_eq!(collection_request.headers.len(), 1);
        assert_eq!(collection_request.pre_request_script, script);
        assert_eq!(collection_request.tests.len(), 1);
    }

    #[test]
    fn maps_collection_content_types_to_raw_body_format() {
        assert!(matches!(
            raw_format_from_content_type("application/vnd.api+json"),
            RawBodyFormat::Json
        ));
        assert!(matches!(
            raw_format_from_content_type("application/xml"),
            RawBodyFormat::Xml
        ));
        assert!(matches!(
            raw_format_from_content_type("text/html; charset=utf-8"),
            RawBodyFormat::Html
        ));
        assert!(matches!(
            raw_format_from_content_type("text/plain"),
            RawBodyFormat::Text
        ));
    }

    #[test]
    fn highlights_json_raw_body_tokens() {
        let input = r#"{"name":"Zen","active":true,"count":42,"empty":null}"#;
        let highlights = syntax_highlights(input, RawBodyFormat::Json);

        assert!(highlights.contains(&(0..1, SyntaxTokenKind::Punctuation)));
        assert!(highlights.contains(&(1..7, SyntaxTokenKind::String)));
        assert!(highlights.contains(&(23..27, SyntaxTokenKind::Keyword)));
        assert!(highlights.contains(&(36..38, SyntaxTokenKind::Number)));
        assert!(highlights.contains(&(47..51, SyntaxTokenKind::Keyword)));
    }

    #[test]
    fn highlights_markup_raw_body_tokens() {
        let input = r#"<user id="42" active='true'>Zen</user>"#;
        let highlights = syntax_highlights(input, RawBodyFormat::Html);

        assert!(highlights.contains(&(0..1, SyntaxTokenKind::Punctuation)));
        assert!(highlights.contains(&(1..5, SyntaxTokenKind::Tag)));
        assert!(highlights.contains(&(6..8, SyntaxTokenKind::Attribute)));
        assert!(highlights.contains(&(9..13, SyntaxTokenKind::String)));
        assert!(highlights.contains(&(14..20, SyntaxTokenKind::Attribute)));
        assert!(highlights.contains(&(21..27, SyntaxTokenKind::String)));
        assert!(highlights.contains(&(33..37, SyntaxTokenKind::Tag)));
    }

    #[test]
    fn counts_collection_requests_recursively() {
        let items = vec![CollectionItem::Folder(CollectionFolder {
            name: "Users".to_string(),
            description: String::new(),
            items: vec![
                CollectionItem::Request(CollectionRequest {
                    name: "List".to_string(),
                    method: "GET".to_string(),
                    url: "https://api.example.com/users".to_string(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    body: CollectionBody::None,
                    pre_request_script: String::new(),
                    tests: Vec::new(),
                }),
                CollectionItem::Request(CollectionRequest {
                    name: "Create".to_string(),
                    method: "POST".to_string(),
                    url: "https://api.example.com/users".to_string(),
                    headers: Vec::new(),
                    query_params: Vec::new(),
                    body: CollectionBody::None,
                    pre_request_script: String::new(),
                    tests: Vec::new(),
                }),
            ],
        })];

        assert_eq!(collection_item_count(&items), 2);
    }

    #[test]
    fn collection_export_requires_at_least_one_request() {
        assert_eq!(COLLECTION_EXPORT_EMPTY_TITLE, "No export");
        assert_eq!(SAVED_REQUESTS_EMPTY_BODY, "No saved requests.");
        assert_eq!(RUNNER_EMPTY_TITLE, "No requests");
        assert_eq!(MOCK_ROUTES_REQUIRED_TITLE, "No routes");
        assert_eq!(MOCK_ROUTES_REQUIRED_BODY, "No routes loaded.");
        assert!(
            [
                COLLECTION_EXPORT_EMPTY_TITLE,
                SAVED_REQUESTS_EMPTY_BODY,
                RUNNER_EMPTY_TITLE,
                MOCK_ROUTES_REQUIRED_TITLE,
                MOCK_ROUTES_REQUIRED_BODY
            ]
            .iter()
            .all(|label| !label.contains("Enter ")
                && !label.contains(" before ")
                && !label.contains("Import ")
                && !label.contains("needs")
                && !label.contains("empty"))
        );
        let empty = ApiCollection::new("Empty");
        assert!(!can_export_collection(&empty));

        let folders_only = ApiCollection {
            name: "Folders".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: Vec::new(),
            })],
        };
        assert!(!can_export_collection(&folders_only));

        let with_request = ApiCollection {
            name: "Requests".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Request(CollectionRequest {
                name: "List".to_string(),
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: CollectionBody::None,
                pre_request_script: String::new(),
                tests: Vec::new(),
            })],
        };
        assert!(can_export_collection(&with_request));
    }

    #[test]
    fn parses_collection_node_ids() {
        assert_eq!(collection_node_indices("collection"), Some(Vec::new()));
        assert_eq!(collection_node_indices("collection/0/2"), Some(vec![0, 2]));
        assert_eq!(collection_node_indices("routes/0"), None);
    }

    #[test]
    fn mutates_collection_items_for_context_menu_actions() {
        let mut collection = ApiCollection {
            name: "Demo".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: Vec::new(),
            })],
        };

        assert!(insert_collection_item(
            &mut collection.items,
            "collection/0",
            CollectionItem::Request(blank_collection_request())
        ));
        assert_eq!(collection_item_count(&collection.items), 1);

        assert!(rename_collection_node(
            &mut collection,
            "collection/0/0",
            "List users"
        ));
        let CollectionItem::Folder(folder) = &collection.items[0] else {
            panic!("expected folder");
        };
        let CollectionItem::Request(request) = &folder.items[0] else {
            panic!("expected request");
        };
        assert_eq!(request.name, "List users");

        assert!(duplicate_collection_item(
            &mut collection.items,
            "collection/0/0"
        ));
        let CollectionItem::Folder(folder) = &collection.items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.items.len(), 2);

        assert!(remove_collection_item(&mut collection.items, "collection/0/1").is_some());
        assert_eq!(collection_item_count(&collection.items), 1);
    }

    #[test]
    fn moves_collection_items_for_drag_and_drop() {
        let mut items = vec![
            CollectionItem::Folder(CollectionFolder {
                name: "Users".to_string(),
                description: String::new(),
                items: Vec::new(),
            }),
            CollectionItem::Request(CollectionRequest {
                name: "List users".to_string(),
                method: "GET".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: CollectionBody::None,
                pre_request_script: String::new(),
                tests: Vec::new(),
            }),
            CollectionItem::Request(CollectionRequest {
                name: "Create user".to_string(),
                method: "POST".to_string(),
                url: "https://api.example.com/users".to_string(),
                headers: Vec::new(),
                query_params: Vec::new(),
                body: CollectionBody::None,
                pre_request_script: String::new(),
                tests: Vec::new(),
            }),
        ];

        assert!(move_collection_item(
            &mut items,
            "collection/1",
            "collection/0"
        ));
        let CollectionItem::Folder(folder) = &items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.items.len(), 1);

        assert!(move_collection_item(
            &mut items,
            "collection/1",
            "collection/0/0"
        ));
        let CollectionItem::Folder(folder) = &items[0] else {
            panic!("expected folder");
        };
        assert_eq!(folder.items.len(), 2);
        let CollectionItem::Request(request) = &folder.items[1] else {
            panic!("expected request");
        };
        assert_eq!(request.name, "Create user");
    }

    #[test]
    fn rejects_moving_collection_folder_into_itself() {
        let mut items = vec![CollectionItem::Folder(CollectionFolder {
            name: "Users".to_string(),
            description: String::new(),
            items: vec![CollectionItem::Folder(CollectionFolder {
                name: "Nested".to_string(),
                description: String::new(),
                items: Vec::new(),
            })],
        })];

        assert!(!move_collection_item(
            &mut items,
            "collection/0",
            "collection/0/0"
        ));
        assert_eq!(collection_item_count(&items), 0);
    }

    #[test]
    fn ui_metrics_keep_collection_tree_and_editors_aligned() {
        assert_eq!(APP_WINDOW_WIDTH, 1180.);
        assert_eq!(APP_WINDOW_HEIGHT, 760.);
        assert_eq!(
            WORKSPACE_SIDEBAR_DEFAULT_RATIO,
            SIDEBAR_WIDTH / APP_WINDOW_WIDTH
        );
        assert_eq!(WORKSPACE_SIDEBAR_MIN_RATIO, 0.24);
        assert_eq!(WORKSPACE_SIDEBAR_MAX_RATIO, 0.38);
        assert_eq!(WORKSPACE_REQUEST_DEFAULT_RATIO, 0.37);
        assert_eq!(WORKSPACE_REQUEST_MIN_RATIO, 0.32);
        assert_eq!(WORKSPACE_REQUEST_MAX_RATIO, 0.56);
        assert_eq!(WORKSPACE_RESPONSE_MIN_RATIO, 0.24);
        assert_eq!(WORKSPACE_SPLIT_HANDLE_WIDTH, 8.);
        assert_eq!(WORKSPACE_SPLIT_DIVIDER_WIDTH, 1.);
        assert_eq!(WORKSPACE_SPLIT_RATIO_EPSILON, 0.001);
        assert_eq!(WORKSPACE_SPLIT_UPDATE_STEP_PX, 48.);
        assert_eq!(SIDEBAR_NAV_HEIGHT, 42.);
        assert_eq!(LAYOUT_ZERO, 0.);
        assert_eq!(SCROLLBAR_HIDDEN_SIZE, LAYOUT_ZERO);
        assert_eq!(SCROLLBAR_WIDTH, 6.);
        assert_eq!(SCROLLBAR_RIGHT_OFFSET, 3.);
        assert_eq!(
            SCROLLBAR_GUTTER_WIDTH,
            SCROLLBAR_WIDTH + SCROLLBAR_RIGHT_OFFSET * 2.
        );
        assert_eq!(SCROLLBAR_CONTENT_RIGHT_PADDING, SCROLLBAR_GUTTER_WIDTH + 8.);
        assert_eq!(SCROLLBAR_MIN_THUMB_HEIGHT, 28.);
        assert_eq!(REQUEST_EDITOR_TAB_BAR_HEIGHT, 34.);
        assert_eq!(REQUEST_EDITOR_TAB_COUNT, 7);
        assert_eq!(
            request_editor_tabs().map(|(label, _)| label),
            ["Params", "Hdrs", "Auth", "Body", "Script", "Live", "Tools"]
        );
        assert_eq!(
            request_tab_shortcuts().map(|(shortcut, _)| shortcut),
            [1, 2, 3, 4, 5, 6, 7]
        );
        assert_eq!(
            request_tab_shortcuts().map(|(_, tab)| tab),
            [
                RequestPaneTab::Params,
                RequestPaneTab::Headers,
                RequestPaneTab::Auth,
                RequestPaneTab::Body,
                RequestPaneTab::Scripts,
                RequestPaneTab::Realtime,
                RequestPaneTab::Tools
            ]
        );
        assert!(WORKSPACE_SIDEBAR_MIN_RATIO < WORKSPACE_SIDEBAR_DEFAULT_RATIO);
        assert!(WORKSPACE_SIDEBAR_DEFAULT_RATIO < WORKSPACE_SIDEBAR_MAX_RATIO);
        assert!(WORKSPACE_REQUEST_MIN_RATIO < WORKSPACE_REQUEST_DEFAULT_RATIO);
        assert!(WORKSPACE_REQUEST_DEFAULT_RATIO < WORKSPACE_REQUEST_MAX_RATIO);
        assert!(
            WORKSPACE_SIDEBAR_DEFAULT_RATIO
                + WORKSPACE_REQUEST_DEFAULT_RATIO
                + WORKSPACE_RESPONSE_MIN_RATIO
                < 1.
        );
        assert_eq!(TOP_BAR_HEIGHT, 40.);
        assert_eq!(TOP_BAR_BRAND_WIDTH, 72.);
        assert_eq!(TOP_BAR_ACTION_WIDTH, 76.);
        assert_eq!(TOP_BAR_MOCK_ACTION_WIDTH, 68.);
        assert_eq!(TOP_BAR_ACTION_HEIGHT, 30.);
        assert_eq!(STATUS_BAR_HEIGHT, 32.);
        assert_eq!(DISABLED_CONTROL_OPACITY, 0.78);
        assert_eq!(IMPORT_POPOVER_WIDTH, 520.);
        assert_eq!(IMPORT_POPOVER_HEIGHT, 58.);
        assert!(IMPORT_POPOVER_WIDTH > ACTION_BUTTON_WIDTH + IMPORT_POPOVER_PADDING * 2.);
        assert_eq!(SIDEBAR_WIDTH, 320.);
        assert_eq!(REQUEST_BAR_HEIGHT, 54.);
        assert_eq!(REQUEST_BAR_CONTROL_Y_OFFSET, 8.);
        assert_eq!(REQUEST_METHOD_SEGMENT_WIDTH, 100.);
        assert_eq!(REQUEST_ADDRESS_RADIUS, TEXT_INPUT_RADIUS);
        assert_eq!(
            METHOD_MENU_TOP_OFFSET,
            PANEL_HEADER_HEIGHT + REQUEST_BAR_CONTROL_Y_OFFSET + TEXT_INPUT_HEIGHT + 4.
        );
        assert_eq!(METHOD_MENU_WIDTH, REQUEST_METHOD_SEGMENT_WIDTH);
        assert_eq!(METHOD_MENU_ITEM_HEIGHT, 30.);
        assert_eq!(REQUEST_SEND_WIDTH, 86.);
        assert_eq!(TEXT_INPUT_HEIGHT, 40.);
        assert_eq!(TEXT_INPUT_LINE_HEIGHT, 24.);
        assert_eq!(UI_RADIUS_TIGHT, 4.);
        assert_eq!(UI_RADIUS_CONTROL, 5.);
        assert_eq!(UI_RADIUS_INPUT, 6.);
        assert_eq!(TEXT_INPUT_RADIUS, UI_RADIUS_INPUT);
        assert_eq!(TEXT_INPUT_BORDER_WIDTH, 2.);
        assert_eq!(ROUTE_ROW_HEIGHT, 48.);
        assert_eq!(HISTORY_ROW_HEIGHT, 46.);
        assert_eq!(ROUTE_SELECTED_MARKER_WIDTH, 3.);
        assert_eq!(REQUEST_ADDRESS_DIVIDER_HEIGHT, 22.);
        assert_eq!(REQUEST_ADDRESS_DIVIDER_WIDTH, 1.);
        assert_eq!(RESPONSE_TAB_BAR_HEIGHT, 36.);
        assert_eq!(RESPONSE_TAB_COUNT, 3);
        assert_eq!(
            response_tabs().map(|(label, _)| label),
            ["Pretty", "Raw", "Hdrs"]
        );
        assert_eq!(
            response_tab_shortcuts().map(|(shortcut, _)| shortcut),
            [1, 2, 3]
        );
        assert_eq!(
            response_tab_shortcuts().map(|(_, view)| view),
            [
                ResponseView::Pretty,
                ResponseView::Raw,
                ResponseView::Headers
            ]
        );
        assert_eq!(RESPONSE_TAB_WIDTH, 72.);
        assert_eq!(RESPONSE_FOLD_BUTTON_WIDTH, 54.);
        assert_eq!(RESPONSE_COPY_BUTTON_WIDTH, 54.);
        assert_eq!(STATUS_BAR_TRAILING_MAX_WIDTH, 220.);
        assert!(STATUS_BAR_TRAILING_MAX_WIDTH < 360.);
        assert_eq!(SIDEBAR_BUTTON_HEIGHT, 28.);
        assert_eq!(SIDEBAR_SECTION_BUTTON_HEIGHT, 28.);
        assert_eq!(COMPACT_CONTROL_HEIGHT, 28.);
        assert_eq!(COMPACT_TOGGLE_SHORT_WIDTH, 76.);
        assert_eq!(COMPACT_TOGGLE_LONG_WIDTH, 156.);
        assert_eq!(COMPACT_TOGGLE_LONG_LABEL_THRESHOLD, 12);
        assert_eq!(compact_toggle_width("Short"), COMPACT_TOGGLE_SHORT_WIDTH);
        assert_eq!(
            compact_toggle_width("application/json"),
            COMPACT_TOGGLE_LONG_WIDTH
        );
        assert_eq!(BODY_EDITOR_HEIGHT, 118.);
        assert_eq!(GRAPHQL_VARIABLES_EDITOR_HEIGHT, 86.);
        assert_eq!(BODY_PREVIEW_HEIGHT, 86.);
        assert_eq!(BODY_PREVIEW_LINE_HEIGHT, 26.);
        assert_eq!(GRAPHQL_QUERY_TEMPLATE_LIMIT, 5);
        assert_eq!(GRAPHQL_QUERY_TEMPLATE_USE_BUTTON_WIDTH, 54.);
        assert_eq!(GRAPHQL_SCHEMA_BROWSER_HEIGHT, 112.);
        assert_eq!(CODEGEN_SNIPPET_HEIGHT, 180.);
        assert_eq!(CODEGEN_SNIPPET_LINE_HEIGHT, 26.);
        assert_eq!(RESPONSE_BODY_LINE_HEIGHT, 26.);
        assert_eq!(RESULT_ROW_HEIGHT, 34.);
        assert_eq!(COLLECTION_DRAG_PREVIEW_HEIGHT, 28.);
        assert_eq!(COLLECTION_DRAG_PREVIEW_MAX_WIDTH, 220.);
        assert_eq!(PANEL_HEADER_HEIGHT, 40.);
        assert_eq!(PANEL_HEADER_META_MAX_WIDTH, 180.);
        assert!(PANEL_HEADER_META_MAX_WIDTH < 260.);
        assert_eq!(PANEL_HEADER_RIGHT_PADDING, 14.);
        assert_eq!(PANEL_HEADER_UNDERLINE_HEIGHT, 2.);
        assert_eq!(PANEL_HEADER_UNDERLINE_LEFT_OFFSET, 12.);
        assert_eq!(EMPTY_STATE_ROW_HEIGHT, 36.);
        assert_eq!(APP_BASE_TEXT_SIZE, 14.);
        assert_eq!(TOP_BAR_BRAND_TEXT_SIZE, 16.);
        assert_eq!(TOP_BAR_ACTION_TEXT_SIZE, 14.);
        assert_eq!(ACTION_BUTTON_TEXT_SIZE, 16.);
        assert_eq!(COMPACT_CONTROL_TEXT_SIZE, 15.);
        assert_eq!(COMPACT_SYMBOL_TEXT_SIZE, 13.);
        assert_eq!(METHOD_CHEVRON_TEXT_SIZE, 12.);
        assert_eq!(PANE_HEADER_TITLE_TEXT_SIZE, 19.);
        assert_eq!(SIDEBAR_NAV_TEXT_SIZE, 16.);
        assert_eq!(SIDEBAR_ACTION_TEXT_SIZE, 15.);
        assert_eq!(SIDEBAR_PRIMARY_ROW_TEXT_SIZE, 17.);
        assert_eq!(SIDEBAR_METHOD_TEXT_SIZE, 16.);
        assert_eq!(SIDEBAR_COMPACT_METHOD_TEXT_SIZE, 14.);
        assert_eq!(ROW_META_TEXT_SIZE, 15.);
        assert_eq!(REQUEST_PRIMARY_CONTROL_TEXT_SIZE, 18.);
        assert_eq!(REQUEST_EDITOR_TAB_TEXT_SIZE, 16.);
        assert_eq!(PANEL_TITLE_TEXT_SIZE, 18.);
        assert_eq!(PANEL_CONTENT_TEXT_SIZE, 18.);
        assert_eq!(PANEL_META_TEXT_SIZE, 15.);
        assert_eq!(TABLE_HEADER_TEXT_SIZE, 16.);
        assert_eq!(TEXT_INPUT_TEXT_SIZE, 18.);
        assert_eq!(RESPONSE_BODY_TEXT_SIZE, 18.);
        assert_eq!(collection_tree_indent(0), 8.);
        assert_eq!(collection_tree_indent(1), 22.);
        assert_eq!(collection_tree_indent(2), 36.);
        assert_eq!(COLLECTION_TREE_INDENT_MAX, 78.);
        assert_eq!(collection_tree_indent(5), COLLECTION_TREE_INDENT_MAX);
        assert_eq!(collection_tree_indent(24), COLLECTION_TREE_INDENT_MAX);
        assert_eq!(COLLECTION_TREE_ROOT_ROW_HEIGHT, 32.);
        assert_eq!(COLLECTION_TREE_FOLDER_ROW_HEIGHT, 32.);
        assert_eq!(COLLECTION_TREE_REQUEST_ROW_HEIGHT, 40.);
        assert_eq!(
            collection_tree_row_height(CollectionNodeKind::Root),
            collection_tree_row_height(CollectionNodeKind::Folder)
        );
        assert!(
            collection_tree_row_height(CollectionNodeKind::Request)
                > collection_tree_row_height(CollectionNodeKind::Folder)
        );
        assert_eq!(COLLECTION_TREE_MARKER_WIDTH, 14.);
        assert_eq!(HTTP_METHOD_LABEL_WIDTH, 64.);
        assert_eq!(SIDEBAR_SECONDARY_ROW_INDENT, HTTP_METHOD_LABEL_WIDTH + 8.);
        assert_eq!(RUNNER_METHOD_COLUMN_WIDTH, 70.);
        assert_eq!(RUNNER_STATUS_COLUMN_WIDTH, 42.);
        assert_eq!(RUNNER_PRE_REQUEST_COLUMN_WIDTH, 52.);
        assert_eq!(RUNNER_TESTS_COLUMN_WIDTH, 64.);
        assert_eq!(TEST_RESULT_STATUS_COLUMN_WIDTH, 48.);
        assert_eq!(TEST_RESULT_NAME_COLUMN_WIDTH, 140.);
        assert_eq!(WEBSOCKET_DIRECTION_COLUMN_WIDTH, 52.);
        assert_eq!(WEBSOCKET_KIND_COLUMN_WIDTH, 52.);
        assert_eq!(SSE_EVENT_COLUMN_WIDTH, 74.);
        assert_eq!(SSE_ID_COLUMN_WIDTH, 58.);
        assert_eq!(KEY_VALUE_KEY_COLUMN_WIDTH, 150.);
        assert_eq!(KEY_VALUE_EDITOR_KEY_COLUMN_WIDTH, 128.);
        assert_eq!(KEY_VALUE_EDITOR_COMPACT_KEY_COLUMN_WIDTH, 112.);
        assert_eq!(KEY_VALUE_ROW_ACTION_BUTTON_WIDTH, 28.);
        assert_eq!(TEST_ASSERTION_NAME_COLUMN_WIDTH, 132.);
        assert_eq!(TEST_ASSERTION_KIND_COLUMN_WIDTH, 96.);
        assert_eq!(TESTS_CLEAR_RESULTS_BUTTON_WIDTH, 54.);
        assert_eq!(UI_COLOR_SURFACE, UI_COLOR_SURFACE_MUTED);
        assert_ne!(UI_COLOR_APP_CHROME, UI_COLOR_SIDEBAR_PANE);
        assert_eq!(UI_COLOR_REQUEST_PANE, UI_COLOR_SURFACE);
        assert_eq!(UI_COLOR_REQUEST_TAB_BAR, UI_COLOR_SURFACE_MUTED);
        assert_eq!(UI_COLOR_RESPONSE_TAB_BAR, UI_COLOR_SURFACE_MUTED);
        assert_eq!(UI_COLOR_SIDEBAR_PANE, UI_COLOR_RESPONSE_PANE);
        assert_eq!(UI_COLOR_SIDEBAR_PANE, UI_COLOR_SURFACE);
        assert_eq!(UI_COLOR_REQUEST_PANE, UI_COLOR_SURFACE);
        assert_eq!(UI_COLOR_RESPONSE_PANE, UI_COLOR_SURFACE);
        assert_eq!(UI_COLOR_DISABLED_SURFACE, 0xf2f2f3);
        assert_eq!(UI_COLOR_DISABLED_BORDER, 0xd9d9df);
        assert_eq!(UI_COLOR_DISABLED_TEXT, 0x8a8f98);
        assert_eq!(UI_COLOR_BORDER_STRONG, 0xaeb7c2);
        assert_eq!(UI_COLOR_HOVER, 0xf4f4f5);
        assert_eq!(UI_COLOR_TEXT_SECONDARY, 0x4b5563);
        assert_eq!(UI_COLOR_TEXT_MUTED, 0x64748b);
        assert_eq!(UI_COLOR_TEXT_PLACEHOLDER, 0xb8c0cc);
        assert_eq!(UI_COLOR_TEXT_BODY, 0x1f2937);
        assert_eq!(UI_COLOR_SIDEBAR_DETAIL_TEXT, UI_COLOR_TEXT_BODY);
        assert_ne!(UI_COLOR_DISABLED_SURFACE, UI_COLOR_HOVER);
        assert_ne!(UI_COLOR_TEXT_PLACEHOLDER, UI_COLOR_TEXT_MUTED);
        assert_ne!(UI_COLOR_SIDEBAR_DETAIL_TEXT, UI_COLOR_TEXT_MUTED);
        assert_ne!(UI_COLOR_TEXT_MUTED, UI_COLOR_DISABLED_TEXT);
        assert_ne!(UI_COLOR_BORDER, UI_COLOR_BORDER_STRONG);
        assert_eq!(WORKSPACE_SPLIT_UPDATE_STEP_PX, 48.);
        assert_eq!(UI_COLOR_STATUS_SUCCESS, 0x059669);
        assert_eq!(UI_COLOR_STATUS_BUSY, 0xd97706);
        assert_eq!(UI_COLOR_STATUS_ERROR, 0xdc2626);
        assert_eq!(UI_COLOR_SYNTAX_KEYWORD, UI_COLOR_ACCENT);
        assert_eq!(UI_COLOR_SYNTAX_PUNCTUATION, UI_COLOR_TEXT_SECONDARY);
    }

    #[test]
    fn top_bar_labels_stay_compact() {
        let labels = [
            APP_BRAND_LABEL,
            TOP_BAR_IMPORT_LABEL,
            IMPORT_OPEN_LABEL,
            MOCK_START_LABEL,
            MOCK_STOP_LABEL,
        ];
        assert_eq!(labels, ["ZenAPI", "Import", "Open", "Mock", "Stop"]);
        assert!(labels.iter().all(|label| label.len() <= 6));
        assert_eq!(mock_button_label(false), MOCK_START_LABEL);
        assert_eq!(mock_button_label(true), MOCK_STOP_LABEL);
    }

    #[test]
    fn sidebar_labels_stay_compact() {
        let nav_labels = [
            SIDEBAR_ROUTES_LABEL,
            SIDEBAR_SAVED_LABEL,
            SIDEBAR_HISTORY_LABEL,
        ];
        assert_eq!(nav_labels, ["Routes", "Saved", "History"]);
        assert!(nav_labels.iter().all(|label| label.len() <= 7));

        let sidebar_placeholders = [
            PLACEHOLDER_COLLECTION_PATH,
            PLACEHOLDER_ROUTE_FILTER,
            PLACEHOLDER_HISTORY_FILTER,
        ];
        assert_eq!(sidebar_placeholders, ["JSON path", "Filter", "Filter"]);
        assert!(sidebar_placeholders.iter().all(|label| label.len() <= 9));

        let empty_labels = [
            SIDEBAR_EMPTY_ROUTES_LABEL,
            SIDEBAR_EMPTY_SAVED_LABEL,
            SIDEBAR_EMPTY_HISTORY_LABEL,
            SIDEBAR_EMPTY_MATCHES_LABEL,
        ];
        assert_eq!(
            empty_labels,
            ["No routes", "No saved", "No history", "No matches"]
        );
        assert!(empty_labels.iter().all(|label| label.len() <= 10));
        assert!(
            empty_labels
                .iter()
                .all(|label| !label.contains("collection")
                    && !label.contains("imported")
                    && !label.contains("matching"))
        );

        let collection_labels = [
            COLLECTION_IMPORT_LABEL,
            COLLECTION_SAVE_LABEL,
            COLLECTION_EXPORT_LABEL,
            COLLECTION_POSTMAN_LABEL,
            COLLECTION_MENU_CLOSE_LABEL,
            COLLECTION_MENU_NEW_REQUEST_LABEL,
            COLLECTION_MENU_NEW_FOLDER_LABEL,
            COLLECTION_MENU_COPY_LABEL,
            COLLECTION_MENU_DELETE_LABEL,
            COLLECTION_MENU_RENAME_LABEL,
        ];
        assert_eq!(
            collection_labels,
            [
                "Import", "Save", "Export", "PM", "x", "+ Req", "+ Dir", "Copy", "Del", "Rename"
            ]
        );
        assert!(collection_labels.iter().all(|label| label.len() <= 6));
        assert!(
            collection_labels
                .iter()
                .all(|label| !label.contains("Postman") && !label.contains("Delete"))
        );
    }

    #[test]
    fn collection_sidebar_status_labels_stay_compact() {
        assert_eq!(collection_sidebar_status_label(""), None);
        assert_eq!(collection_sidebar_status_label("No collection file"), None);
        let labels = [
            collection_sidebar_status_label("Imported Demo").unwrap(),
            collection_sidebar_status_label("Exported Postman").unwrap(),
            collection_sidebar_status_label("Request created: 4 requests").unwrap(),
            collection_sidebar_status_label("Folder created: 4 requests").unwrap(),
            collection_sidebar_status_label("Item copied: 4 requests").unwrap(),
            collection_sidebar_status_label("Item deleted: 4 requests").unwrap(),
            collection_sidebar_status_label("Item moved: 4 requests").unwrap(),
            collection_sidebar_status_label("Item renamed: 4 requests").unwrap(),
            collection_sidebar_status_label("Import failed").unwrap(),
            collection_sidebar_status_label("Export failed").unwrap(),
            collection_sidebar_status_label("Nothing to export").unwrap(),
            collection_sidebar_status_label("Rename unavailable while busy").unwrap(),
        ];

        assert_eq!(
            labels,
            [
                "Imported",
                "Exported",
                "+ Req",
                "+ Dir",
                "Copied",
                "Deleted",
                "Moved",
                "Renamed",
                "Import fail",
                "Export fail",
                "No export",
                "Busy"
            ]
        );
        assert!(labels.iter().all(|label| label.len() <= 11));
    }

    #[test]
    fn request_pane_labels_and_placeholders_stay_compact() {
        let placeholders = [
            PLACEHOLDER_IMPORT_PATH,
            PLACEHOLDER_COLLECTION_PATH,
            PLACEHOLDER_COLLECTION_ITEM,
            PLACEHOLDER_REQUEST_URL,
            PLACEHOLDER_ENVIRONMENT_NAME,
            PLACEHOLDER_BEARER_TOKEN,
            PLACEHOLDER_OAUTH2_ACCESS_TOKEN,
            PLACEHOLDER_BASIC_USERNAME,
            PLACEHOLDER_BASIC_PASSWORD,
            PLACEHOLDER_JWT_TOKEN,
            PLACEHOLDER_API_KEY_NAME,
            PLACEHOLDER_API_KEY_VALUE,
            PLACEHOLDER_REQUEST_BODY,
            PLACEHOLDER_GRAPHQL_QUERY,
            PLACEHOLDER_GRAPHQL_VARIABLES,
            PLACEHOLDER_BINARY_BODY_PATH,
        ];
        assert_eq!(
            placeholders,
            [
                "Spec path",
                "JSON path",
                "Item name",
                "URL",
                "Env",
                "Token",
                "Access token",
                "User",
                "Pass",
                "JWT",
                "X-API-Key",
                "Key value",
                "Body",
                "Query",
                "Vars",
                "File path"
            ]
        );
        assert!(placeholders.iter().all(|label| label.len() <= 16));

        let auth_labels = [
            AUTH_PANEL_TITLE,
            AUTH_NONE_LABEL,
            AUTH_BEARER_LABEL,
            AUTH_OAUTH_LABEL,
            AUTH_BASIC_LABEL,
            AUTH_JWT_LABEL,
            AUTH_API_KEY_LABEL,
            AUTH_API_KEY_HEADER_LABEL,
            AUTH_API_KEY_QUERY_LABEL,
        ];
        assert_eq!(
            auth_labels,
            [
                "Auth", "None", "Bearer", "OAuth", "Basic", "JWT", "API", "Header", "Query"
            ]
        );
        assert!(auth_labels.iter().all(|label| label.len() <= 6));

        let action_labels = [
            REQUEST_SEND_LABEL,
            PARAMS_PANEL_TITLE,
            HEADERS_PANEL_TITLE,
            HEADER_COPY_BULK_LABEL,
            HEADER_PASTE_BULK_LABEL,
            HEADER_ACCEPT_JSON_LABEL,
            HEADER_CONTENT_JSON_LABEL,
            HEADER_BEARER_AUTH_LABEL,
            PRE_REQUEST_PANEL_TITLE,
            RUNNER_PANEL_TITLE,
            RUNNER_STOP_ON_FAILURE_LABEL,
            RUNNER_RUN_ALL_LABEL,
            RUNNER_EMPTY_REQUESTS_LABEL,
            RUNNER_EMPTY_RESULTS_LABEL,
            RAW_FORMAT_JSON_LABEL,
            RAW_PREVIEW_TITLE,
            RAW_EMPTY_PREVIEW_LABEL,
            GRAPHQL_PANEL_TITLE,
            GRAPHQL_INTROSPECT_LABEL,
            GRAPHQL_PAYLOAD_TITLE,
            GRAPHQL_SCHEMA_TITLE,
            GRAPHQL_SCHEMA_BROWSER_TITLE,
            GRAPHQL_QUERY_ASSISTANT_TITLE,
            CODEGEN_PANEL_TITLE,
            CODEGEN_COPY_LABEL,
        ];
        assert_eq!(
            action_labels,
            [
                "Send",
                "Params",
                "Hdrs",
                "Copy",
                "Paste",
                "Accept",
                "Content",
                "Bearer",
                "Pre",
                "Runner",
                "Stop Fail",
                "Run",
                "No requests",
                "No results",
                "Format",
                "Preview",
                "No body",
                "GraphQL",
                "Schema",
                "Payload",
                "Schema",
                "Fields",
                "Templates",
                "Code",
                "Copy"
            ]
        );
        assert!(action_labels.iter().all(|label| label.len() <= 11));
        assert!(action_labels.iter().all(|label| !label.contains("Bulk")
            && !label.contains("JSON")
            && !label.contains("Auth")
            && !label.contains("Query")
            && !label.contains("Assistant")
            && !label.contains("Browser")
            && !label.contains("Headers")
            && !label.contains("Pre-request")
            && !label.contains("Syntax")
            && !label.contains("collection")));

        let raw_format_labels = [
            RAW_FORMAT_JSON_MODE_LABEL,
            RAW_FORMAT_XML_MODE_LABEL,
            RAW_FORMAT_TEXT_MODE_LABEL,
            RAW_FORMAT_HTML_MODE_LABEL,
        ];
        assert_eq!(raw_format_labels, ["JSON", "XML", "Text", "HTML"]);
        assert!(raw_format_labels.iter().all(|label| label.len() <= 4));

        let variables_labels = [
            VARIABLES_PANEL_TITLE,
            VARIABLES_ENV_LABEL,
            VARIABLES_NO_ENV_LABEL,
            VARIABLES_ADD_ENV_LABEL,
            VARIABLES_DELETE_ENV_LABEL,
            VARIABLES_GLOBAL_TITLE,
            VARIABLES_ENV_TITLE,
            VARIABLES_ENV_NEEDED_TITLE,
            VARIABLES_ENV_SELECTED_TITLE,
            VARIABLES_ENV_CREATED_TITLE,
            VARIABLES_ENV_DELETED_TITLE,
        ];
        assert_eq!(
            variables_labels,
            [
                "Vars",
                "Env",
                "No Env",
                "+ Env",
                "Del",
                "Global",
                "Env",
                "Env needed",
                "Active",
                "Created",
                "Removed"
            ]
        );
        assert!(variables_labels.iter().all(|label| label.len() <= 12));
        assert!(
            variables_labels
                .iter()
                .all(|label| !label.contains("Environment") && !label.contains("Delete"))
        );

        let realtime_placeholders = [
            PLACEHOLDER_WEBSOCKET_URL,
            PLACEHOLDER_WEBSOCKET_PROTOCOLS,
            PLACEHOLDER_SSE_URL,
        ];
        assert_eq!(realtime_placeholders, ["WS URL", "Protocols", "SSE URL"]);
        assert!(realtime_placeholders.iter().all(|label| label.len() <= 9));

        let realtime_labels = [
            REALTIME_WEBSOCKET_TITLE,
            REALTIME_WEBSOCKET_CONNECT_LABEL,
            REALTIME_WEBSOCKET_SEND_LABEL,
            REALTIME_WEBSOCKET_CLOSE_LABEL,
            REALTIME_WEBSOCKET_HEADERS_TITLE,
            REALTIME_WEBSOCKET_TEXT_LABEL,
            REALTIME_WEBSOCKET_BINARY_LABEL,
            REALTIME_WEBSOCKET_EMPTY_LABEL,
            REALTIME_SSE_TITLE,
            REALTIME_SSE_FETCH_LABEL,
            REALTIME_SSE_SUBSCRIBE_LABEL,
            REALTIME_SSE_STOP_LABEL,
            REALTIME_SSE_HEADERS_TITLE,
            REALTIME_SSE_EMPTY_LABEL,
            MOCK_LOG_TITLE,
            MOCK_LOG_EMPTY_LABEL,
        ];
        assert_eq!(
            realtime_labels,
            [
                "WS",
                "Open",
                "Send",
                "End",
                "WS Hdrs",
                "Text",
                "Hex",
                "No messages",
                "SSE",
                "Once",
                "Stream",
                "Stop",
                "SSE Hdrs",
                "No events",
                "Log",
                "No logs"
            ]
        );
        assert!(realtime_labels.iter().all(|label| label.len() <= 11));
        assert!(realtime_labels.iter().all(|label| !label.contains("mock")
            && !label.contains("Mock Log")
            && !label.contains("WebSocket")
            && !label.contains("WS messages")));

        let test_headers = [
            TEST_ASSERTION_NAME_HEADER,
            TEST_ASSERTION_KIND_HEADER,
            TEST_ASSERTION_TARGET_HEADER,
            TEST_ASSERTION_EXPECTED_HEADER,
        ];
        assert_eq!(test_headers, ["Test", "Kind", "Target", "Expect"]);
        assert!(test_headers.iter().all(|label| label.len() <= 6));

        let test_placeholders = [
            PLACEHOLDER_TEST_ASSERTION_NAME,
            PLACEHOLDER_TEST_ASSERTION_TARGET,
            PLACEHOLDER_TEST_ASSERTION_EXPECTED,
        ];
        assert_eq!(test_placeholders, ["Test", "Target", "Expect"]);
        assert!(test_placeholders.iter().all(|label| label.len() <= 6));

        let test_kind_labels = [
            TestAssertionKind::StatusEquals.label(),
            TestAssertionKind::StatusInRange.label(),
            TestAssertionKind::HeaderExists.label(),
            TestAssertionKind::HeaderEquals.label(),
            TestAssertionKind::BodyContains.label(),
            TestAssertionKind::JsonPathEquals.label(),
        ];
        assert_eq!(
            test_kind_labels,
            [
                "Status =", "Range", "Header ?", "Header =", "Body ?", "JSON ="
            ]
        );
        assert!(test_kind_labels.iter().all(|label| label.len() <= 8));
        assert!(
            test_kind_labels
                .iter()
                .all(|label| !label.contains("exists")
                    && !label.contains("contains")
                    && !label.contains("path"))
        );
    }

    #[test]
    fn sidebar_focus_shortcut_targets_active_section_input() {
        assert_eq!(
            sidebar_focus_target(SidebarSection::Endpoints),
            SidebarFocusTarget::RouteFilter
        );
        assert_eq!(
            sidebar_focus_target(SidebarSection::Collections),
            SidebarFocusTarget::CollectionPath
        );
        assert_eq!(
            sidebar_focus_target(SidebarSection::History),
            SidebarFocusTarget::HistoryFilter
        );
        assert!(can_focus_sidebar_input(
            false,
            SidebarFocusTarget::RouteFilter
        ));
        assert!(can_focus_sidebar_input(
            true,
            SidebarFocusTarget::RouteFilter
        ));
        assert!(can_focus_sidebar_input(
            false,
            SidebarFocusTarget::HistoryFilter
        ));
        assert!(can_focus_sidebar_input(
            true,
            SidebarFocusTarget::HistoryFilter
        ));
        assert!(can_focus_sidebar_input(
            false,
            SidebarFocusTarget::CollectionPath
        ));
        assert!(!can_focus_sidebar_input(
            true,
            SidebarFocusTarget::CollectionPath
        ));
    }

    #[test]
    fn request_url_focus_shortcut_respects_busy_state() {
        assert!(can_focus_request_url(false));
        assert!(!can_focus_request_url(true));
    }

    #[test]
    fn close_transient_ui_state_closes_all_temporary_layers() {
        let mut state = TransientUiState::default();
        assert!(!close_transient_ui_state(&mut state));
        assert_eq!(state, TransientUiState::default());

        state = TransientUiState {
            import_popover_open: true,
            method_menu_open: true,
            codegen_menu_open: true,
            collection_menu_open: true,
        };

        assert!(close_transient_ui_state(&mut state));
        assert_eq!(state, TransientUiState::default());
    }

    #[test]
    fn reset_scroll_handle_returns_scroll_to_origin() {
        let scroll = ScrollHandle::new();
        scroll.set_offset(point(px(12.), px(-128.)));

        reset_scroll_handle(&scroll);

        assert_eq!(scroll.offset(), point(px(LAYOUT_ZERO), px(LAYOUT_ZERO)));
    }

    #[test]
    fn sending_response_body_identifies_pending_request() {
        let body = sending_response_body("POST", "https://api.example.com/users");

        assert_eq!(PENDING_RESPONSE_BODY_LABEL, "Pending");
        assert!(PENDING_RESPONSE_BODY_LABEL.len() <= 8);
        assert_eq!(body, "Pending\n\nPOST https://api.example.com/users");
        assert!(!body.contains("response"));
    }

    #[test]
    fn request_worker_stopped_message_is_user_visible() {
        assert_eq!(request_worker_stopped_message(), "Request worker stopped.");
        assert!(!request_worker_stopped_message().contains(" before "));
    }

    #[derive(Debug, PartialEq, Eq)]
    struct VisibleCollectionRow {
        depth: usize,
        kind: &'static str,
        label: String,
        method: Option<String>,
    }

    fn visible_collection_rows(
        collection: &ApiCollection,
        expanded_nodes: &[String],
    ) -> Vec<VisibleCollectionRow> {
        let mut rows = vec![VisibleCollectionRow {
            depth: 0,
            kind: "collection",
            label: collection.name.clone(),
            method: None,
        }];

        if expanded_nodes.iter().any(|node| node == "collection") {
            append_visible_collection_rows(
                &mut rows,
                &collection.items,
                "collection",
                1,
                expanded_nodes,
            );
        }

        rows
    }

    fn append_visible_collection_rows(
        rows: &mut Vec<VisibleCollectionRow>,
        items: &[CollectionItem],
        parent_id: &str,
        depth: usize,
        expanded_nodes: &[String],
    ) {
        for (index, item) in items.iter().enumerate() {
            let id = format!("{parent_id}/{index}");
            match item {
                CollectionItem::Folder(folder) => {
                    rows.push(VisibleCollectionRow {
                        depth,
                        kind: "folder",
                        label: folder.name.clone(),
                        method: None,
                    });
                    if expanded_nodes.iter().any(|node| node == &id) {
                        append_visible_collection_rows(
                            rows,
                            &folder.items,
                            &id,
                            depth + 1,
                            expanded_nodes,
                        );
                    }
                }
                CollectionItem::Request(request) => rows.push(VisibleCollectionRow {
                    depth,
                    kind: "request",
                    label: request.name.clone(),
                    method: Some(request.method.clone()),
                }),
            }
        }
    }

    #[test]
    fn postman_collection_import_projects_to_visible_tree_rows() {
        let input = r#"
{
  "info": {
    "name": "Postman Demo",
    "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
  },
  "item": [
    {
      "name": "Users",
      "item": [
        {
          "name": "List users",
          "request": {
            "method": "GET",
            "url": "https://api.example.com/users"
          }
        },
        {
          "name": "Create user",
          "request": {
            "method": "POST",
            "url": "https://api.example.com/users"
          }
        }
      ]
    }
  ]
}
"#;
        let collection = ApiCollection::from_postman_json(input).expect("postman collection");

        let rows = visible_collection_rows(
            &collection,
            &["collection".to_string(), "collection/0".to_string()],
        );

        assert_eq!(
            rows,
            vec![
                VisibleCollectionRow {
                    depth: 0,
                    kind: "collection",
                    label: "Postman Demo".to_string(),
                    method: None,
                },
                VisibleCollectionRow {
                    depth: 1,
                    kind: "folder",
                    label: "Users".to_string(),
                    method: None,
                },
                VisibleCollectionRow {
                    depth: 2,
                    kind: "request",
                    label: "List users".to_string(),
                    method: Some("GET".to_string()),
                },
                VisibleCollectionRow {
                    depth: 2,
                    kind: "request",
                    label: "Create user".to_string(),
                    method: Some("POST".to_string()),
                },
            ]
        );
    }
}
