use boardmesh::render::{RenderScope, render_scene_png as render_to_png};
use boardmesh::runtime::{
    DEFAULT_RUNTIME_ADDR, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, METHOD_ADD_CIRCLE,
    METHOD_ADD_LINE, METHOD_ADD_POLYGON, METHOD_ADD_RECTANGLE, METHOD_ADD_STROKE, METHOD_ADD_TEXT,
    METHOD_ADD_TRIANGLE, METHOD_ASSIGN_OBJECT_TO_CANVAS, METHOD_CLEAR_OBJECT_CANVAS_ASSIGNMENT,
    METHOD_CLEAR_SCENE, METHOD_CREATE_CANVAS, METHOD_CREATE_SESSION,
    METHOD_CREATE_SESSION_FROM_SCENE, METHOD_DELETE_OBJECT, METHOD_DUPLICATE_OBJECT,
    METHOD_EXPORT_SCENE_JSON, METHOD_FILL_OBJECT, METHOD_GET_API_DOCS, METHOD_GET_SCENE,
    METHOD_GET_SELECTED_OBJECTS, METHOD_GET_SELECTION_CONTEXT, METHOD_GET_SESSION_INFO,
    METHOD_GET_TOOL_HELP, METHOD_GET_UNIVERSE, METHOD_LIST_SESSIONS, METHOD_LOAD_SCENE_JSON,
    METHOD_MOVE_OBJECT_BACKWARD, METHOD_MOVE_OBJECT_FORWARD, METHOD_MOVE_OBJECT_TO_BACK,
    METHOD_MOVE_OBJECT_TO_FRONT, METHOD_RENDER_SCENE_PNG, METHOD_RENDER_SELECTED_PNG,
    METHOD_SAVE_SCENE_AS, METHOD_SCENE_GET, METHOD_SCENE_REPLACE, METHOD_SCENE_UPDATED,
    METHOD_SELECT_CANVASES, METHOD_SELECT_OBJECTS, METHOD_SESSION_ATTACH, METHOD_SESSION_LIST,
    METHOD_SESSION_SWITCH, METHOD_SET_ACTIVE_CANVAS, METHOD_SET_OBJECT_LOCKED,
    METHOD_SET_OBJECT_VISIBILITY, METHOD_SWITCH_SCENE, METHOD_SWITCH_SESSION, RuntimeSceneSnapshot,
    SceneReplaceParams, SceneUpdatedParams, SessionAttachParams, SessionSwitchParams,
};
use boardmesh::scene::{
    CanvasPoint, SceneColor, SceneDocument, SceneRect, SceneStroke, TriangleObject,
};
use boardmesh::{
    AdapterSessionListResult as SessionListResult, AttachedServer, BoardWorkspace,
    CanvasMutationResult, MutationResult, RUNTIME_API_VERSION, SESSION_PROTOCOL_VERSION,
    SceneFileResult, SceneJsonResult, ScreenshotResult, SelectedObjectsResult,
    SelectionContextResult, SessionInfo, SessionManifest, SessionMigrationResult, SessionResult,
    UniverseResult, WorkspaceEvent, WorkspaceEventKind, WorkspaceEventPublisher, ensure_scene_file,
    load_scene, load_session_manifest, load_universe, mutation_result, resolve_session_path,
    save_session_manifest, save_universe, session_id_from_path, session_info_from_manifest,
    session_manifest_path, unix_time_ms,
};
use futures_util::{SinkExt, StreamExt};
use rmcp::{
    Json, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use uuid::Uuid;

#[derive(Clone)]
struct BoardmeshServer {
    server_id: String,
    tool_router: ToolRouter<Self>,
    workspace: Arc<Mutex<BoardWorkspace>>,
    runtime_events: broadcast::Sender<SceneUpdatedParams>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RuntimeConnectionState {
    session_id: Option<String>,
    scene_path: Option<PathBuf>,
}

#[derive(Clone)]
struct RuntimeEventPublisher {
    runtime_events: broadcast::Sender<SceneUpdatedParams>,
}

impl WorkspaceEventPublisher for RuntimeEventPublisher {
    fn publish(&self, event: WorkspaceEvent) -> anyhow::Result<()> {
        let source = match event.kind {
            WorkspaceEventKind::SceneChanged => "mcp",
            WorkspaceEventKind::SessionActivated => "session-switch",
        };
        let _ = self.runtime_events.send(SceneUpdatedParams {
            source: source.to_string(),
            snapshot: event.runtime_snapshot(),
        });
        Ok(())
    }
}

impl RuntimeConnectionState {
    fn from_snapshot(snapshot: &RuntimeSceneSnapshot) -> Self {
        Self {
            session_id: snapshot.session_id.clone(),
            scene_path: Some(PathBuf::from(&snapshot.scene_path)),
        }
    }

    fn update_from_snapshot(&mut self, snapshot: &RuntimeSceneSnapshot) {
        *self = Self::from_snapshot(snapshot);
    }

    fn update_from_scene_file(&mut self, session_id: Option<String>, scene_path: String) {
        self.session_id = session_id;
        self.scene_path = Some(PathBuf::from(scene_path));
    }

    fn matches_event(&self, event: &SceneUpdatedParams) -> bool {
        match (&self.session_id, &self.scene_path) {
            (Some(session_id), _) => {
                event.snapshot.session_id.as_deref() == Some(session_id.as_str())
            }
            (None, Some(scene_path)) => {
                Path::new(&event.snapshot.scene_path) == scene_path.as_path()
            }
            (None, None) => false,
        }
    }
}

fn log_mcp(message: &str) {
    eprintln!("[boardmesh-mcp] {message}");
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ToolHelpRequest {
    #[serde(default)]
    tool_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ToolHelpEntry {
    tool_name: String,
    description: String,
    parameter_notes: Vec<String>,
    example_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ToolHelpResult {
    server_summary: String,
    parameter_format_notes: Vec<String>,
    tools: Vec<ToolHelpEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ApiFieldDoc {
    name: String,
    required: bool,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ApiToolDoc {
    tool_name: String,
    description: String,
    request_fields: Vec<ApiFieldDoc>,
    returns: Vec<String>,
    example_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ApiDocsResult {
    server_summary: String,
    conventions: Vec<String>,
    tools: Vec<ApiToolDoc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddLineRequest {
    start: CanvasPoint,
    end: CanvasPoint,
    #[serde(default)]
    stroke: Option<SceneStroke>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddRectangleRequest {
    rect: SceneRect,
    #[serde(default)]
    stroke: Option<SceneStroke>,
    #[serde(default)]
    fill: Option<SceneColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddCircleRequest {
    center: CanvasPoint,
    radius: i32,
    #[serde(default)]
    stroke: Option<SceneStroke>,
    #[serde(default)]
    fill: Option<SceneColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddTriangleRequest {
    #[serde(default)]
    rect: Option<SceneRect>,
    #[serde(default)]
    points: Option<Vec<CanvasPoint>>,
    #[serde(default)]
    stroke: Option<SceneStroke>,
    #[serde(default)]
    fill: Option<SceneColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddPolygonRequest {
    points: Vec<CanvasPoint>,
    #[serde(default)]
    stroke: Option<SceneStroke>,
    #[serde(default)]
    fill: Option<SceneColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddStrokeRequest {
    points: Vec<CanvasPoint>,
    #[serde(default)]
    stroke: Option<SceneStroke>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AddTextRequest {
    position: CanvasPoint,
    text: String,
    #[serde(default)]
    color: Option<SceneColor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct FillObjectRequest {
    object_id: String,
    fill: SceneColor,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct DuplicateObjectRequest {
    object_id: String,
    #[serde(default = "default_duplicate_offset")]
    dx: i32,
    #[serde(default = "default_duplicate_offset")]
    dy: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SetObjectVisibilityRequest {
    object_id: String,
    visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SetObjectLockedRequest {
    object_id: String,
    locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ReorderObjectRequest {
    object_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct DeleteObjectRequest {
    object_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SelectObjectsRequest {
    object_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct CreateCanvasRequest {
    name: String,
    rect: SceneRect,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SetActiveCanvasRequest {
    #[serde(default)]
    canvas_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SelectCanvasesRequest {
    canvas_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct AssignObjectToCanvasRequest {
    object_id: String,
    canvas_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ClearObjectCanvasAssignmentRequest {
    object_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SaveSceneAsRequest {
    path: String,
    #[serde(default)]
    make_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct LoadSceneJsonRequest {
    scene_json: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    make_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SwitchSceneRequest {
    path: String,
    #[serde(default)]
    create_if_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct CreateSessionRequest {
    session_id: String,
    #[serde(default)]
    make_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct CreateSessionFromSceneRequest {
    session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct SwitchSessionRequest {
    session_id: String,
    #[serde(default)]
    create_if_missing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
struct ScreenshotRequest {
    #[serde(default)]
    path: Option<String>,
    #[serde(default = "default_screenshot_padding")]
    padding: i32,
}

#[derive(Debug, Clone)]
struct ServerOptions {
    session_root: PathBuf,
    active_session_id: Option<String>,
    scene_path: PathBuf,
    runtime_addr: Option<String>,
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for BoardmeshServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Controls versioned object-based board sessions used by the companion wxDragon desktop app. \
             Preferred workflow: use session tools first to create, list, inspect, and switch named sessions, \
             then use drawing/export tools against the active session. Raw scene-file workflows remain available \
             for compatibility but are not the primary multi-board mode.",
        )
    }
}

#[tool_router(router = tool_router)]
impl BoardmeshServer {
    fn new(
        session_root: PathBuf,
        active_session_id: Option<String>,
        scene_path: PathBuf,
        scene: SceneDocument,
    ) -> Self {
        let (runtime_events, _) = broadcast::channel(128);
        let publisher = Arc::new(RuntimeEventPublisher {
            runtime_events: runtime_events.clone(),
        });
        Self {
            server_id: format!("mcp-{}", Uuid::new_v4()),
            tool_router: Self::tool_router(),
            workspace: Arc::new(Mutex::new(BoardWorkspace::with_ports(
                session_root,
                active_session_id,
                scene_path,
                scene,
                Arc::new(boardmesh::FileSystemWorkspaceStorage),
                publisher,
            ))),
            runtime_events,
        }
    }

    #[tool(
        description = "List known named board sessions and indicate which one is currently active in this MCP server instance."
    )]
    async fn list_sessions(&self) -> Result<Json<SessionListResult>, String> {
        let session_root = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?
            .session_root()
            .to_path_buf();
        let sessions = boardmesh::list_session_ids(&session_root)
            .map_err(|error| format!("{error:#}"))?
            .into_iter()
            .filter_map(|session_id| self.load_session_info(&session_id).ok())
            .collect::<Vec<_>>();

        Ok(Json(SessionListResult {
            active_session_id: self.active_session_id(),
            sessions_dir: session_root.display().to_string(),
            sessions,
        }))
    }

    #[tool(
        description = "Return metadata for the currently active named session, if this MCP server instance is attached to one."
    )]
    async fn get_session_info(&self) -> Result<Json<SessionInfo>, String> {
        let Some(session_id) = self.active_session_id() else {
            return Err(
                "no active named session; this server is operating on a raw scene path".to_string(),
            );
        };
        Ok(Json(self.load_session_info(&session_id)?))
    }

    #[tool(
        description = "Create a new named board session backed by its own scene file and manifest. Optionally make it the active session for this MCP server instance."
    )]
    async fn create_session(
        &self,
        Parameters(request): Parameters<CreateSessionRequest>,
    ) -> Result<Json<SessionResult>, String> {
        let (scene_path, manifest) = self.ensure_named_session(&request.session_id)?;
        if request.make_active {
            self.activate_session(Some(request.session_id.clone()), scene_path)?;
        }
        Ok(Json(SessionResult {
            ok: true,
            active_session_id: self.active_session_id(),
            session: session_info_from_manifest(&manifest),
        }))
    }

    #[tool(
        description = "Create a named session from the currently active raw scene file and attach this MCP server instance to it. Use this to migrate orphan file-based boards into the session model."
    )]
    async fn create_session_from_scene(
        &self,
        Parameters(request): Parameters<CreateSessionFromSceneRequest>,
    ) -> Result<Json<SessionMigrationResult>, String> {
        if self.active_session_id().is_some() {
            return Err(
                "the server is already attached to a named session; use switch_session instead"
                    .to_string(),
            );
        }

        self.refresh_state_from_disk()?;
        let previous_scene_path = self.scene_path_string()?;
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        let (scene_path, manifest) = workspace
            .create_named_session_from_current_scene(&request.session_id)
            .map_err(|error| format!("{error:#}"))?;
        drop(workspace);
        self.activate_session(Some(request.session_id), scene_path)?;

        Ok(Json(SessionMigrationResult {
            ok: true,
            previous_scene_path,
            active_session_id: self.active_session_id(),
            session: session_info_from_manifest(&manifest),
        }))
    }

    #[tool(
        description = "Switch this MCP server instance to a named session. Set create_if_missing=true to create it on demand."
    )]
    async fn switch_session(
        &self,
        Parameters(request): Parameters<SwitchSessionRequest>,
    ) -> Result<Json<SessionResult>, String> {
        let (scene_path, manifest) = if request.create_if_missing {
            self.ensure_named_session(&request.session_id)?
        } else {
            let session_root = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?
                .session_root()
                .to_path_buf();
            let scene_path = resolve_session_path(&session_root, &request.session_id)
                .map_err(|error| format!("{error:#}"))?;
            let manifest_path = session_manifest_path(&session_root, &request.session_id);
            let manifest =
                load_session_manifest(&manifest_path).map_err(|error| format!("{error:#}"))?;
            (scene_path, manifest)
        };
        self.activate_session(Some(request.session_id.clone()), scene_path)?;
        Ok(Json(SessionResult {
            ok: true,
            active_session_id: self.active_session_id(),
            session: session_info_from_manifest(&manifest),
        }))
    }

    #[tool(description = "Return the current board scene as structured JSON.")]
    async fn get_scene(&self) -> Result<Json<SceneDocument>, String> {
        self.refresh_state_from_disk()?;
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        Ok(Json(workspace.scene_clone()))
    }

    #[tool(
        description = "Return the active Boardmesh universe document, including canvas frames, selection state, memberships, viewport state, and the embedded scene."
    )]
    async fn get_universe(&self) -> Result<Json<UniverseResult>, String> {
        Ok(Json(self.runtime_get_universe(
            None,
            &RuntimeConnectionState::default(),
        )?))
    }

    #[tool(description = "Return the currently selected board objects.")]
    async fn get_selected_objects(&self) -> Result<Json<SelectedObjectsResult>, String> {
        self.refresh_state_from_disk()?;
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;

        let scene = workspace.scene();
        let objects = scene.selected_objects().into_iter().cloned().collect();
        Ok(Json(SelectedObjectsResult {
            selected_object_ids: scene.selected_object_ids.clone(),
            objects,
            object_count: scene.object_count(),
            session_id: workspace.active_session_id_owned(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: workspace.scene_path_string(),
        }))
    }

    #[tool(
        description = "Return semantic context for the current selection, including canvas membership, connected objects, and nearby objects."
    )]
    async fn get_selection_context(&self) -> Result<Json<SelectionContextResult>, String> {
        Ok(Json(self.runtime_get_selection_context(
            None,
            &RuntimeConnectionState::default(),
        )?))
    }

    #[tool(
        description = "Create a named canvas frame inside the active universe document and make it the active canvas."
    )]
    async fn create_canvas(
        &self,
        Parameters(request): Parameters<CreateCanvasRequest>,
    ) -> Result<Json<CanvasMutationResult>, String> {
        Ok(Json(self.runtime_create_canvas(
            None,
            &RuntimeConnectionState::default(),
            request,
        )?))
    }

    #[tool(description = "Set or clear the active canvas in the active universe document.")]
    async fn set_active_canvas(
        &self,
        Parameters(request): Parameters<SetActiveCanvasRequest>,
    ) -> Result<Json<CanvasMutationResult>, String> {
        Ok(Json(self.runtime_set_active_canvas(
            None,
            &RuntimeConnectionState::default(),
            request,
        )?))
    }

    #[tool(description = "Replace the current canvas selection with the provided canvas ids.")]
    async fn select_canvases(
        &self,
        Parameters(request): Parameters<SelectCanvasesRequest>,
    ) -> Result<Json<CanvasMutationResult>, String> {
        Ok(Json(self.runtime_select_canvases(
            None,
            &RuntimeConnectionState::default(),
            request,
        )?))
    }

    #[tool(description = "Assign an existing scene object to a canvas frame by id.")]
    async fn assign_object_to_canvas(
        &self,
        Parameters(request): Parameters<AssignObjectToCanvasRequest>,
    ) -> Result<Json<CanvasMutationResult>, String> {
        Ok(Json(self.runtime_assign_object_to_canvas(
            None,
            &RuntimeConnectionState::default(),
            request,
        )?))
    }

    #[tool(description = "Remove any canvas membership from an existing scene object.")]
    async fn clear_object_canvas_assignment(
        &self,
        Parameters(request): Parameters<ClearObjectCanvasAssignmentRequest>,
    ) -> Result<Json<CanvasMutationResult>, String> {
        Ok(Json(self.runtime_clear_object_canvas_assignment(
            None,
            &RuntimeConnectionState::default(),
            request,
        )?))
    }

    #[tool(
        description = "Return the complete board scene as a pretty-printed JSON string suitable for saving to another file."
    )]
    async fn export_scene_json(&self) -> Result<Json<SceneJsonResult>, String> {
        self.refresh_state_from_disk()?;
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        let scene_json = serde_json::to_string_pretty(workspace.scene())
            .map_err(|error| format!("failed to serialize scene: {error}"))?;

        Ok(Json(SceneJsonResult {
            scene_json,
            object_count: workspace.scene().object_count(),
            selected_object_ids: workspace.scene().selected_object_ids.clone(),
            session_id: workspace.active_session_id_owned(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: workspace.scene_path_string(),
        }))
    }

    #[tool(
        description = "Render the whole active board scene to a PNG file and return the output path plus crop metadata."
    )]
    async fn render_scene_png(
        &self,
        Parameters(request): Parameters<ScreenshotRequest>,
    ) -> Result<Json<ScreenshotResult>, String> {
        self.refresh_state_from_disk()?;
        let scene = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?
            .scene_clone();
        let output_path = screenshot_target_path(request.path.as_deref(), "scene");
        let rendered = render_to_png(
            &scene,
            &output_path,
            RenderScope::Scene,
            request.padding.max(0),
        )
        .map_err(|error| format!("{error:#}"))?;

        Ok(Json(self.screenshot_result(
            &output_path,
            "scene",
            rendered,
        )?))
    }

    #[tool(
        description = "Render only the currently selected objects to a cropped PNG file and return the output path plus crop metadata."
    )]
    async fn render_selected_png(
        &self,
        Parameters(request): Parameters<ScreenshotRequest>,
    ) -> Result<Json<ScreenshotResult>, String> {
        self.refresh_state_from_disk()?;
        let scene = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?
            .scene_clone();
        let output_path = screenshot_target_path(request.path.as_deref(), "selection");
        let rendered = render_to_png(
            &scene,
            &output_path,
            RenderScope::Selected,
            request.padding.max(0),
        )
        .map_err(|error| format!("{error:#}"))?;

        Ok(Json(self.screenshot_result(
            &output_path,
            "selected",
            rendered,
        )?))
    }

    #[tool(
        description = "Return example payloads and parameter format notes for Boardmesh MCP tools. Use this when an agent needs concrete argument shapes."
    )]
    async fn get_tool_help(
        &self,
        Parameters(request): Parameters<ToolHelpRequest>,
    ) -> Result<Json<ToolHelpResult>, String> {
        let tools = match request.tool_name.as_deref() {
            Some(tool_name) => vec![
                self.tool_help_entry(tool_name)
                    .ok_or_else(|| format!("unknown tool: {tool_name}"))?,
            ],
            None => self.all_tool_help_entries(),
        };

        Ok(Json(ToolHelpResult {
            server_summary: "Object-based Boardmesh MCP server. Preferred workflow: use named-session tools first (list_sessions, create_session, switch_session, get_session_info), then use drawing/export tools on the active session. Clients can inspect MCP tool schemas, and this tool returns concrete example payloads for agents that need explicit argument shapes. For the full command reference in one call, use get_api_docs.".to_string(),
            parameter_format_notes: vec![
                "CanvasPoint accepts {\"x\":380,\"y\":392}, [380,392], or \"380,392\".".to_string(),
                "SceneRect accepts {\"x\":650,\"y\":120,\"width\":150,\"height\":24}, [650,120,150,24], or \"650,120,150,24\".".to_string(),
                "Colors use {\"r\":255,\"g\":200,\"b\":0,\"a\":255}. The alpha channel is optional in examples only if your client fills missing fields explicitly; safest is to send all four fields.".to_string(),
                "Prefer named-session tools for multi-board work. Treat raw scene-file workflows as compatibility mode.".to_string(),
                "If get_session_info fails because there is no active named session, use create_session_from_scene to migrate the current raw scene into a managed session.".to_string(),
                "For the full per-command reference with request fields and return fields, call get_api_docs.".to_string(),
            ],
            tools,
        }))
    }

    #[tool(
        description = "Return full documentation for every Boardmesh MCP command in one call, including request fields, return fields, and example payloads."
    )]
    async fn get_api_docs(&self) -> Result<Json<ApiDocsResult>, String> {
        Ok(Json(ApiDocsResult {
            server_summary: "Object-based Boardmesh MCP server. Preferred mode is named sessions backed by scene and manifest files. Raw scene-file mode remains available for compatibility.".to_string(),
            conventions: vec![
                "Recommended order: list_sessions/create_session/switch_session/get_session_info first, then drawing and export tools.".to_string(),
                "All mutating tools persist the active scene file immediately.".to_string(),
                "Most mutating tools return: ok, object_id when applicable, object_count, selected_object_ids, and scene_path.".to_string(),
                "Named sessions are the primary multi-board workflow. Raw scene-file tools and --scene mode are compatibility workflows.".to_string(),
                "If this server is still attached to a raw scene file, use create_session_from_scene to migrate that board into a named session before doing multi-board/session-oriented work.".to_string(),
                "CanvasPoint accepts {\"x\":380,\"y\":392}, [380,392], or \"380,392\".".to_string(),
                "SceneRect accepts {\"x\":650,\"y\":120,\"width\":150,\"height\":24}, [650,120,150,24], or \"650,120,150,24\".".to_string(),
                "SceneColor uses {\"r\":255,\"g\":200,\"b\":0,\"a\":255}. Use all four fields for compatibility.".to_string(),
                "SceneStroke uses {\"color\": SceneColor, \"width\": number}.".to_string(),
                "Object ids come from get_scene or get_selected_objects.".to_string(),
                "Triangles can be created with add_triangle(rect=...) for quick isosceles shapes or add_triangle(points=[...]) for arbitrary three-vertex shapes.".to_string(),
                "add_polygon(points=[...]) also accepts exactly three points if you want to model triangles as generic polygons.".to_string(),
            ],
            tools: self.api_docs_entries(),
        }))
    }

    #[tool(
        description = "Save the current scene to another file path. Optionally make that file the active scene being worked on."
    )]
    async fn save_scene_as(
        &self,
        Parameters(request): Parameters<SaveSceneAsRequest>,
    ) -> Result<Json<SceneFileResult>, String> {
        self.refresh_state_from_disk()?;
        let target_path = PathBuf::from(request.path);
        let (_scene, result) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            workspace
                .save_scene_as(target_path.clone(), request.make_active)
                .map_err(|error| format!("{error:#}"))?;
            let scene = workspace.scene_clone();
            let result = self.scene_file_result_for_workspace(&workspace, &target_path, &scene)?;
            (scene, result)
        };
        Ok(Json(result))
    }

    #[tool(
        description = "Load a complete scene from a JSON string. By default it replaces the active scene file; you can also write it to another path and optionally switch to that file."
    )]
    async fn load_scene_json(
        &self,
        Parameters(request): Parameters<LoadSceneJsonRequest>,
    ) -> Result<Json<SceneFileResult>, String> {
        let scene = serde_json::from_str::<SceneDocument>(&request.scene_json)
            .map_err(|error| format!("failed to parse scene json: {error}"))?;

        let requested_path = request.path.clone();
        let target_path = if let Some(path) = requested_path {
            PathBuf::from(path)
        } else {
            self.scene_path()?
        };

        let result = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            workspace
                .load_scene_json(
                    scene.clone(),
                    target_path.clone(),
                    request.make_active || request.path.is_none(),
                )
                .map_err(|error| format!("{error:#}"))?;
            self.scene_file_result_for_workspace(&workspace, &target_path, workspace.scene())?
        };
        Ok(Json(result))
    }

    #[tool(
        description = "Switch the active scene file the MCP server is working on. You can optionally create a new default scene if the file does not exist."
    )]
    async fn switch_scene(
        &self,
        Parameters(request): Parameters<SwitchSceneRequest>,
    ) -> Result<Json<SceneFileResult>, String> {
        let target_path = PathBuf::from(request.path);
        let (_scene, result) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            workspace
                .switch_scene(target_path.clone(), request.create_if_missing)
                .map_err(|error| format!("{error:#}"))?;
            let scene = workspace.scene_clone();
            let result = self.scene_file_result_for_workspace(&workspace, &target_path, &scene)?;
            (scene, result)
        };
        Ok(Json(result))
    }

    #[tool(description = "Clear every object from the board scene.")]
    async fn clear_scene(&self) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            workspace
                .clear_scene()
                .map_err(|error| format!("{error:#}"))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, None);
            (scene, result, session_id)
        };
        self.log_mutation("clear_scene", None, session_id.as_deref(), &scene);
        Ok(Json(result))
    }

    #[tool(description = "Add a straight line object to the board.")]
    async fn add_line(
        &self,
        Parameters(request): Parameters<AddLineRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let object_id = workspace
                .add_line(
                    request.start,
                    request.end,
                    request.stroke.unwrap_or_default(),
                )
                .map_err(|error| format!("{error:#}"))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation("add_line", Some(&object_id), session_id.as_deref(), &scene);
        Ok(Json(result))
    }

    #[tool(description = "Add a rectangle object to the board.")]
    async fn add_rectangle(
        &self,
        Parameters(request): Parameters<AddRectangleRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let object_id = workspace
                .add_rectangle(
                    request.rect,
                    request.stroke.unwrap_or_default(),
                    request.fill,
                )
                .map_err(|error| format!("{error:#}"))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation(
            "add_rectangle",
            Some(&object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Add a circle object to the board.")]
    async fn add_circle(
        &self,
        Parameters(request): Parameters<AddCircleRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let object_id = workspace
                .add_circle(
                    request.center,
                    request.radius,
                    request.stroke.unwrap_or_default(),
                    request.fill,
                )
                .map_err(|error| format!("{error:#}"))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation(
            "add_circle",
            Some(&object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(
        description = "Add a triangle object to the board. This is a convenience wrapper over a 3-point shape: provide either a quick bounding rect or exactly three explicit points."
    )]
    async fn add_triangle(
        &self,
        Parameters(request): Parameters<AddTriangleRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let triangle = request.triangle()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let object_id = workspace
                .add_triangle(triangle, request.stroke.unwrap_or_default(), request.fill)
                .map_err(|error| format!("{error:#}"))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation(
            "add_triangle",
            Some(&object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Add a polygon object to the board from an ordered list of points.")]
    async fn add_polygon(
        &self,
        Parameters(request): Parameters<AddPolygonRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let Some(object_id) = workspace
                .add_polygon(
                    request.points,
                    request.stroke.unwrap_or_default(),
                    request.fill,
                )
                .map_err(|error| format!("{error:#}"))?
            else {
                return Err("polygon requires at least three points".to_string());
            };
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation(
            "add_polygon",
            Some(&object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Add a freehand stroke object to the board.")]
    async fn add_stroke(
        &self,
        Parameters(request): Parameters<AddStrokeRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let Some(object_id) = workspace
                .add_stroke(request.points, request.stroke.unwrap_or_default())
                .map_err(|error| format!("{error:#}"))?
            else {
                return Err("stroke requires at least two points".to_string());
            };
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation(
            "add_stroke",
            Some(&object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Add a text label object to the board.")]
    async fn add_text(
        &self,
        Parameters(request): Parameters<AddTextRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let object_id = workspace
                .add_text(
                    request.position,
                    request.text,
                    request.color.unwrap_or(SceneColor::rgb(15, 23, 42)),
                )
                .map_err(|error| format!("{error:#}"))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation("add_text", Some(&object_id), session_id.as_deref(), &scene);
        Ok(Json(result))
    }

    #[tool(description = "Set or replace the fill color of an existing object.")]
    async fn fill_object(
        &self,
        Parameters(request): Parameters<FillObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .fill_object(&request.object_id, request.fill)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "fill_object",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Duplicate an existing object by id with an optional offset.")]
    async fn duplicate_object(
        &self,
        Parameters(request): Parameters<DuplicateObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (object_id, scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            let object_id = workspace
                .duplicate_object(&request.object_id, request.dx, request.dy)
                .map_err(|error| format!("{error:#}"))?
                .ok_or_else(|| format!("object not found: {}", request.object_id))?;
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(object_id.clone()));
            (object_id, scene, result, session_id)
        };
        self.log_mutation(
            "duplicate_object",
            Some(&object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Set whether an object is visible.")]
    async fn set_object_visibility(
        &self,
        Parameters(request): Parameters<SetObjectVisibilityRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .set_object_visibility(&request.object_id, request.visible)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "set_object_visibility",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Set whether an object is locked against movement.")]
    async fn set_object_locked(
        &self,
        Parameters(request): Parameters<SetObjectLockedRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .set_object_locked(&request.object_id, request.locked)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "set_object_locked",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Move an object to the front of the draw order.")]
    async fn move_object_to_front(
        &self,
        Parameters(request): Parameters<ReorderObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .move_object_to_front(&request.object_id)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "move_object_to_front",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Move an object to the back of the draw order.")]
    async fn move_object_to_back(
        &self,
        Parameters(request): Parameters<ReorderObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .move_object_to_back(&request.object_id)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "move_object_to_back",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Move an object one step forward in the draw order.")]
    async fn move_object_forward(
        &self,
        Parameters(request): Parameters<ReorderObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .move_object_forward(&request.object_id)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "move_object_forward",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Move an object one step backward in the draw order.")]
    async fn move_object_backward(
        &self,
        Parameters(request): Parameters<ReorderObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .move_object_backward(&request.object_id)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "move_object_backward",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Delete a board object by id.")]
    async fn delete_object(
        &self,
        Parameters(request): Parameters<DeleteObjectRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .delete_object(&request.object_id)
                .map_err(|error| format!("{error:#}"))?
            {
                return Err(format!("object not found: {}", request.object_id));
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, Some(request.object_id.clone()));
            (scene, result, session_id)
        };
        self.log_mutation(
            "delete_object",
            Some(&request.object_id),
            session_id.as_deref(),
            &scene,
        );
        Ok(Json(result))
    }

    #[tool(description = "Replace the current object selection with the provided object ids.")]
    async fn select_objects(
        &self,
        Parameters(request): Parameters<SelectObjectsRequest>,
    ) -> Result<Json<MutationResult>, String> {
        self.refresh_state_from_disk()?;
        let (scene, result, session_id) = {
            let mut workspace = self
                .workspace
                .lock()
                .map_err(|_| "workspace lock poisoned".to_string())?;
            if !workspace
                .select_objects(request.object_ids.clone())
                .map_err(|error| format!("{error:#}"))?
            {
                return Err("one or more object ids were not found".to_string());
            }
            let session_id = workspace.active_session_id_owned();
            let scene = workspace.scene_clone();
            let result = self.result_for_workspace(&workspace, None);
            (scene, result, session_id)
        };
        self.log_mutation("select_objects", None, session_id.as_deref(), &scene);
        Ok(Json(result))
    }

    fn refresh_state_from_disk(&self) -> Result<(), String> {
        let _ = self.write_server_heartbeat();
        let mut workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        workspace
            .refresh_from_disk()
            .map_err(|error| format!("{error:#}"))?;
        log_mcp(&format!(
            "refreshed state from disk session={:?} scene={} object_count={}",
            workspace.active_session_id_owned(),
            workspace.scene_path().display(),
            workspace.scene().object_count()
        ));
        Ok(())
    }

    fn log_mutation(
        &self,
        tool_name: &str,
        object_id: Option<&str>,
        session_id: Option<&str>,
        scene: &SceneDocument,
    ) {
        log_mcp(&format!(
            "tool={} session={:?} object_id={:?} object_count={} selected_count={}",
            tool_name,
            session_id,
            object_id,
            scene.object_count(),
            scene.selected_object_ids.len()
        ));
    }

    fn result_for_workspace(
        &self,
        workspace: &BoardWorkspace,
        object_id: Option<String>,
    ) -> MutationResult {
        mutation_result(
            workspace.scene(),
            workspace.active_session_id_owned(),
            workspace.scene_path_string(),
            object_id,
        )
    }

    fn scene_file_result_for_workspace(
        &self,
        workspace: &BoardWorkspace,
        target_path: &Path,
        scene: &SceneDocument,
    ) -> Result<SceneFileResult, String> {
        Ok(SceneFileResult {
            ok: true,
            target_path: target_path.display().to_string(),
            session_id: workspace.active_session_id_owned(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: workspace.scene_path_string(),
            object_count: scene.object_count(),
            selected_object_ids: scene.selected_object_ids.clone(),
        })
    }

    fn screenshot_result(
        &self,
        output_path: &Path,
        scope: &str,
        rendered: boardmesh::RenderResult,
    ) -> Result<ScreenshotResult, String> {
        Ok(ScreenshotResult {
            ok: true,
            output_path: output_path.display().to_string(),
            scope: scope.to_string(),
            bounds: rendered.bounds,
            width: rendered.width,
            height: rendered.height,
            object_count: rendered.object_count,
            session_id: self.active_session_id(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: self.scene_path_string()?,
        })
    }

    fn screenshot_result_for_workspace(
        &self,
        workspace: &BoardWorkspace,
        output_path: &Path,
        scope: &str,
        rendered: boardmesh::RenderResult,
    ) -> Result<ScreenshotResult, String> {
        Ok(ScreenshotResult {
            ok: true,
            output_path: output_path.display().to_string(),
            scope: scope.to_string(),
            bounds: rendered.bounds,
            width: rendered.width,
            height: rendered.height,
            object_count: rendered.object_count,
            session_id: workspace.active_session_id_owned(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: workspace.scene_path_string(),
        })
    }

    fn scene_path(&self) -> Result<PathBuf, String> {
        self.workspace
            .lock()
            .map(|workspace| workspace.scene_path().to_path_buf())
            .map_err(|_| "workspace lock poisoned".to_string())
    }

    fn scene_path_string(&self) -> Result<String, String> {
        Ok(self.scene_path()?.display().to_string())
    }

    fn active_session_id(&self) -> Option<String> {
        self.workspace
            .lock()
            .ok()
            .and_then(|workspace| workspace.active_session_id_owned())
    }

    fn session_root(&self) -> Result<PathBuf, String> {
        self.workspace
            .lock()
            .map(|workspace| workspace.session_root().to_path_buf())
            .map_err(|_| "workspace lock poisoned".to_string())
    }

    fn current_runtime_scope(&self) -> Result<RuntimeConnectionState, String> {
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        Ok(RuntimeConnectionState {
            session_id: workspace.active_session_id_owned(),
            scene_path: Some(workspace.scene_path().to_path_buf()),
        })
    }

    fn runtime_scope_for_session(
        &self,
        session_id: &str,
        create_if_missing: bool,
    ) -> Result<RuntimeConnectionState, String> {
        let session_root = self.session_root()?;
        let manifest_path = session_manifest_path(&session_root, session_id);
        let scene_path = if create_if_missing {
            self.ensure_named_session(session_id)?.0
        } else {
            let _ = load_session_manifest(&manifest_path).map_err(|error| format!("{error:#}"))?;
            resolve_session_path(&session_root, session_id).map_err(|error| format!("{error:#}"))?
        };
        Ok(RuntimeConnectionState {
            session_id: Some(session_id.to_string()),
            scene_path: Some(scene_path),
        })
    }

    fn runtime_scope_for_request(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<RuntimeConnectionState, String> {
        if let Some(session_id) = requested_session_id {
            return self.runtime_scope_for_session(session_id, false);
        }
        if connection.scene_path.is_some() || connection.session_id.is_some() {
            return Ok(connection.clone());
        }
        self.current_runtime_scope()
    }

    fn runtime_workspace_for_scope(
        &self,
        scope: &RuntimeConnectionState,
    ) -> Result<BoardWorkspace, String> {
        let session_root = self.session_root()?;
        let scene_path = scope
            .scene_path
            .clone()
            .ok_or_else(|| "runtime session is not attached to a scene".to_string())?;
        let active_session_id = match scope.session_id.clone() {
            Some(session_id) => Some(session_id),
            None => session_id_from_path(&session_root, &scene_path)
                .filter(|session_id| session_manifest_path(&session_root, session_id).exists()),
        };
        let scene = load_scene(&scene_path).map_err(|error| format!("{error:#}"))?;

        Ok(BoardWorkspace::with_ports(
            session_root,
            active_session_id,
            scene_path,
            scene,
            Arc::new(boardmesh::FileSystemWorkspaceStorage),
            Arc::new(RuntimeEventPublisher {
                runtime_events: self.runtime_events.clone(),
            }),
        ))
    }

    fn ensure_named_session(&self, session_id: &str) -> Result<(PathBuf, SessionManifest), String> {
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        workspace
            .ensure_named_session(session_id)
            .map_err(|error| format!("{error:#}"))
    }

    fn write_server_heartbeat(&self) -> Result<(), String> {
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        let Some(session_id) = workspace.active_session_id() else {
            return Ok(());
        };
        let scene_path = workspace.scene_path().to_path_buf();
        let manifest_path = session_manifest_path(workspace.session_root(), session_id);
        let mut manifest = if manifest_path.exists() {
            load_session_manifest(&manifest_path).map_err(|error| format!("{error:#}"))?
        } else {
            SessionManifest::new(session_id.to_string(), scene_path.clone())
        };
        manifest.scene_path = scene_path.display().to_string();
        manifest.update_server(AttachedServer {
            server_id: self.server_id.clone(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            runtime_api_version: RUNTIME_API_VERSION,
            pid: Some(std::process::id()),
            last_seen_ms: unix_time_ms(),
        });
        save_session_manifest(&manifest_path, &manifest).map_err(|error| format!("{error:#}"))
    }

    fn load_session_info(&self, session_id: &str) -> Result<SessionInfo, String> {
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        let manifest_path = session_manifest_path(workspace.session_root(), session_id);
        let manifest =
            load_session_manifest(&manifest_path).map_err(|error| format!("{error:#}"))?;
        Ok(session_info_from_manifest(&manifest))
    }

    fn activate_session(
        &self,
        session_id: Option<String>,
        scene_path: PathBuf,
    ) -> Result<(), String> {
        let mut workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        workspace
            .activate_session(session_id, scene_path.clone())
            .map_err(|error| format!("{error:#}"))?;
        log_mcp(&format!(
            "activated session={:?} scene={}",
            workspace.active_session_id_owned(),
            workspace.scene_path().display()
        ));
        Ok(())
    }

    fn runtime_session_list(&self) -> Result<boardmesh::RuntimeSessionListResult, String> {
        let workspace = self
            .workspace
            .lock()
            .map_err(|_| "workspace lock poisoned".to_string())?;
        workspace
            .runtime_session_list()
            .map_err(|error| format!("{error:#}"))
    }

    fn attach_runtime_session(
        &self,
        params: SessionAttachParams,
    ) -> Result<RuntimeSceneSnapshot, String> {
        if let Some(session_id) = params.session_id.as_deref() {
            let scope = self.runtime_scope_for_session(session_id, params.create_if_missing)?;
            return self
                .runtime_workspace_for_scope(&scope)
                .map(|workspace| workspace.current_runtime_snapshot());
        }
        let scope = self.current_runtime_scope()?;
        self.runtime_workspace_for_scope(&scope)
            .map(|workspace| workspace.current_runtime_snapshot())
    }

    fn switch_runtime_session(
        &self,
        params: SessionSwitchParams,
    ) -> Result<RuntimeSceneSnapshot, String> {
        let scope = self.runtime_scope_for_session(&params.session_id, params.create_if_missing)?;
        self.runtime_workspace_for_scope(&scope)
            .map(|workspace| workspace.current_runtime_snapshot())
    }

    fn replace_scene_from_runtime(
        &self,
        connection: &RuntimeConnectionState,
        params: SceneReplaceParams,
    ) -> Result<RuntimeSceneSnapshot, String> {
        let scope = self.runtime_scope_for_request(params.session_id.as_deref(), connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        workspace
            .replace_scene(params.scene)
            .map_err(|error| format!("{error:#}"))
    }

    fn runtime_snapshot_for_request(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<RuntimeSceneSnapshot, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        self.runtime_workspace_for_scope(&scope)
            .map(|workspace| workspace.current_runtime_snapshot())
    }

    fn runtime_get_scene(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<SceneDocument, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        self.runtime_workspace_for_scope(&scope)
            .map(|workspace| workspace.scene_clone())
    }

    fn load_runtime_universe(
        &self,
        scope: &RuntimeConnectionState,
    ) -> Result<(PathBuf, Option<String>, UniverseResult), String> {
        let session_root = self.session_root()?;
        let scene_path = scope
            .scene_path
            .clone()
            .ok_or_else(|| "runtime session is not attached to a scene".to_string())?;
        let session_id = match scope.session_id.clone() {
            Some(session_id) => Some(session_id),
            None => session_id_from_path(&session_root, &scene_path)
                .filter(|session_id| session_manifest_path(&session_root, session_id).exists()),
        };
        let universe = load_universe(&scene_path).map_err(|error| format!("{error:#}"))?;
        Ok((
            scene_path.clone(),
            session_id.clone(),
            UniverseResult {
                session_id,
                protocol_version: SESSION_PROTOCOL_VERSION,
                scene_path: scene_path.display().to_string(),
                universe,
            },
        ))
    }

    fn runtime_get_universe(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<UniverseResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (_, _, result) = self.load_runtime_universe(&scope)?;
        Ok(result)
    }

    fn canvas_mutation_result(
        &self,
        scene_path: &Path,
        session_id: Option<String>,
        universe: &boardmesh::UniverseDocument,
        canvas_id: Option<String>,
        object_id: Option<String>,
        object_canvas_id: Option<String>,
    ) -> CanvasMutationResult {
        CanvasMutationResult {
            ok: true,
            canvas_id,
            object_id,
            object_canvas_id,
            active_canvas_id: universe.active_canvas_id.clone(),
            selected_canvas_ids: universe.selected_canvas_ids.clone(),
            canvases: universe.canvases.clone(),
            memberships: universe.memberships.clone(),
            session_id,
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: scene_path.display().to_string(),
        }
    }

    fn runtime_create_canvas(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: CreateCanvasRequest,
    ) -> Result<CanvasMutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (scene_path, session_id, mut result) = self.load_runtime_universe(&scope)?;
        let canvas_id = result.universe.create_canvas(request.name, request.rect);
        save_universe(&scene_path, &result.universe).map_err(|error| format!("{error:#}"))?;
        Ok(self.canvas_mutation_result(
            &scene_path,
            session_id,
            &result.universe,
            Some(canvas_id),
            None,
            None,
        ))
    }

    fn runtime_set_active_canvas(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SetActiveCanvasRequest,
    ) -> Result<CanvasMutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (scene_path, session_id, mut result) = self.load_runtime_universe(&scope)?;
        if !result
            .universe
            .set_active_canvas(request.canvas_id.as_deref())
        {
            return Err("canvas id was not found".to_string());
        }
        save_universe(&scene_path, &result.universe).map_err(|error| format!("{error:#}"))?;
        Ok(self.canvas_mutation_result(
            &scene_path,
            session_id,
            &result.universe,
            request.canvas_id,
            None,
            None,
        ))
    }

    fn runtime_select_canvases(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SelectCanvasesRequest,
    ) -> Result<CanvasMutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (scene_path, session_id, mut result) = self.load_runtime_universe(&scope)?;
        if !result.universe.select_canvases(request.canvas_ids.clone()) {
            return Err("one or more canvas ids were not found".to_string());
        }
        save_universe(&scene_path, &result.universe).map_err(|error| format!("{error:#}"))?;
        Ok(
            self.canvas_mutation_result(
                &scene_path,
                session_id,
                &result.universe,
                None,
                None,
                None,
            ),
        )
    }

    fn runtime_assign_object_to_canvas(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AssignObjectToCanvasRequest,
    ) -> Result<CanvasMutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (scene_path, session_id, mut result) = self.load_runtime_universe(&scope)?;
        if !result
            .universe
            .assign_object_to_canvas(&request.object_id, &request.canvas_id)
        {
            return Err("object id or canvas id was not found".to_string());
        }
        save_universe(&scene_path, &result.universe).map_err(|error| format!("{error:#}"))?;
        Ok(self.canvas_mutation_result(
            &scene_path,
            session_id,
            &result.universe,
            Some(request.canvas_id.clone()),
            Some(request.object_id.clone()),
            Some(request.canvas_id),
        ))
    }

    fn runtime_clear_object_canvas_assignment(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ClearObjectCanvasAssignmentRequest,
    ) -> Result<CanvasMutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (scene_path, session_id, mut result) = self.load_runtime_universe(&scope)?;
        if !result
            .universe
            .clear_object_canvas_assignment(&request.object_id)
        {
            return Err("object id did not have a canvas assignment".to_string());
        }
        save_universe(&scene_path, &result.universe).map_err(|error| format!("{error:#}"))?;
        Ok(self.canvas_mutation_result(
            &scene_path,
            session_id,
            &result.universe,
            None,
            Some(request.object_id),
            None,
        ))
    }

    fn runtime_get_selected_objects(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<SelectedObjectsResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let workspace = self.runtime_workspace_for_scope(&scope)?;
        let scene = workspace.scene();
        Ok(SelectedObjectsResult {
            selected_object_ids: scene.selected_object_ids.clone(),
            objects: scene.selected_objects().into_iter().cloned().collect(),
            object_count: scene.object_count(),
            session_id: workspace.active_session_id_owned(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: workspace.scene_path_string(),
        })
    }

    fn runtime_get_selection_context(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<SelectionContextResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let (_, _, universe_result) = self.load_runtime_universe(&scope)?;
        Ok(SelectionContextResult {
            session_id: universe_result.session_id,
            protocol_version: universe_result.protocol_version,
            scene_path: universe_result.scene_path,
            context: boardmesh::SelectionContext::from_universe(&universe_result.universe),
        })
    }

    fn runtime_export_scene_json(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<SceneJsonResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let workspace = self.runtime_workspace_for_scope(&scope)?;
        let scene_json = serde_json::to_string_pretty(workspace.scene())
            .map_err(|error| format!("failed to serialize scene: {error}"))?;
        Ok(SceneJsonResult {
            scene_json,
            object_count: workspace.scene().object_count(),
            selected_object_ids: workspace.scene().selected_object_ids.clone(),
            session_id: workspace.active_session_id_owned(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            scene_path: workspace.scene_path_string(),
        })
    }

    fn runtime_render_scene_png(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ScreenshotRequest,
    ) -> Result<ScreenshotResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let workspace = self.runtime_workspace_for_scope(&scope)?;
        let output_path = screenshot_target_path(request.path.as_deref(), "scene");
        let rendered = render_to_png(
            workspace.scene(),
            &output_path,
            RenderScope::Scene,
            request.padding.max(0),
        )
        .map_err(|error| format!("{error:#}"))?;
        self.screenshot_result_for_workspace(&workspace, &output_path, "scene", rendered)
    }

    fn runtime_render_selected_png(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ScreenshotRequest,
    ) -> Result<ScreenshotResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let workspace = self.runtime_workspace_for_scope(&scope)?;
        let output_path = screenshot_target_path(request.path.as_deref(), "selection");
        let rendered = render_to_png(
            workspace.scene(),
            &output_path,
            RenderScope::Selected,
            request.padding.max(0),
        )
        .map_err(|error| format!("{error:#}"))?;
        self.screenshot_result_for_workspace(&workspace, &output_path, "selected", rendered)
    }

    fn runtime_save_scene_as(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SaveSceneAsRequest,
    ) -> Result<SceneFileResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let target_path = PathBuf::from(request.path);
        workspace
            .save_scene_as(target_path.clone(), request.make_active)
            .map_err(|error| format!("{error:#}"))?;
        self.scene_file_result_for_workspace(&workspace, &target_path, workspace.scene())
    }

    fn runtime_load_scene_json(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: LoadSceneJsonRequest,
    ) -> Result<SceneFileResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let scene = serde_json::from_str::<SceneDocument>(&request.scene_json)
            .map_err(|error| format!("failed to parse scene json: {error}"))?;
        let target_path = if let Some(path) = request.path.clone() {
            PathBuf::from(path)
        } else {
            workspace.scene_path().to_path_buf()
        };
        let make_active = request.make_active || request.path.is_none();
        workspace
            .load_scene_json(scene, target_path.clone(), make_active)
            .map_err(|error| format!("{error:#}"))?;
        self.scene_file_result_for_workspace(&workspace, &target_path, workspace.scene())
    }

    fn runtime_switch_scene(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SwitchSceneRequest,
    ) -> Result<SceneFileResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let target_path = PathBuf::from(request.path);
        workspace
            .switch_scene(target_path.clone(), request.create_if_missing)
            .map_err(|error| format!("{error:#}"))?;
        self.scene_file_result_for_workspace(&workspace, &target_path, workspace.scene())
    }

    fn runtime_clear_scene(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        workspace
            .clear_scene()
            .map_err(|error| format!("{error:#}"))?;
        Ok(self.result_for_workspace(&workspace, None))
    }

    fn runtime_add_line(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddLineRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let object_id = workspace
            .add_line(
                request.start,
                request.end,
                request.stroke.unwrap_or_default(),
            )
            .map_err(|error| format!("{error:#}"))?;
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_add_rectangle(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddRectangleRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let object_id = workspace
            .add_rectangle(
                request.rect,
                request.stroke.unwrap_or_default(),
                request.fill,
            )
            .map_err(|error| format!("{error:#}"))?;
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_add_circle(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddCircleRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let object_id = workspace
            .add_circle(
                request.center,
                request.radius,
                request.stroke.unwrap_or_default(),
                request.fill,
            )
            .map_err(|error| format!("{error:#}"))?;
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_add_triangle(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddTriangleRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let object_id = workspace
            .add_triangle(
                request.triangle()?,
                request.stroke.unwrap_or_default(),
                request.fill,
            )
            .map_err(|error| format!("{error:#}"))?;
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_add_polygon(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddPolygonRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let Some(object_id) = workspace
            .add_polygon(
                request.points,
                request.stroke.unwrap_or_default(),
                request.fill,
            )
            .map_err(|error| format!("{error:#}"))?
        else {
            return Err("polygon requires at least three points".to_string());
        };
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_add_stroke(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddStrokeRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let Some(object_id) = workspace
            .add_stroke(request.points, request.stroke.unwrap_or_default())
            .map_err(|error| format!("{error:#}"))?
        else {
            return Err("stroke requires at least two points".to_string());
        };
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_add_text(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: AddTextRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let object_id = workspace
            .add_text(
                request.position,
                request.text,
                request.color.unwrap_or(SceneColor::rgb(15, 23, 42)),
            )
            .map_err(|error| format!("{error:#}"))?;
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_fill_object(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: FillObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .fill_object(&request.object_id, request.fill)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_duplicate_object(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: DuplicateObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        let object_id = workspace
            .duplicate_object(&request.object_id, request.dx, request.dy)
            .map_err(|error| format!("{error:#}"))?
            .ok_or_else(|| format!("object not found: {}", request.object_id))?;
        Ok(self.result_for_workspace(&workspace, Some(object_id)))
    }

    fn runtime_set_object_visibility(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SetObjectVisibilityRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .set_object_visibility(&request.object_id, request.visible)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_set_object_locked(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SetObjectLockedRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .set_object_locked(&request.object_id, request.locked)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_move_object_to_front(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ReorderObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .move_object_to_front(&request.object_id)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_move_object_to_back(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ReorderObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .move_object_to_back(&request.object_id)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_move_object_forward(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ReorderObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .move_object_forward(&request.object_id)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_move_object_backward(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: ReorderObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .move_object_backward(&request.object_id)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_delete_object(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: DeleteObjectRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .delete_object(&request.object_id)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err(format!("object not found: {}", request.object_id));
        }
        Ok(self.result_for_workspace(&workspace, Some(request.object_id)))
    }

    fn runtime_select_objects(
        &self,
        requested_session_id: Option<&str>,
        connection: &RuntimeConnectionState,
        request: SelectObjectsRequest,
    ) -> Result<MutationResult, String> {
        let scope = self.runtime_scope_for_request(requested_session_id, connection)?;
        let mut workspace = self.runtime_workspace_for_scope(&scope)?;
        if !workspace
            .select_objects(request.object_ids)
            .map_err(|error| format!("{error:#}"))?
        {
            return Err("one or more object ids were not found".to_string());
        }
        Ok(self.result_for_workspace(&workspace, None))
    }

    fn all_tool_help_entries(&self) -> Vec<ToolHelpEntry> {
        [
            "list_sessions",
            "get_session_info",
            "create_session",
            "create_session_from_scene",
            "switch_session",
            "get_scene",
            "get_universe",
            "get_selected_objects",
            "get_selection_context",
            "create_canvas",
            "set_active_canvas",
            "select_canvases",
            "assign_object_to_canvas",
            "clear_object_canvas_assignment",
            "export_scene_json",
            "render_scene_png",
            "render_selected_png",
            "get_tool_help",
            "get_api_docs",
            "save_scene_as",
            "load_scene_json",
            "switch_scene",
            "clear_scene",
            "add_line",
            "add_rectangle",
            "add_circle",
            "add_triangle",
            "add_polygon",
            "add_stroke",
            "add_text",
            "fill_object",
            "duplicate_object",
            "set_object_visibility",
            "set_object_locked",
            "move_object_to_front",
            "move_object_to_back",
            "move_object_forward",
            "move_object_backward",
            "delete_object",
            "select_objects",
        ]
        .into_iter()
        .filter_map(|tool_name| self.tool_help_entry(tool_name))
        .collect()
    }

    fn tool_help_entry(&self, tool_name: &str) -> Option<ToolHelpEntry> {
        match tool_name {
            "list_sessions" => Some(ToolHelpEntry {
                tool_name: "list_sessions".to_string(),
                description: "List known named board sessions and show which one is active in this MCP server instance.".to_string(),
                parameter_notes: vec![
                    "No parameters.".to_string(),
                    "Named sessions are scene files under the sessions directory plus a companion manifest file.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            }),
            "get_session_info" => Some(ToolHelpEntry {
                tool_name: "get_session_info".to_string(),
                description: "Return metadata for the currently active named session.".to_string(),
                parameter_notes: vec![
                    "No parameters.".to_string(),
                    "Fails if the server is operating on a raw scene path instead of a named session.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            }),
            "create_session" => Some(ToolHelpEntry {
                tool_name: "create_session".to_string(),
                description: "Create a named session backed by its own scene file and manifest.".to_string(),
                parameter_notes: vec![
                    "session_id must match the session id naming rules documented by get_api_docs.".to_string(),
                    "Set make_active=true to immediately switch this MCP server instance to the new session.".to_string(),
                ],
                example_args: vec![
                    r#"{"session_id":"agent-a"}"#.to_string(),
                    r#"{"session_id":"shared-review","make_active":true}"#.to_string(),
                ],
            }),
            "create_session_from_scene" => Some(ToolHelpEntry {
                tool_name: "create_session_from_scene".to_string(),
                description: "Create a named session from the currently active raw scene file and attach this MCP server to it.".to_string(),
                parameter_notes: vec![
                    "Use this when get_session_info fails because the server is still operating on a raw scene path.".to_string(),
                    "Fails if the server is already attached to a named session.".to_string(),
                    "Copies the current scene into the managed sessions directory under the requested session id.".to_string(),
                ],
                example_args: vec![r#"{"session_id":"migrated-board"}"#.to_string()],
            }),
            "switch_session" => Some(ToolHelpEntry {
                tool_name: "switch_session".to_string(),
                description: "Switch this MCP server instance to a named session.".to_string(),
                parameter_notes: vec![
                    "session_id selects the target session.".to_string(),
                    "Set create_if_missing=true to create the session manifest and scene on demand.".to_string(),
                ],
                example_args: vec![
                    r#"{"session_id":"agent-a"}"#.to_string(),
                    r#"{"session_id":"shared-review","create_if_missing":true}"#.to_string(),
                ],
            }),
            "get_scene" => Some(ToolHelpEntry {
                tool_name: "get_scene".to_string(),
                description: "Return the active board scene as structured JSON.".to_string(),
                parameter_notes: vec!["No parameters.".to_string()],
                example_args: vec!["{}".to_string()],
            }),
            "get_universe" => Some(ToolHelpEntry {
                tool_name: "get_universe".to_string(),
                description: "Return the active universe document, including canvases, memberships, viewport, and the embedded scene.".to_string(),
                parameter_notes: vec![
                    "No parameters.".to_string(),
                    "Legacy scene files are migrated on read into a default universe wrapper.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            }),
            "get_selected_objects" => Some(ToolHelpEntry {
                tool_name: "get_selected_objects".to_string(),
                description: "Return the active selection and full object payloads for selected objects.".to_string(),
                parameter_notes: vec!["No parameters.".to_string()],
                example_args: vec!["{}".to_string()],
            }),
            "get_selection_context" => Some(ToolHelpEntry {
                tool_name: "get_selection_context".to_string(),
                description: "Return semantic context for the current selection, including canvas membership, connected objects, and nearby objects.".to_string(),
                parameter_notes: vec![
                    "No parameters.".to_string(),
                    "Uses the active universe document plus the current scene selection.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            }),
            "create_canvas" => Some(ToolHelpEntry {
                tool_name: "create_canvas".to_string(),
                description: "Create a named canvas frame in the active universe.".to_string(),
                parameter_notes: vec![
                    "name is the user-facing canvas title.".to_string(),
                    "rect uses SceneRect in world coordinates.".to_string(),
                ],
                example_args: vec![
                    r#"{"name":"Review","rect":{"x":0,"y":0,"width":240,"height":160}}"#
                        .to_string(),
                ],
            }),
            "set_active_canvas" => Some(ToolHelpEntry {
                tool_name: "set_active_canvas".to_string(),
                description: "Set the active canvas by id, or clear it with null.".to_string(),
                parameter_notes: vec![
                    "canvas_id must already exist when provided.".to_string(),
                    "Use null to clear the active canvas.".to_string(),
                ],
                example_args: vec![
                    r#"{"canvas_id":"canvas-123"}"#.to_string(),
                    r#"{"canvas_id":null}"#.to_string(),
                ],
            }),
            "select_canvases" => Some(ToolHelpEntry {
                tool_name: "select_canvases".to_string(),
                description: "Replace the current canvas selection with the provided canvas ids.".to_string(),
                parameter_notes: vec!["All ids must already exist.".to_string()],
                example_args: vec![
                    r#"{"canvas_ids":["canvas-123"]}"#.to_string(),
                    r#"{"canvas_ids":["canvas-123","canvas-456"]}"#.to_string(),
                ],
            }),
            "assign_object_to_canvas" => Some(ToolHelpEntry {
                tool_name: "assign_object_to_canvas".to_string(),
                description: "Assign an existing scene object to a canvas frame.".to_string(),
                parameter_notes: vec![
                    "Both object_id and canvas_id must already exist.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123","canvas_id":"canvas-123"}"#.to_string(),
                ],
            }),
            "clear_object_canvas_assignment" => Some(ToolHelpEntry {
                tool_name: "clear_object_canvas_assignment".to_string(),
                description: "Remove any canvas membership from an existing scene object.".to_string(),
                parameter_notes: vec![
                    "Fails if the object is not currently assigned to a canvas.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            }),
            "export_scene_json" => Some(ToolHelpEntry {
                tool_name: "export_scene_json".to_string(),
                description: "Return the full scene as a pretty-printed JSON string.".to_string(),
                parameter_notes: vec!["No parameters.".to_string()],
                example_args: vec!["{}".to_string()],
            }),
            "render_scene_png" => Some(ToolHelpEntry {
                tool_name: "render_scene_png".to_string(),
                description: "Render the whole active scene to a PNG file.".to_string(),
                parameter_notes: vec![
                    "If path is omitted, the server writes to a temporary png file.".to_string(),
                    "padding expands the crop rectangle around the rendered content.".to_string(),
                ],
                example_args: vec![
                    "{}".to_string(),
                    r#"{"path":"/tmp/boardmesh-scene.png","padding":24}"#.to_string(),
                ],
            }),
            "render_selected_png" => Some(ToolHelpEntry {
                tool_name: "render_selected_png".to_string(),
                description: "Render only the currently selected objects to a cropped PNG file.".to_string(),
                parameter_notes: vec![
                    "If path is omitted, the server writes to a temporary png file.".to_string(),
                    "Fails if there is no current selection.".to_string(),
                ],
                example_args: vec![
                    "{}".to_string(),
                    r#"{"path":"/tmp/boardmesh-selection.png","padding":16}"#.to_string(),
                ],
            }),
            "save_scene_as" => Some(ToolHelpEntry {
                tool_name: "save_scene_as".to_string(),
                description: "Write the current scene to another file path.".to_string(),
                parameter_notes: vec!["Set make_active=true if the new file should become the active scene.".to_string()],
                example_args: vec![
                    r#"{"path":"/tmp/diagram-copy.json"}"#.to_string(),
                    r#"{"path":"/tmp/diagram-copy.json","make_active":true}"#.to_string(),
                ],
            }),
            "load_scene_json" => Some(ToolHelpEntry {
                tool_name: "load_scene_json".to_string(),
                description: "Load a complete scene JSON string into the active scene or another path.".to_string(),
                parameter_notes: vec![
                    "scene_json must be a full serialized SceneDocument string.".to_string(),
                    "If path is omitted, the active scene file is replaced.".to_string(),
                ],
                example_args: vec![
                    r#"{"scene_json":"{\"objects\":[],\"selected_object_ids\":[]}"}"#.to_string(),
                    r#"{"scene_json":"{\"objects\":[],\"selected_object_ids\":[]}","path":"/tmp/new-scene.json","make_active":true}"#.to_string(),
                ],
            }),
            "switch_scene" => Some(ToolHelpEntry {
                tool_name: "switch_scene".to_string(),
                description: "Change which scene file the MCP server is currently working on.".to_string(),
                parameter_notes: vec!["Set create_if_missing=true to create a default scene file if it does not exist.".to_string()],
                example_args: vec![
                    r#"{"path":"/tmp/existing-scene.json"}"#.to_string(),
                    r#"{"path":"/tmp/new-scene.json","create_if_missing":true}"#.to_string(),
                ],
            }),
            "clear_scene" => Some(ToolHelpEntry {
                tool_name: "clear_scene".to_string(),
                description: "Remove all objects from the active scene.".to_string(),
                parameter_notes: vec!["No parameters.".to_string()],
                example_args: vec!["{}".to_string()],
            }),
            "add_line" => Some(ToolHelpEntry {
                tool_name: "add_line".to_string(),
                description: "Add a straight line object.".to_string(),
                parameter_notes: vec![
                    "start and end use CanvasPoint.".to_string(),
                    "stroke is optional.".to_string(),
                ],
                example_args: vec![
                    r#"{"start":{"x":80,"y":120},"end":{"x":260,"y":120}}"#.to_string(),
                    r#"{"start":"80,120","end":"260,120","stroke":{"color":{"r":30,"g":41,"b":59,"a":255},"width":3}}"#.to_string(),
                ],
            }),
            "add_rectangle" => Some(ToolHelpEntry {
                tool_name: "add_rectangle".to_string(),
                description: "Add a rectangle object.".to_string(),
                parameter_notes: vec![
                    "rect uses SceneRect.".to_string(),
                    "stroke and fill are optional.".to_string(),
                ],
                example_args: vec![
                    r#"{"rect":{"x":120,"y":140,"width":220,"height":100}}"#.to_string(),
                    r#"{"rect":"120,140,220,100","fill":{"r":255,"g":240,"b":180,"a":255}}"#.to_string(),
                ],
            }),
            "add_circle" => Some(ToolHelpEntry {
                tool_name: "add_circle".to_string(),
                description: "Add a circle object.".to_string(),
                parameter_notes: vec![
                    "center uses CanvasPoint.".to_string(),
                    "radius is an integer pixel value.".to_string(),
                ],
                example_args: vec![
                    r#"{"center":{"x":420,"y":220},"radius":60}"#.to_string(),
                    r#"{"center":"420,220","radius":60,"fill":{"r":190,"g":230,"b":255,"a":255}}"#.to_string(),
                ],
            }),
            "add_triangle" => Some(ToolHelpEntry {
                tool_name: "add_triangle".to_string(),
                description: "Add a triangle using either a quick bounding rectangle or three explicit points. For arbitrary rotated triangles, prefer points.".to_string(),
                parameter_notes: vec![
                    "Provide either rect or points.".to_string(),
                    "rect creates the old quick isosceles triangle from a drag box.".to_string(),
                    "points must contain exactly three CanvasPoint values.".to_string(),
                    "add_polygon with exactly three points is also valid if you want to treat triangles as generic polygons.".to_string(),
                ],
                example_args: vec![
                    r#"{"rect":{"x":520,"y":140,"width":180,"height":120}}"#.to_string(),
                    r#"{"rect":"520,140,180,120","fill":{"r":255,"g":210,"b":120,"a":255}}"#.to_string(),
                    r#"{"points":["610,120","520,260","700,260"]}"#.to_string(),
                ],
            }),
            "add_polygon" => Some(ToolHelpEntry {
                tool_name: "add_polygon".to_string(),
                description: "Add a polygon from an ordered list of points. A 3-point polygon is effectively a free-form triangle.".to_string(),
                parameter_notes: vec![
                    "points must contain at least three CanvasPoint values.".to_string(),
                    "fill is optional.".to_string(),
                ],
                example_args: vec![
                    r#"{"points":[{"x":100,"y":80},{"x":220,"y":80},{"x":260,"y":180},{"x":140,"y":220}]}"#.to_string(),
                    r#"{"points":["100,80","220,80","260,180","140,220"],"fill":{"r":220,"g":245,"b":210,"a":255}}"#.to_string(),
                ],
            }),
            "add_stroke" => Some(ToolHelpEntry {
                tool_name: "add_stroke".to_string(),
                description: "Add a freehand stroke from an ordered list of points.".to_string(),
                parameter_notes: vec!["points must contain at least two CanvasPoint values.".to_string()],
                example_args: vec![
                    r#"{"points":[{"x":80,"y":300},{"x":120,"y":320},{"x":180,"y":315},{"x":240,"y":340}]}"#.to_string(),
                    r#"{"points":["80,300","120,320","180,315","240,340"],"stroke":{"color":{"r":220,"g":38,"b":38,"a":255},"width":4}}"#.to_string(),
                ],
            }),
            "add_text" => Some(ToolHelpEntry {
                tool_name: "add_text".to_string(),
                description: "Add a text label object.".to_string(),
                parameter_notes: vec![
                    "position uses CanvasPoint.".to_string(),
                    "color is optional.".to_string(),
                ],
                example_args: vec![
                    r#"{"position":{"x":380,"y":392},"text":"API server"}"#.to_string(),
                    r#"{"position":"380,392","text":"API server","color":{"r":15,"g":23,"b":42,"a":255}}"#.to_string(),
                ],
            }),
            "fill_object" => Some(ToolHelpEntry {
                tool_name: "fill_object".to_string(),
                description: "Set or replace the fill color of an existing object by id.".to_string(),
                parameter_notes: vec!["Use get_scene or get_selected_objects to find object ids.".to_string()],
                example_args: vec![r#"{"object_id":"obj-123","fill":{"r":255,"g":220,"b":120,"a":255}}"#.to_string()],
            }),
            "duplicate_object" => Some(ToolHelpEntry {
                tool_name: "duplicate_object".to_string(),
                description: "Duplicate an existing object by id with an optional offset.".to_string(),
                parameter_notes: vec![
                    "Use get_scene or get_selected_objects to find object ids.".to_string(),
                    "dx and dy default to 24 pixels if omitted.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123"}"#.to_string(),
                    r#"{"object_id":"obj-123","dx":48,"dy":12}"#.to_string(),
                ],
            }),
            "set_object_visibility" => Some(ToolHelpEntry {
                tool_name: "set_object_visibility".to_string(),
                description: "Hide or show an object by id.".to_string(),
                parameter_notes: vec!["Hidden objects stay in the scene but no longer participate in hit testing.".to_string()],
                example_args: vec![
                    r#"{"object_id":"obj-123","visible":false}"#.to_string(),
                    r#"{"object_id":"obj-123","visible":true}"#.to_string(),
                ],
            }),
            "set_object_locked" => Some(ToolHelpEntry {
                tool_name: "set_object_locked".to_string(),
                description: "Lock or unlock an object against movement.".to_string(),
                parameter_notes: vec!["Locked objects remain visible and selectable.".to_string()],
                example_args: vec![
                    r#"{"object_id":"obj-123","locked":true}"#.to_string(),
                    r#"{"object_id":"obj-123","locked":false}"#.to_string(),
                ],
            }),
            "move_object_to_front" => Some(ToolHelpEntry {
                tool_name: "move_object_to_front".to_string(),
                description: "Move an object to the front of the draw order.".to_string(),
                parameter_notes: vec!["Use get_scene to inspect current ordering if needed.".to_string()],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            }),
            "move_object_to_back" => Some(ToolHelpEntry {
                tool_name: "move_object_to_back".to_string(),
                description: "Move an object to the back of the draw order.".to_string(),
                parameter_notes: vec!["Use get_scene to inspect current ordering if needed.".to_string()],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            }),
            "move_object_forward" => Some(ToolHelpEntry {
                tool_name: "move_object_forward".to_string(),
                description: "Move an object one step forward in the draw order.".to_string(),
                parameter_notes: vec!["Use get_scene to inspect current ordering if needed.".to_string()],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            }),
            "move_object_backward" => Some(ToolHelpEntry {
                tool_name: "move_object_backward".to_string(),
                description: "Move an object one step backward in the draw order.".to_string(),
                parameter_notes: vec!["Use get_scene to inspect current ordering if needed.".to_string()],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            }),
            "delete_object" => Some(ToolHelpEntry {
                tool_name: "delete_object".to_string(),
                description: "Delete a single object by id.".to_string(),
                parameter_notes: vec!["Use get_scene or get_selected_objects to find object ids.".to_string()],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            }),
            "select_objects" => Some(ToolHelpEntry {
                tool_name: "select_objects".to_string(),
                description: "Replace the current selection with the provided object ids.".to_string(),
                parameter_notes: vec!["All ids must exist in the scene.".to_string()],
                example_args: vec![
                    r#"{"object_ids":["obj-123"]}"#.to_string(),
                    r#"{"object_ids":["obj-123","obj-456"]}"#.to_string(),
                ],
            }),
            _ => None,
        }
    }

    fn api_docs_entries(&self) -> Vec<ApiToolDoc> {
        vec![
            ApiToolDoc {
                tool_name: "list_sessions".to_string(),
                description: "List the named board sessions known under the configured sessions directory.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "active_session_id: current named session for this MCP server instance, if any.".to_string(),
                    "sessions_dir: directory used to store named session scene and manifest files.".to_string(),
                    "sessions: array of session metadata.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "get_session_info".to_string(),
                description: "Return metadata for the currently active named session.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "session_id: active named session id.".to_string(),
                    "scene_path: backing scene file path.".to_string(),
                    "protocol_version: required session protocol version.".to_string(),
                    "runtime_api_version: this MCP binary's runtime API version.".to_string(),
                    "attached_windows: live windows recorded by the GUI heartbeat.".to_string(),
                    "attached_servers: live MCP servers recorded by the server heartbeat.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "create_session".to_string(),
                description: "Create a named session backed by a scene JSON file and a manifest JSON file.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "session_id".to_string(),
                        required: true,
                        description: "New session id. ASCII letters, digits, '-', '_' and '.' are allowed.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "make_active".to_string(),
                        required: false,
                        description: "If true, switch this MCP server instance to the new session immediately.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "active_session_id: current named session after the call.".to_string(),
                    "session: metadata for the created session.".to_string(),
                ],
                example_args: vec![
                    r#"{"session_id":"agent-a"}"#.to_string(),
                    r#"{"session_id":"shared-review","make_active":true}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "create_session_from_scene".to_string(),
                description: "Create a named session from the currently active raw scene file and attach this MCP server to it. Use this to migrate orphan file-based boards into the named-session workflow.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "session_id".to_string(),
                    required: true,
                    description: "New session id. ASCII letters, digits, '-', '_' and '.' are allowed.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "previous_scene_path: raw scene path that was migrated.".to_string(),
                    "active_session_id: current named session after the call.".to_string(),
                    "session: metadata for the created and attached session.".to_string(),
                ],
                example_args: vec![r#"{"session_id":"migrated-board"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "switch_session".to_string(),
                description: "Switch this MCP server instance to a named session.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "session_id".to_string(),
                        required: true,
                        description: "Target session id.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "create_if_missing".to_string(),
                        required: false,
                        description: "If true, create the session scene and manifest when they do not exist.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "active_session_id: current named session after the call.".to_string(),
                    "session: metadata for the activated session.".to_string(),
                ],
                example_args: vec![
                    r#"{"session_id":"agent-a"}"#.to_string(),
                    r#"{"session_id":"shared-review","create_if_missing":true}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "get_scene".to_string(),
                description: "Return the full active scene as structured JSON.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "SceneDocument with objects and selected_object_ids.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "get_universe".to_string(),
                description: "Return the full active universe document, including canvases, memberships, viewport state, and the embedded scene.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene file path.".to_string(),
                    "universe: complete serialized UniverseDocument.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "get_selected_objects".to_string(),
                description: "Return the current selection and full payloads for the selected objects.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "selected_object_ids: array of object ids.".to_string(),
                    "objects: array of the selected object payloads.".to_string(),
                    "object_count: total object count in the scene.".to_string(),
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene file path.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "get_selection_context".to_string(),
                description: "Return semantic context for the current selection, including canvas membership, connected objects, and nearby objects.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene file path.".to_string(),
                    "context: structured SelectionContext for the active selection.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "create_canvas".to_string(),
                description: "Create a named canvas frame in the active universe and make it active.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "name".to_string(),
                        required: true,
                        description: "Canvas title.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "rect".to_string(),
                        required: true,
                        description: "Canvas bounds in world coordinates.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "canvas_id: id of the new canvas.".to_string(),
                    "active_canvas_id / selected_canvas_ids: updated canvas focus state.".to_string(),
                    "canvases / memberships: updated universe metadata lists.".to_string(),
                ],
                example_args: vec![
                    r#"{"name":"Review","rect":{"x":0,"y":0,"width":240,"height":160}}"#
                        .to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "set_active_canvas".to_string(),
                description: "Set or clear the active canvas.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "canvas_id".to_string(),
                    required: false,
                    description: "Existing canvas id, or null to clear.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "active_canvas_id: updated active canvas.".to_string(),
                    "canvases / memberships: updated universe metadata lists.".to_string(),
                ],
                example_args: vec![
                    r#"{"canvas_id":"canvas-123"}"#.to_string(),
                    r#"{"canvas_id":null}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "select_canvases".to_string(),
                description: "Replace the current canvas selection with the provided canvas ids.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "canvas_ids".to_string(),
                    required: true,
                    description: "Array of existing canvas ids.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "selected_canvas_ids: updated canvas selection.".to_string(),
                    "canvases / memberships: updated universe metadata lists.".to_string(),
                ],
                example_args: vec![
                    r#"{"canvas_ids":["canvas-123"]}"#.to_string(),
                    r#"{"canvas_ids":["canvas-123","canvas-456"]}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "assign_object_to_canvas".to_string(),
                description: "Assign an existing object to a canvas frame.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "object_id".to_string(),
                        required: true,
                        description: "Existing object id.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "canvas_id".to_string(),
                        required: true,
                        description: "Existing canvas id.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id / object_canvas_id: updated membership for the object.".to_string(),
                    "canvases / memberships: updated universe metadata lists.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123","canvas_id":"canvas-123"}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "clear_object_canvas_assignment".to_string(),
                description: "Remove any canvas membership from an existing object.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_id".to_string(),
                    required: true,
                    description: "Existing object id with a current canvas assignment.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: object whose membership was cleared.".to_string(),
                    "object_canvas_id: null after success.".to_string(),
                    "canvases / memberships: updated universe metadata lists.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "export_scene_json".to_string(),
                description: "Return the full active scene as a pretty-printed JSON string that can be written directly to another file.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "scene_json: complete serialized scene string.".to_string(),
                    "object_count: total object count.".to_string(),
                    "selected_object_ids: current selection.".to_string(),
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene file path.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "render_scene_png".to_string(),
                description: "Render the whole active scene to a PNG file and return the output path plus crop metadata.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "path".to_string(),
                        required: false,
                        description: "Optional destination PNG path. If omitted, the server writes to a temporary file.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "padding".to_string(),
                        required: false,
                        description: "Optional padding in pixels to expand the crop bounds around the scene content.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "output_path: PNG file path.".to_string(),
                    "scope: scene.".to_string(),
                    "bounds: crop rectangle in scene coordinates.".to_string(),
                    "width and height: raster size.".to_string(),
                    "object_count: number of objects rendered.".to_string(),
                ],
                example_args: vec![
                    "{}".to_string(),
                    r#"{"path":"/tmp/boardmesh-scene.png","padding":24}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "render_selected_png".to_string(),
                description: "Render only the currently selected objects to a cropped PNG file.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "path".to_string(),
                        required: false,
                        description: "Optional destination PNG path. If omitted, the server writes to a temporary file.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "padding".to_string(),
                        required: false,
                        description: "Optional padding in pixels to expand the crop bounds around the selected objects.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "output_path: PNG file path.".to_string(),
                    "scope: selected.".to_string(),
                    "bounds: crop rectangle in scene coordinates.".to_string(),
                    "width and height: raster size.".to_string(),
                    "object_count: number of selected objects rendered.".to_string(),
                ],
                example_args: vec![
                    "{}".to_string(),
                    r#"{"path":"/tmp/boardmesh-selection.png","padding":16}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "get_tool_help".to_string(),
                description: "Return example payloads and parameter notes for one tool or for all tools.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "tool_name".to_string(),
                    required: false,
                    description: "Optional tool name filter. Omit it to get examples for every tool.".to_string(),
                }],
                returns: vec![
                    "server_summary: short overview.".to_string(),
                    "parameter_format_notes: shared parsing rules.".to_string(),
                    "tools: example payloads grouped by tool.".to_string(),
                ],
                example_args: vec![
                    "{}".to_string(),
                    r#"{"tool_name":"add_text"}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "get_api_docs".to_string(),
                description: "Return full documentation for every Boardmesh MCP command in one call.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "server_summary: high-level server description.".to_string(),
                    "conventions: shared request/response rules.".to_string(),
                    "tools: full per-tool documentation.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "save_scene_as".to_string(),
                description: "Write the current active scene to another file path.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "path".to_string(),
                        required: true,
                        description: "Destination path for the saved scene JSON file.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "make_active".to_string(),
                        required: false,
                        description: "If true, switch the MCP server to use the new file as the active scene.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "target_path: file written by the call.".to_string(),
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene after the call.".to_string(),
                    "object_count and selected_object_ids from the written scene.".to_string(),
                ],
                example_args: vec![
                    r#"{"path":"/tmp/diagram-copy.json"}"#.to_string(),
                    r#"{"path":"/tmp/diagram-copy.json","make_active":true}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "load_scene_json".to_string(),
                description: "Load a full scene JSON string into the active scene file or another target path.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "scene_json".to_string(),
                        required: true,
                        description: "A complete serialized SceneDocument string.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "path".to_string(),
                        required: false,
                        description: "Optional destination file. If omitted, the active scene file is replaced.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "make_active".to_string(),
                        required: false,
                        description: "If true, switch the MCP server to the destination file after writing.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "target_path: file written by the call.".to_string(),
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene after the call.".to_string(),
                    "object_count and selected_object_ids from the loaded scene.".to_string(),
                ],
                example_args: vec![
                    r#"{"scene_json":"{\"objects\":[],\"selected_object_ids\":[]}"}"#.to_string(),
                    r#"{"scene_json":"{\"objects\":[],\"selected_object_ids\":[]}","path":"/tmp/new-scene.json","make_active":true}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "switch_scene".to_string(),
                description: "Change which scene file the MCP server is currently reading and writing.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "path".to_string(),
                        required: true,
                        description: "Path to the scene JSON file to activate.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "create_if_missing".to_string(),
                        required: false,
                        description: "If true, create a default empty scene file when the path does not exist.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "target_path: the path switched to.".to_string(),
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene after the call.".to_string(),
                    "object_count and selected_object_ids from the loaded scene.".to_string(),
                ],
                example_args: vec![
                    r#"{"path":"/tmp/existing-scene.json"}"#.to_string(),
                    r#"{"path":"/tmp/new-scene.json","create_if_missing":true}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "clear_scene".to_string(),
                description: "Remove every object from the active scene.".to_string(),
                request_fields: vec![],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_count: 0 after clearing.".to_string(),
                    "selected_object_ids: empty after clearing.".to_string(),
                    "session_id: active named session for this server instance, if any.".to_string(),
                    "protocol_version: current session protocol version.".to_string(),
                    "scene_path: active scene file path.".to_string(),
                ],
                example_args: vec!["{}".to_string()],
            },
            ApiToolDoc {
                tool_name: "add_line".to_string(),
                description: "Add a straight line object.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "start".to_string(),
                        required: true,
                        description: "Line start point as CanvasPoint.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "end".to_string(),
                        required: true,
                        description: "Line end point as CanvasPoint.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "stroke".to_string(),
                        required: false,
                        description: "Optional stroke override. Defaults to the server's standard stroke.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new line.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"start":{"x":80,"y":120},"end":{"x":260,"y":120}}"#.to_string(),
                    r#"{"start":"80,120","end":"260,120","stroke":{"color":{"r":30,"g":41,"b":59,"a":255},"width":3}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "add_rectangle".to_string(),
                description: "Add a rectangle object.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "rect".to_string(),
                        required: true,
                        description: "Bounding rectangle as SceneRect.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "stroke".to_string(),
                        required: false,
                        description: "Optional stroke override.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "fill".to_string(),
                        required: false,
                        description: "Optional fill color.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new rectangle.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"rect":{"x":120,"y":140,"width":220,"height":100}}"#.to_string(),
                    r#"{"rect":"120,140,220,100","fill":{"r":255,"g":240,"b":180,"a":255}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "add_circle".to_string(),
                description: "Add a circle object.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "center".to_string(),
                        required: true,
                        description: "Circle center as CanvasPoint.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "radius".to_string(),
                        required: true,
                        description: "Circle radius in pixels.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "stroke".to_string(),
                        required: false,
                        description: "Optional stroke override.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "fill".to_string(),
                        required: false,
                        description: "Optional fill color.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new circle.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"center":{"x":420,"y":220},"radius":60}"#.to_string(),
                    r#"{"center":"420,220","radius":60,"fill":{"r":190,"g":230,"b":255,"a":255}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "add_triangle".to_string(),
                description: "Add a triangle using either a quick bounding rectangle or three explicit points. This is a convenience API over a 3-point shape.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "rect".to_string(),
                        required: false,
                        description: "Optional bounding rectangle as SceneRect. Use this for quick drag-style isosceles triangle creation.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "points".to_string(),
                        required: false,
                        description: "Optional array of exactly three CanvasPoint values. Use this for arbitrary or rotated triangles.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "stroke".to_string(),
                        required: false,
                        description: "Optional stroke override.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "fill".to_string(),
                        required: false,
                        description: "Optional fill color.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new triangle.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"rect":{"x":520,"y":140,"width":180,"height":120}}"#.to_string(),
                    r#"{"rect":"520,140,180,120","fill":{"r":255,"g":210,"b":120,"a":255}}"#.to_string(),
                    r#"{"points":["610,120","520,260","700,260"],"fill":{"r":255,"g":210,"b":120,"a":255}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "add_polygon".to_string(),
                description: "Add a polygon from an ordered list of points. Supplying exactly three points creates a free-form triangle without using the triangle convenience wrapper.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "points".to_string(),
                        required: true,
                        description: "Array of at least three CanvasPoint values in drawing order.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "stroke".to_string(),
                        required: false,
                        description: "Optional stroke override.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "fill".to_string(),
                        required: false,
                        description: "Optional fill color.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new polygon.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"points":[{"x":100,"y":80},{"x":220,"y":80},{"x":260,"y":180},{"x":140,"y":220}]}"#.to_string(),
                    r#"{"points":["100,80","220,80","260,180","140,220"],"fill":{"r":220,"g":245,"b":210,"a":255}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "add_stroke".to_string(),
                description: "Add a freehand stroke object from an ordered list of points.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "points".to_string(),
                        required: true,
                        description: "Array of at least two CanvasPoint values in drawing order.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "stroke".to_string(),
                        required: false,
                        description: "Optional stroke override.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new stroke.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"points":[{"x":80,"y":300},{"x":120,"y":320},{"x":180,"y":315},{"x":240,"y":340}]}"#.to_string(),
                    r#"{"points":["80,300","120,320","180,315","240,340"],"stroke":{"color":{"r":220,"g":38,"b":38,"a":255},"width":4}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "add_text".to_string(),
                description: "Add a text label object.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "position".to_string(),
                        required: true,
                        description: "Text anchor position as CanvasPoint.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "text".to_string(),
                        required: true,
                        description: "String content for the label.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "color".to_string(),
                        required: false,
                        description: "Optional text color.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: id of the new text object.".to_string(),
                    "selected_object_ids: set to the new object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"position":{"x":380,"y":392},"text":"API server"}"#.to_string(),
                    r#"{"position":"380,392","text":"API server","color":{"r":15,"g":23,"b":42,"a":255}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "fill_object".to_string(),
                description: "Set or replace the fill color of an existing object by id.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "object_id".to_string(),
                        required: true,
                        description: "Existing object id.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "fill".to_string(),
                        required: true,
                        description: "Fill color as SceneColor.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the updated object id.".to_string(),
                    "selected_object_ids: set to that object.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123","fill":{"r":255,"g":220,"b":120,"a":255}}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "duplicate_object".to_string(),
                description: "Duplicate an existing object by id with an optional offset.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "object_id".to_string(),
                        required: true,
                        description: "Existing object id.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "dx".to_string(),
                        required: false,
                        description: "Optional x offset in pixels. Defaults to 24.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "dy".to_string(),
                        required: false,
                        description: "Optional y offset in pixels. Defaults to 24.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the duplicated object id.".to_string(),
                    "selected_object_ids: set to the duplicate.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123"}"#.to_string(),
                    r#"{"object_id":"obj-123","dx":48,"dy":12}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "set_object_visibility".to_string(),
                description: "Hide or show an object by id.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "object_id".to_string(),
                        required: true,
                        description: "Existing object id.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "visible".to_string(),
                        required: true,
                        description: "Whether the object should remain visible.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the updated object id.".to_string(),
                    "object_count: scene object count remains unchanged.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123","visible":false}"#.to_string(),
                    r#"{"object_id":"obj-123","visible":true}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "set_object_locked".to_string(),
                description: "Lock or unlock an object against movement.".to_string(),
                request_fields: vec![
                    ApiFieldDoc {
                        name: "object_id".to_string(),
                        required: true,
                        description: "Existing object id.".to_string(),
                    },
                    ApiFieldDoc {
                        name: "locked".to_string(),
                        required: true,
                        description: "Whether the object should be locked.".to_string(),
                    },
                ],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the updated object id.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_id":"obj-123","locked":true}"#.to_string(),
                    r#"{"object_id":"obj-123","locked":false}"#.to_string(),
                ],
            },
            ApiToolDoc {
                tool_name: "move_object_to_front".to_string(),
                description: "Move an object to the front of the draw order.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_id".to_string(),
                    required: true,
                    description: "Existing object id.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the reordered object id.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "move_object_to_back".to_string(),
                description: "Move an object to the back of the draw order.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_id".to_string(),
                    required: true,
                    description: "Existing object id.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the reordered object id.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "move_object_forward".to_string(),
                description: "Move an object one step forward in the draw order.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_id".to_string(),
                    required: true,
                    description: "Existing object id.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the reordered object id.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "move_object_backward".to_string(),
                description: "Move an object one step backward in the draw order.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_id".to_string(),
                    required: true,
                    description: "Existing object id.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the reordered object id.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "delete_object".to_string(),
                description: "Delete a single object by id.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_id".to_string(),
                    required: true,
                    description: "Existing object id.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "object_id: the deleted object id.".to_string(),
                    "object_count: updated scene object count.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                ],
                example_args: vec![r#"{"object_id":"obj-123"}"#.to_string()],
            },
            ApiToolDoc {
                tool_name: "select_objects".to_string(),
                description: "Replace the current selection with the provided object ids.".to_string(),
                request_fields: vec![ApiFieldDoc {
                    name: "object_ids".to_string(),
                    required: true,
                    description: "Array of existing object ids.".to_string(),
                }],
                returns: vec![
                    "ok: true on success.".to_string(),
                    "selected_object_ids: the new selection.".to_string(),
                    "session_id / protocol_version: current session metadata.".to_string(),
                    "scene_path: active scene file path.".to_string(),
                ],
                example_args: vec![
                    r#"{"object_ids":["obj-123"]}"#.to_string(),
                    r#"{"object_ids":["obj-123","obj-456"]}"#.to_string(),
                ],
            },
        ]
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = parse_options()?;
    let scene = ensure_scene_file(&options.scene_path)?;
    log_mcp(&format!(
        "starting server session={:?} scene={} sessions_dir={} object_count={}",
        options.active_session_id,
        options.scene_path.display(),
        options.session_root.display(),
        scene.object_count()
    ));
    let server = BoardmeshServer::new(
        options.session_root,
        options.active_session_id,
        options.scene_path,
        scene,
    );
    let heartbeat_server = server.clone();
    tokio::spawn(async move {
        loop {
            let _ = heartbeat_server.write_server_heartbeat();
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
    if let Some(runtime_addr) = options.runtime_addr.clone() {
        let runtime_server = server.clone();
        tokio::spawn(async move {
            if let Err(error) = run_runtime_ws_server(runtime_server, runtime_addr).await {
                log_mcp(&format!("runtime websocket server stopped: {error:#}"));
            }
        });
    }
    log_mcp("stdio transport ready; waiting for MCP client requests");
    server
        .serve(rmcp::transport::io::stdio())
        .await?
        .waiting()
        .await?;
    Ok(())
}

async fn run_runtime_ws_server(
    server: BoardmeshServer,
    runtime_addr: String,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(&runtime_addr).await?;
    log_mcp(&format!(
        "runtime websocket listening on ws://{runtime_addr}"
    ));
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        let runtime_server = server.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_runtime_ws_connection(runtime_server, stream).await {
                log_mcp(&format!(
                    "runtime websocket client {peer_addr} disconnected: {error}"
                ));
            }
        });
    }
}

async fn handle_runtime_ws_connection(
    server: BoardmeshServer,
    stream: TcpStream,
) -> Result<(), String> {
    let peer = stream
        .peer_addr()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    let websocket = accept_async(stream)
        .await
        .map_err(|error| format!("websocket handshake failed: {error}"))?;
    log_mcp(&format!("runtime websocket client connected: {peer}"));
    let (mut writer, mut reader) = websocket.split();
    let mut runtime_events = server.runtime_events.subscribe();
    let mut connection = RuntimeConnectionState::default();

    loop {
        tokio::select! {
            incoming = reader.next() => {
                let Some(message) = incoming else {
                    return Ok(());
                };
                let message = message.map_err(|error| format!("websocket read failed: {error}"))?;
                match message {
                    Message::Text(text) => {
                        log_mcp(&format!("runtime websocket request from {peer}: {}", text.as_str()));
                        if let Some(response) = handle_runtime_ws_request(&server, &mut connection, text.as_str()).await? {
                            writer
                                .send(Message::Text(response))
                                .await
                                .map_err(|error| format!("websocket write failed: {error}"))?;
                        }
                    }
                    Message::Ping(payload) => {
                        writer
                            .send(Message::Pong(payload))
                            .await
                            .map_err(|error| format!("websocket pong failed: {error}"))?;
                    }
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
            event = runtime_events.recv() => {
                match event {
                    Ok(event) => {
                        if !connection.matches_event(&event) {
                            continue;
                        }
                        log_mcp(&format!(
                            "runtime websocket notify {peer}: method={} object_count={}",
                            METHOD_SCENE_UPDATED,
                            event.snapshot.scene.objects.len()
                        ));
                        let notification = JsonRpcNotification::new(METHOD_SCENE_UPDATED, event);
                        let payload = serde_json::to_string(&notification)
                            .map_err(|error| format!("failed to encode runtime notification: {error}"))?;
                        writer
                            .send(Message::Text(payload))
                            .await
                            .map_err(|error| format!("websocket write failed: {error}"))?;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        log_mcp(&format!("runtime websocket client lagged; skipped {skipped} notifications"));
                    }
                    Err(broadcast::error::RecvError::Closed) => return Ok(()),
                }
            }
        }
    }
}

async fn handle_runtime_ws_request(
    server: &BoardmeshServer,
    connection: &mut RuntimeConnectionState,
    payload: &str,
) -> Result<Option<String>, String> {
    let request: JsonRpcRequest = serde_json::from_str(payload)
        .map_err(|error| format!("invalid json-rpc request: {error}"))?;
    let response = match request.method.as_str() {
        METHOD_SESSION_ATTACH => {
            let params = decode_params::<SessionAttachParams>(request.params)?;
            let snapshot = server.attach_runtime_session(params)?;
            connection.update_from_snapshot(&snapshot);
            JsonRpcResponse::success(request.id, snapshot)
        }
        METHOD_SESSION_LIST => JsonRpcResponse::success(request.id, server.runtime_session_list()?),
        METHOD_SESSION_SWITCH => {
            let params = decode_params::<SessionSwitchParams>(request.params)?;
            let snapshot = server.switch_runtime_session(params)?;
            connection.update_from_snapshot(&snapshot);
            JsonRpcResponse::success(request.id, snapshot)
        }
        METHOD_SCENE_GET => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_snapshot_for_request(session_id.as_deref(), connection)?,
            )
        }
        METHOD_SCENE_REPLACE => {
            let params = decode_params::<SceneReplaceParams>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.replace_scene_from_runtime(connection, params)?,
            )
        }
        METHOD_LIST_SESSIONS => {
            JsonRpcResponse::success(request.id, server.list_sessions().await?.0)
        }
        METHOD_GET_SESSION_INFO => {
            JsonRpcResponse::success(request.id, server.get_session_info().await?.0)
        }
        METHOD_CREATE_SESSION => {
            let params = decode_params::<CreateSessionRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.create_session(Parameters(params)).await?.0,
            )
        }
        METHOD_CREATE_SESSION_FROM_SCENE => {
            let params = decode_params::<CreateSessionFromSceneRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server
                    .create_session_from_scene(Parameters(params))
                    .await?
                    .0,
            )
        }
        METHOD_SWITCH_SESSION => {
            let params = decode_params::<SwitchSessionRequest>(request.params)?;
            let (scene_path, manifest) = if params.create_if_missing {
                server.ensure_named_session(&params.session_id)?
            } else {
                let session_root = server.session_root()?;
                let scene_path = resolve_session_path(&session_root, &params.session_id)
                    .map_err(|error| format!("{error:#}"))?;
                let manifest_path = session_manifest_path(&session_root, &params.session_id);
                let manifest =
                    load_session_manifest(&manifest_path).map_err(|error| format!("{error:#}"))?;
                (scene_path, manifest)
            };
            connection.update_from_scene_file(
                Some(params.session_id.clone()),
                scene_path.display().to_string(),
            );
            JsonRpcResponse::success(
                request.id,
                SessionResult {
                    ok: true,
                    active_session_id: connection.session_id.clone(),
                    session: session_info_from_manifest(&manifest),
                },
            )
        }
        METHOD_GET_SCENE => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_get_scene(session_id.as_deref(), connection)?,
            )
        }
        METHOD_GET_UNIVERSE => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_get_universe(session_id.as_deref(), connection)?,
            )
        }
        METHOD_GET_SELECTED_OBJECTS => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_get_selected_objects(session_id.as_deref(), connection)?,
            )
        }
        METHOD_GET_SELECTION_CONTEXT => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_get_selection_context(session_id.as_deref(), connection)?,
            )
        }
        METHOD_CREATE_CANVAS => {
            let (session_id, params) = decode_scoped_params::<CreateCanvasRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_create_canvas(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_SET_ACTIVE_CANVAS => {
            let (session_id, params) =
                decode_scoped_params::<SetActiveCanvasRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_set_active_canvas(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_SELECT_CANVASES => {
            let (session_id, params) =
                decode_scoped_params::<SelectCanvasesRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_select_canvases(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ASSIGN_OBJECT_TO_CANVAS => {
            let (session_id, params) =
                decode_scoped_params::<AssignObjectToCanvasRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_assign_object_to_canvas(
                    session_id.as_deref(),
                    connection,
                    params,
                )?,
            )
        }
        METHOD_CLEAR_OBJECT_CANVAS_ASSIGNMENT => {
            let (session_id, params) =
                decode_scoped_params::<ClearObjectCanvasAssignmentRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_clear_object_canvas_assignment(
                    session_id.as_deref(),
                    connection,
                    params,
                )?,
            )
        }
        METHOD_EXPORT_SCENE_JSON => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_export_scene_json(session_id.as_deref(), connection)?,
            )
        }
        METHOD_RENDER_SCENE_PNG => {
            let (session_id, params) = decode_scoped_params::<ScreenshotRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_render_scene_png(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_RENDER_SELECTED_PNG => {
            let (session_id, params) = decode_scoped_params::<ScreenshotRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_render_selected_png(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_GET_TOOL_HELP => {
            let params = decode_params::<ToolHelpRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.get_tool_help(Parameters(params)).await?.0,
            )
        }
        METHOD_GET_API_DOCS => JsonRpcResponse::success(request.id, server.get_api_docs().await?.0),
        METHOD_SAVE_SCENE_AS => {
            let (session_id, params) = decode_scoped_params::<SaveSceneAsRequest>(request.params)?;
            let result = server.runtime_save_scene_as(session_id.as_deref(), connection, params)?;
            if result.scene_path == result.target_path {
                connection
                    .update_from_scene_file(result.session_id.clone(), result.scene_path.clone());
            }
            JsonRpcResponse::success(request.id, result)
        }
        METHOD_LOAD_SCENE_JSON => {
            let (session_id, params) =
                decode_scoped_params::<LoadSceneJsonRequest>(request.params)?;
            let result =
                server.runtime_load_scene_json(session_id.as_deref(), connection, params)?;
            connection.update_from_scene_file(result.session_id.clone(), result.scene_path.clone());
            JsonRpcResponse::success(request.id, result)
        }
        METHOD_SWITCH_SCENE => {
            let (session_id, params) = decode_scoped_params::<SwitchSceneRequest>(request.params)?;
            let result = server.runtime_switch_scene(session_id.as_deref(), connection, params)?;
            connection.update_from_scene_file(result.session_id.clone(), result.scene_path.clone());
            JsonRpcResponse::success(request.id, result)
        }
        METHOD_CLEAR_SCENE => {
            let session_id = decode_runtime_scope(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_clear_scene(session_id.as_deref(), connection)?,
            )
        }
        METHOD_ADD_LINE => {
            let (session_id, params) = decode_scoped_params::<AddLineRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_line(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ADD_RECTANGLE => {
            let (session_id, params) = decode_scoped_params::<AddRectangleRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_rectangle(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ADD_CIRCLE => {
            let (session_id, params) = decode_scoped_params::<AddCircleRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_circle(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ADD_TRIANGLE => {
            let (session_id, params) = decode_scoped_params::<AddTriangleRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_triangle(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ADD_POLYGON => {
            let (session_id, params) = decode_scoped_params::<AddPolygonRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_polygon(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ADD_STROKE => {
            let (session_id, params) = decode_scoped_params::<AddStrokeRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_stroke(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_ADD_TEXT => {
            let (session_id, params) = decode_scoped_params::<AddTextRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_add_text(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_FILL_OBJECT => {
            let (session_id, params) = decode_scoped_params::<FillObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_fill_object(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_DUPLICATE_OBJECT => {
            let (session_id, params) =
                decode_scoped_params::<DuplicateObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_duplicate_object(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_SET_OBJECT_VISIBILITY => {
            let (session_id, params) =
                decode_scoped_params::<SetObjectVisibilityRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_set_object_visibility(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_SET_OBJECT_LOCKED => {
            let (session_id, params) =
                decode_scoped_params::<SetObjectLockedRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_set_object_locked(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_MOVE_OBJECT_TO_FRONT => {
            let (session_id, params) =
                decode_scoped_params::<ReorderObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_move_object_to_front(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_MOVE_OBJECT_TO_BACK => {
            let (session_id, params) =
                decode_scoped_params::<ReorderObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_move_object_to_back(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_MOVE_OBJECT_FORWARD => {
            let (session_id, params) =
                decode_scoped_params::<ReorderObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_move_object_forward(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_MOVE_OBJECT_BACKWARD => {
            let (session_id, params) =
                decode_scoped_params::<ReorderObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_move_object_backward(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_DELETE_OBJECT => {
            let (session_id, params) = decode_scoped_params::<DeleteObjectRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_delete_object(session_id.as_deref(), connection, params)?,
            )
        }
        METHOD_SELECT_OBJECTS => {
            let (session_id, params) =
                decode_scoped_params::<SelectObjectsRequest>(request.params)?;
            JsonRpcResponse::success(
                request.id,
                server.runtime_select_objects(session_id.as_deref(), connection, params)?,
            )
        }
        other => JsonRpcResponse::failure(request.id, -32601, format!("unknown method: {other}")),
    };
    serde_json::to_string(&response)
        .map(Some)
        .map_err(|error| format!("failed to encode json-rpc response: {error}"))
}

fn decode_params<T: serde::de::DeserializeOwned>(params: Option<Value>) -> Result<T, String> {
    serde_json::from_value(params.unwrap_or(Value::Null))
        .map_err(|error| format!("invalid params: {error}"))
}

fn decode_runtime_scope(params: Option<Value>) -> Result<Option<String>, String> {
    let Some(value) = params else {
        return Ok(None);
    };
    let Some(object) = value.as_object() else {
        return Err("invalid params: expected an object".to_string());
    };
    match object.get("session_id") {
        Some(Value::Null) | None => Ok(None),
        Some(value) => serde_json::from_value::<String>(value.clone())
            .map(Some)
            .map_err(|error| format!("invalid params: {error}")),
    }
}

fn decode_scoped_params<T: serde::de::DeserializeOwned>(
    params: Option<Value>,
) -> Result<(Option<String>, T), String> {
    let session_id = decode_runtime_scope(params.clone())?;
    let mut value = params.unwrap_or(Value::Object(Default::default()));
    if let Some(object) = value.as_object_mut() {
        object.remove("session_id");
    }
    let decoded =
        serde_json::from_value(value).map_err(|error| format!("invalid params: {error}"))?;
    Ok((session_id, decoded))
}

fn parse_options() -> anyhow::Result<ServerOptions> {
    let mut session_root = PathBuf::from(boardmesh::DEFAULT_SESSIONS_DIR);
    let mut explicit_scene_path = None;
    let mut requested_session_id = None;
    let mut runtime_addr = Some(DEFAULT_RUNTIME_ADDR.to_string());
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--sessions-dir" {
            if let Some(path) = args.next() {
                session_root = PathBuf::from(path);
            }
            continue;
        }

        if argument == "--runtime-addr" {
            if let Some(addr) = args.next() {
                runtime_addr = Some(addr);
            }
            continue;
        }

        if argument == "--no-runtime-ws" {
            runtime_addr = None;
            continue;
        }

        if argument == "--scene" {
            if let Some(path) = args.next() {
                explicit_scene_path = Some(PathBuf::from(path));
                requested_session_id = None;
            }
            continue;
        }

        if argument == "--session"
            && let Some(session_id) = args.next()
        {
            requested_session_id = Some(session_id);
        }
    }

    let scene_path = match (&requested_session_id, explicit_scene_path) {
        (_, Some(scene_path)) => scene_path,
        (Some(session_id), None) => resolve_session_path(&session_root, session_id)?,
        (None, None) => PathBuf::from(boardmesh::DEFAULT_SCENE_FILE),
    };
    Ok(ServerOptions {
        session_root,
        active_session_id: requested_session_id,
        scene_path,
        runtime_addr,
    })
}

impl AddTriangleRequest {
    fn triangle(&self) -> Result<TriangleObject, String> {
        match (&self.rect, &self.points) {
            (Some(rect), None) => Ok(TriangleObject::from_rect(*rect)),
            (None, Some(points)) if points.len() == 3 => {
                Ok(TriangleObject::new([points[0], points[1], points[2]]))
            }
            (None, Some(_)) => Err("triangle points must contain exactly three points".to_string()),
            (Some(_), Some(_)) => {
                Err("provide either rect or points for triangle, not both".to_string())
            }
            (None, None) => Err("triangle requires rect or points".to_string()),
        }
    }
}

fn default_screenshot_padding() -> i32 {
    24
}

fn default_duplicate_offset() -> i32 {
    24
}

fn screenshot_target_path(path: Option<&str>, prefix: &str) -> PathBuf {
    match path {
        Some(path) => PathBuf::from(path),
        None => std::env::temp_dir().join(format!("boardmesh-{prefix}-{}.png", Uuid::new_v4())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boardmesh::TextObject;
    use std::sync::mpsc;
    use std::time::Duration as StdDuration;

    fn test_server() -> BoardmeshServer {
        let root = std::env::temp_dir().join(format!("boardmesh-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("test session root");
        let scene_path = root.join("default.json");
        ensure_scene_file(&scene_path).expect("test scene file");
        BoardmeshServer::new(root, None, scene_path, SceneDocument::default())
    }

    #[test]
    fn runtime_replace_scene_returns_and_broadcasts_update() {
        let server = test_server();
        let mut runtime_events = server.runtime_events.subscribe();
        let mut replacement = SceneDocument::default();
        replacement.add_text(
            TextObject {
                position: CanvasPoint::new(120, 120),
                content: "runtime smoke".to_string(),
            },
            SceneColor::rgb(10, 120, 220),
        );
        let expected_objects = replacement.object_count();

        let (result_tx, result_rx) = mpsc::channel();
        let server_for_call = server.clone();
        let replacement_for_call = replacement.clone();
        std::thread::spawn(move || {
            let result = server_for_call.replace_scene_from_runtime(
                &RuntimeConnectionState::default(),
                SceneReplaceParams {
                    session_id: None,
                    scene: replacement_for_call,
                },
            );
            let _ = result_tx.send(result);
        });

        let snapshot = result_rx
            .recv_timeout(StdDuration::from_secs(2))
            .expect("runtime replace should return promptly")
            .expect("runtime replace should succeed");
        assert_eq!(snapshot.scene.object_count(), expected_objects);

        let (event_tx, event_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = event_tx.send(runtime_events.blocking_recv());
        });
        let event = event_rx
            .recv_timeout(StdDuration::from_secs(2))
            .expect("runtime replace should broadcast an update")
            .expect("runtime broadcast channel should stay open");
        assert_eq!(event.source, "mcp");
        assert_eq!(event.snapshot.scene.object_count(), expected_objects);
    }

    #[test]
    fn runtime_attach_does_not_switch_shared_workspace() {
        let server = test_server();
        let original_scope = server.current_runtime_scope().expect("current scope");
        server
            .ensure_named_session("alpha")
            .expect("create session alpha");

        let snapshot = server
            .attach_runtime_session(SessionAttachParams {
                session_id: Some("alpha".to_string()),
                create_if_missing: false,
            })
            .expect("attach runtime session");

        assert_eq!(snapshot.session_id.as_deref(), Some("alpha"));
        assert_eq!(
            server
                .current_runtime_scope()
                .expect("shared scope after attach"),
            original_scope
        );
    }

    #[test]
    fn runtime_replace_scene_uses_connection_scope_or_explicit_session() {
        let server = test_server();
        server
            .ensure_named_session("alpha")
            .expect("create session alpha");
        server
            .ensure_named_session("beta")
            .expect("create session beta");

        let session_root = server.session_root().expect("session root");
        let alpha_scope = RuntimeConnectionState {
            session_id: Some("alpha".to_string()),
            scene_path: Some(resolve_session_path(&session_root, "alpha").expect("alpha path")),
        };

        let mut alpha_scene = SceneDocument::default();
        alpha_scene.add_text(
            TextObject {
                position: CanvasPoint::new(10, 10),
                content: "alpha".to_string(),
            },
            SceneColor::rgb(1, 2, 3),
        );
        server
            .replace_scene_from_runtime(
                &alpha_scope,
                SceneReplaceParams {
                    session_id: None,
                    scene: alpha_scene.clone(),
                },
            )
            .expect("replace alpha scene");

        let mut beta_scene = SceneDocument::default();
        beta_scene.add_text(
            TextObject {
                position: CanvasPoint::new(20, 20),
                content: "beta".to_string(),
            },
            SceneColor::rgb(4, 5, 6),
        );
        server
            .replace_scene_from_runtime(
                &RuntimeConnectionState::default(),
                SceneReplaceParams {
                    session_id: Some("beta".to_string()),
                    scene: beta_scene.clone(),
                },
            )
            .expect("replace beta scene");

        let loaded_alpha =
            load_scene(&resolve_session_path(&session_root, "alpha").expect("alpha path"))
                .expect("load alpha scene");
        let loaded_beta =
            load_scene(&resolve_session_path(&session_root, "beta").expect("beta path"))
                .expect("load beta scene");

        assert_eq!(loaded_alpha, alpha_scene);
        assert_eq!(loaded_beta, beta_scene);
    }

    #[test]
    fn runtime_connection_filters_events_by_attached_session() {
        let alpha_state = RuntimeConnectionState {
            session_id: Some("alpha".to_string()),
            scene_path: Some(PathBuf::from("/tmp/alpha.json")),
        };
        let beta_event = SceneUpdatedParams {
            source: "mcp".to_string(),
            snapshot: RuntimeSceneSnapshot {
                session_id: Some("beta".to_string()),
                scene_path: "/tmp/beta.json".to_string(),
                manifest_path: Some("/tmp/beta.manifest.json".to_string()),
                scene: SceneDocument::default(),
                protocol_version: SESSION_PROTOCOL_VERSION,
                runtime_api_version: RUNTIME_API_VERSION,
            },
        };
        let alpha_event = SceneUpdatedParams {
            source: "mcp".to_string(),
            snapshot: RuntimeSceneSnapshot {
                session_id: Some("alpha".to_string()),
                scene_path: "/tmp/alpha.json".to_string(),
                manifest_path: Some("/tmp/alpha.manifest.json".to_string()),
                scene: SceneDocument::default(),
                protocol_version: SESSION_PROTOCOL_VERSION,
                runtime_api_version: RUNTIME_API_VERSION,
            },
        };

        assert!(!alpha_state.matches_event(&beta_event));
        assert!(alpha_state.matches_event(&alpha_event));
    }

    #[test]
    fn runtime_duplicate_object_selects_the_duplicate() {
        let server = test_server();
        let (object_id, scene_path) = {
            let mut workspace = server.workspace.lock().expect("workspace");
            let object_id = workspace
                .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
                .expect("add rectangle");
            (object_id, workspace.scene_path().to_path_buf())
        };

        let result = server
            .runtime_duplicate_object(
                None,
                &RuntimeConnectionState::default(),
                DuplicateObjectRequest {
                    object_id: object_id.clone(),
                    dx: 24,
                    dy: 16,
                },
            )
            .expect("duplicate object");

        let duplicate_id = result.object_id.expect("duplicate id");
        assert_ne!(duplicate_id, object_id);
        assert_eq!(result.selected_object_ids, vec![duplicate_id.clone()]);
        let scene = load_scene(&scene_path).expect("load duplicated scene");
        assert_eq!(scene.object_count(), 2);
        assert!(scene.objects.iter().any(|object| object.id == duplicate_id));
    }

    #[test]
    fn runtime_visibility_lock_and_z_order_actions_mutate_the_scene() {
        let server = test_server();
        let (back_id, front_id, scene_path) = {
            let mut workspace = server.workspace.lock().expect("workspace");
            let back_id = workspace
                .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
                .expect("add rectangle");
            let front_id = workspace
                .add_circle(CanvasPoint::new(80, 60), 20, SceneStroke::default(), None)
                .expect("add circle");
            (back_id, front_id, workspace.scene_path().to_path_buf())
        };

        server
            .runtime_set_object_visibility(
                None,
                &RuntimeConnectionState::default(),
                SetObjectVisibilityRequest {
                    object_id: front_id.clone(),
                    visible: false,
                },
            )
            .expect("hide object");
        server
            .runtime_set_object_locked(
                None,
                &RuntimeConnectionState::default(),
                SetObjectLockedRequest {
                    object_id: back_id.clone(),
                    locked: true,
                },
            )
            .expect("lock object");
        server
            .runtime_move_object_to_front(
                None,
                &RuntimeConnectionState::default(),
                ReorderObjectRequest {
                    object_id: back_id.clone(),
                },
            )
            .expect("move to front");

        let scene = load_scene(&scene_path).expect("load reordered scene");
        let back = scene
            .objects
            .iter()
            .find(|object| object.id == back_id)
            .expect("back object");
        let front = scene
            .objects
            .iter()
            .find(|object| object.id == front_id)
            .expect("front object");
        assert!(back.locked);
        assert!(!front.visible);
        assert_eq!(
            scene.objects.last().map(|object| object.id.as_str()),
            Some(back_id.as_str())
        );
    }

    #[test]
    fn runtime_canvas_actions_persist_universe_metadata() {
        let server = test_server();
        let (object_id, scene_path) = {
            let mut workspace = server.workspace.lock().expect("workspace");
            let object_id = workspace
                .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
                .expect("add rectangle");
            (object_id, workspace.scene_path().to_path_buf())
        };

        let initial = server
            .runtime_get_universe(None, &RuntimeConnectionState::default())
            .expect("get universe");
        assert!(initial.universe.canvases.is_empty());

        let created = server
            .runtime_create_canvas(
                None,
                &RuntimeConnectionState::default(),
                CreateCanvasRequest {
                    name: "Review".to_string(),
                    rect: SceneRect::new(0, 0, 240, 160),
                },
            )
            .expect("create canvas");
        let canvas_id = created.canvas_id.expect("canvas id");
        assert_eq!(
            created.active_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );

        let selected = server
            .runtime_select_canvases(
                None,
                &RuntimeConnectionState::default(),
                SelectCanvasesRequest {
                    canvas_ids: vec![canvas_id.clone()],
                },
            )
            .expect("select canvases");
        assert_eq!(selected.selected_canvas_ids, vec![canvas_id.clone()]);

        let assigned = server
            .runtime_assign_object_to_canvas(
                None,
                &RuntimeConnectionState::default(),
                AssignObjectToCanvasRequest {
                    object_id: object_id.clone(),
                    canvas_id: canvas_id.clone(),
                },
            )
            .expect("assign object to canvas");
        assert_eq!(
            assigned.object_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );

        let universe = load_universe(&scene_path).expect("load universe");
        assert_eq!(
            universe.active_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );
        assert_eq!(universe.selected_canvas_ids, vec![canvas_id.clone()]);
        assert_eq!(
            universe.object_canvas_id(&object_id),
            Some(canvas_id.as_str())
        );

        let cleared = server
            .runtime_clear_object_canvas_assignment(
                None,
                &RuntimeConnectionState::default(),
                ClearObjectCanvasAssignmentRequest {
                    object_id: object_id.clone(),
                },
            )
            .expect("clear object canvas assignment");
        assert_eq!(cleared.object_canvas_id, None);

        let updated = load_universe(&scene_path).expect("load updated universe");
        assert_eq!(updated.object_canvas_id(&object_id), None);
    }

    #[test]
    fn runtime_active_canvas_can_be_cleared() {
        let server = test_server();
        let canvas_id = server
            .runtime_create_canvas(
                None,
                &RuntimeConnectionState::default(),
                CreateCanvasRequest {
                    name: "Main".to_string(),
                    rect: SceneRect::new(0, 0, 120, 80),
                },
            )
            .expect("create canvas")
            .canvas_id
            .expect("canvas id");

        let cleared = server
            .runtime_set_active_canvas(
                None,
                &RuntimeConnectionState::default(),
                SetActiveCanvasRequest { canvas_id: None },
            )
            .expect("clear active canvas");
        assert_eq!(cleared.active_canvas_id, None);

        let universe = server
            .runtime_get_universe(None, &RuntimeConnectionState::default())
            .expect("get universe");
        assert!(
            universe
                .universe
                .canvases
                .iter()
                .any(|canvas| canvas.id == canvas_id)
        );
        assert_eq!(universe.universe.active_canvas_id, None);
    }

    #[test]
    fn runtime_get_selection_context_resolves_canvas_links_and_nearby_objects() {
        let server = test_server();
        let (source_id, nearby_id, target_id, scene_path) = {
            let mut workspace = server.workspace.lock().expect("workspace");
            let source_id = workspace
                .add_rectangle(SceneRect::new(20, 20, 80, 50), SceneStroke::default(), None)
                .expect("add source");
            let nearby_id = workspace
                .add_circle(CanvasPoint::new(150, 50), 24, SceneStroke::default(), None)
                .expect("add nearby");
            let target_id = workspace
                .add_rectangle(
                    SceneRect::new(360, 40, 90, 60),
                    SceneStroke::default(),
                    None,
                )
                .expect("add target");
            workspace
                .add_link(
                    boardmesh::LinkObject {
                        source: boardmesh::LinkEnd {
                            endpoint: boardmesh::ConnectorEndpoint::Bound {
                                object_id: source_id.clone(),
                                anchor: boardmesh::ConnectorAnchor::East,
                                offset: CanvasPoint::new(0, 0),
                            },
                            label: Some("from".to_string()),
                        },
                        target: boardmesh::LinkEnd {
                            endpoint: boardmesh::ConnectorEndpoint::Bound {
                                object_id: target_id.clone(),
                                anchor: boardmesh::ConnectorAnchor::West,
                                offset: CanvasPoint::new(0, 0),
                            },
                            label: Some("to".to_string()),
                        },
                        routing: boardmesh::LinkRoutingPreference::Straight,
                        label: Some("references".to_string()),
                    },
                    SceneStroke::default(),
                )
                .expect("add link");
            assert!(
                workspace
                    .select_objects(vec![source_id.clone()])
                    .expect("select source")
            );
            (
                source_id,
                nearby_id,
                target_id,
                workspace.scene_path().to_path_buf(),
            )
        };

        let mut universe = load_universe(&scene_path).expect("load universe");
        let review_canvas = universe.create_canvas("Review", SceneRect::new(0, 0, 320, 220));
        assert!(universe.assign_object_to_canvas(&source_id, &review_canvas));
        save_universe(&scene_path, &universe).expect("save universe");

        let context = server
            .runtime_get_selection_context(None, &RuntimeConnectionState::default())
            .expect("selection context");
        assert_eq!(
            context.context.primary_object_id.as_deref(),
            Some(source_id.as_str())
        );
        assert_eq!(context.context.selected_objects.len(), 1);
        let primary = &context.context.selected_objects[0];
        assert_eq!(primary.object_kind, "rectangle");
        assert_eq!(primary.canvas_id.as_deref(), Some(review_canvas.as_str()));
        assert!(primary.connected_object_ids.contains(&target_id));
        assert!(primary.nearby_object_ids.contains(&nearby_id));
    }
}
