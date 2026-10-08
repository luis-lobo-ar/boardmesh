use crate::scene::{DrawableObject, SceneDocument, SceneRect};
use crate::selection_context::SelectionContext;
use crate::session::{RUNTIME_API_VERSION, SESSION_PROTOCOL_VERSION, SessionManifest};
use crate::universe::{CanvasFrame, CanvasMembership, UniverseDocument};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MutationResult {
    pub ok: bool,
    pub object_id: Option<String>,
    pub object_count: usize,
    pub selected_object_ids: Vec<String>,
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SelectedObjectsResult {
    pub selected_object_ids: Vec<String>,
    pub objects: Vec<DrawableObject>,
    pub object_count: usize,
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SelectionContextResult {
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
    pub context: SelectionContext,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneJsonResult {
    pub scene_json: String,
    pub object_count: usize,
    pub selected_object_ids: Vec<String>,
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SceneFileResult {
    pub ok: bool,
    pub target_path: String,
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
    pub object_count: usize,
    pub selected_object_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotResult {
    pub ok: bool,
    pub output_path: String,
    pub scope: String,
    pub bounds: SceneRect,
    pub width: u32,
    pub height: u32,
    pub object_count: usize,
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UniverseResult {
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
    pub universe: UniverseDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CanvasMutationResult {
    pub ok: bool,
    pub canvas_id: Option<String>,
    pub object_id: Option<String>,
    pub object_canvas_id: Option<String>,
    pub active_canvas_id: Option<String>,
    pub selected_canvas_ids: Vec<String>,
    pub canvases: Vec<CanvasFrame>,
    pub memberships: Vec<CanvasMembership>,
    pub session_id: Option<String>,
    pub protocol_version: u32,
    pub scene_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionWindowInfo {
    pub window_id: String,
    pub app_version: String,
    pub protocol_version: u32,
    pub pid: Option<u32>,
    pub last_seen_ms: u64,
    pub has_window_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionServerInfo {
    pub server_id: String,
    pub app_version: String,
    pub protocol_version: u32,
    pub runtime_api_version: u32,
    pub pid: Option<u32>,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionInfo {
    pub session_id: String,
    pub scene_path: String,
    pub protocol_version: u32,
    pub runtime_api_version: u32,
    pub app_version: String,
    pub attached_window_count: usize,
    pub attached_windows: Vec<SessionWindowInfo>,
    pub attached_server_count: usize,
    pub attached_servers: Vec<SessionServerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdapterSessionListResult {
    pub active_session_id: Option<String>,
    pub sessions_dir: String,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionResult {
    pub ok: bool,
    pub active_session_id: Option<String>,
    pub session: SessionInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionMigrationResult {
    pub ok: bool,
    pub previous_scene_path: String,
    pub active_session_id: Option<String>,
    pub session: SessionInfo,
}

pub fn mutation_result(
    scene: &SceneDocument,
    session_id: Option<String>,
    scene_path: String,
    object_id: Option<String>,
) -> MutationResult {
    MutationResult {
        ok: true,
        object_id,
        object_count: scene.object_count(),
        selected_object_ids: scene.selected_object_ids.clone(),
        session_id,
        protocol_version: SESSION_PROTOCOL_VERSION,
        scene_path,
    }
}

pub fn session_info_from_manifest(manifest: &SessionManifest) -> SessionInfo {
    SessionInfo {
        session_id: manifest.session_id.clone(),
        scene_path: manifest.scene_path.clone(),
        protocol_version: manifest.protocol_version,
        runtime_api_version: RUNTIME_API_VERSION,
        app_version: manifest.app_version.clone(),
        attached_window_count: manifest.attached_windows.len(),
        attached_windows: manifest
            .attached_windows
            .iter()
            .map(|window| SessionWindowInfo {
                window_id: window.window_id.clone(),
                app_version: window.app_version.clone(),
                protocol_version: window.protocol_version,
                pid: window.pid,
                last_seen_ms: window.last_seen_ms,
                has_window_state: window.window_state.is_some(),
            })
            .collect(),
        attached_server_count: manifest.attached_servers.len(),
        attached_servers: manifest
            .attached_servers
            .iter()
            .map(|server| SessionServerInfo {
                server_id: server.server_id.clone(),
                app_version: server.app_version.clone(),
                protocol_version: server.protocol_version,
                runtime_api_version: server.runtime_api_version,
                pid: server.pid,
                last_seen_ms: server.last_seen_ms,
            })
            .collect(),
    }
}
