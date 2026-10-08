use crate::scene::SceneDocument;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RUNTIME_PROTOCOL_VERSION: &str = "2.0";
pub const DEFAULT_RUNTIME_ADDR: &str = "127.0.0.1:9467";
pub const DEFAULT_RUNTIME_URL: &str = "ws://127.0.0.1:9467";

pub const METHOD_SESSION_ATTACH: &str = "session.attach";
pub const METHOD_SESSION_LIST: &str = "session.list";
pub const METHOD_SESSION_SWITCH: &str = "session.switch";
pub const METHOD_SCENE_GET: &str = "scene.get";
pub const METHOD_SCENE_REPLACE: &str = "scene.replace";
pub const METHOD_SCENE_UPDATED: &str = "scene.updated";
pub const METHOD_LIST_SESSIONS: &str = "list_sessions";
pub const METHOD_GET_SESSION_INFO: &str = "get_session_info";
pub const METHOD_CREATE_SESSION: &str = "create_session";
pub const METHOD_CREATE_SESSION_FROM_SCENE: &str = "create_session_from_scene";
pub const METHOD_SWITCH_SESSION: &str = "switch_session";
pub const METHOD_GET_SCENE: &str = "get_scene";
pub const METHOD_GET_UNIVERSE: &str = "get_universe";
pub const METHOD_GET_SELECTED_OBJECTS: &str = "get_selected_objects";
pub const METHOD_GET_SELECTION_CONTEXT: &str = "get_selection_context";
pub const METHOD_EXPORT_SCENE_JSON: &str = "export_scene_json";
pub const METHOD_RENDER_SCENE_PNG: &str = "render_scene_png";
pub const METHOD_RENDER_SELECTED_PNG: &str = "render_selected_png";
pub const METHOD_GET_TOOL_HELP: &str = "get_tool_help";
pub const METHOD_GET_API_DOCS: &str = "get_api_docs";
pub const METHOD_SAVE_SCENE_AS: &str = "save_scene_as";
pub const METHOD_LOAD_SCENE_JSON: &str = "load_scene_json";
pub const METHOD_SWITCH_SCENE: &str = "switch_scene";
pub const METHOD_CLEAR_SCENE: &str = "clear_scene";
pub const METHOD_ADD_LINE: &str = "add_line";
pub const METHOD_ADD_RECTANGLE: &str = "add_rectangle";
pub const METHOD_ADD_CIRCLE: &str = "add_circle";
pub const METHOD_ADD_TRIANGLE: &str = "add_triangle";
pub const METHOD_ADD_POLYGON: &str = "add_polygon";
pub const METHOD_ADD_STROKE: &str = "add_stroke";
pub const METHOD_ADD_TEXT: &str = "add_text";
pub const METHOD_FILL_OBJECT: &str = "fill_object";
pub const METHOD_DUPLICATE_OBJECT: &str = "duplicate_object";
pub const METHOD_SET_OBJECT_VISIBILITY: &str = "set_object_visibility";
pub const METHOD_SET_OBJECT_LOCKED: &str = "set_object_locked";
pub const METHOD_MOVE_OBJECT_TO_FRONT: &str = "move_object_to_front";
pub const METHOD_MOVE_OBJECT_TO_BACK: &str = "move_object_to_back";
pub const METHOD_MOVE_OBJECT_FORWARD: &str = "move_object_forward";
pub const METHOD_MOVE_OBJECT_BACKWARD: &str = "move_object_backward";
pub const METHOD_DELETE_OBJECT: &str = "delete_object";
pub const METHOD_SELECT_OBJECTS: &str = "select_objects";
pub const METHOD_CREATE_CANVAS: &str = "create_canvas";
pub const METHOD_SET_ACTIVE_CANVAS: &str = "set_active_canvas";
pub const METHOD_SELECT_CANVASES: &str = "select_canvases";
pub const METHOD_ASSIGN_OBJECT_TO_CANVAS: &str = "assign_object_to_canvas";
pub const METHOD_CLEAR_OBJECT_CANVAS_ASSIGNMENT: &str = "clear_object_canvas_assignment";

pub const RUNTIME_METHODS: &[&str] = &[
    METHOD_SESSION_ATTACH,
    METHOD_SESSION_LIST,
    METHOD_SESSION_SWITCH,
    METHOD_SCENE_GET,
    METHOD_SCENE_REPLACE,
    METHOD_LIST_SESSIONS,
    METHOD_GET_SESSION_INFO,
    METHOD_CREATE_SESSION,
    METHOD_CREATE_SESSION_FROM_SCENE,
    METHOD_SWITCH_SESSION,
    METHOD_GET_SCENE,
    METHOD_GET_UNIVERSE,
    METHOD_GET_SELECTED_OBJECTS,
    METHOD_GET_SELECTION_CONTEXT,
    METHOD_EXPORT_SCENE_JSON,
    METHOD_RENDER_SCENE_PNG,
    METHOD_RENDER_SELECTED_PNG,
    METHOD_GET_TOOL_HELP,
    METHOD_GET_API_DOCS,
    METHOD_SAVE_SCENE_AS,
    METHOD_LOAD_SCENE_JSON,
    METHOD_SWITCH_SCENE,
    METHOD_CLEAR_SCENE,
    METHOD_ADD_LINE,
    METHOD_ADD_RECTANGLE,
    METHOD_ADD_CIRCLE,
    METHOD_ADD_TRIANGLE,
    METHOD_ADD_POLYGON,
    METHOD_ADD_STROKE,
    METHOD_ADD_TEXT,
    METHOD_FILL_OBJECT,
    METHOD_DUPLICATE_OBJECT,
    METHOD_SET_OBJECT_VISIBILITY,
    METHOD_SET_OBJECT_LOCKED,
    METHOD_MOVE_OBJECT_TO_FRONT,
    METHOD_MOVE_OBJECT_TO_BACK,
    METHOD_MOVE_OBJECT_FORWARD,
    METHOD_MOVE_OBJECT_BACKWARD,
    METHOD_DELETE_OBJECT,
    METHOD_SELECT_OBJECTS,
    METHOD_CREATE_CANVAS,
    METHOD_SET_ACTIVE_CANVAS,
    METHOD_SELECT_CANVASES,
    METHOD_ASSIGN_OBJECT_TO_CANVAS,
    METHOD_CLEAR_OBJECT_CANVAS_ASSIGNMENT,
];

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct SessionAttachParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub create_if_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct SessionSwitchParams {
    pub session_id: String,
    #[serde(default)]
    pub create_if_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SessionListEntry {
    pub session_id: String,
    pub scene_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SessionListResult {
    pub active_session_id: Option<String>,
    pub scene_path: String,
    pub sessions_dir: String,
    pub sessions: Vec<SessionListEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RuntimeSceneSnapshot {
    pub session_id: Option<String>,
    pub scene_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_path: Option<String>,
    pub scene: SceneDocument,
    pub protocol_version: u32,
    pub runtime_api_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SceneReplaceParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub scene: SceneDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SceneUpdatedParams {
    pub source: String,
    pub snapshot: RuntimeSceneSnapshot,
}

impl JsonRpcRequest {
    pub fn new<T: Serialize>(id: u64, method: &str, params: T) -> Self {
        Self {
            jsonrpc: RUNTIME_PROTOCOL_VERSION.to_string(),
            id,
            method: method.to_string(),
            params: Some(serde_json::to_value(params).expect("json-rpc params should serialize")),
        }
    }
}

impl JsonRpcNotification {
    pub fn new<T: Serialize>(method: &str, params: T) -> Self {
        Self {
            jsonrpc: RUNTIME_PROTOCOL_VERSION.to_string(),
            method: method.to_string(),
            params: Some(serde_json::to_value(params).expect("json-rpc params should serialize")),
        }
    }
}

impl JsonRpcResponse {
    pub fn success<T: Serialize>(id: u64, result: T) -> Self {
        Self {
            jsonrpc: RUNTIME_PROTOCOL_VERSION.to_string(),
            id,
            result: Some(serde_json::to_value(result).expect("json-rpc result should serialize")),
            error: None,
        }
    }

    pub fn failure(id: u64, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: RUNTIME_PROTOCOL_VERSION.to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
            }),
        }
    }
}
