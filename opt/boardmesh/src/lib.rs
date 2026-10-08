pub mod app;
pub mod app_results;
pub mod gui_layout;
pub mod history;
pub mod ports;
pub mod render;
pub mod runtime;
pub mod runtime_client;
pub mod scene;
pub mod selection_context;
pub mod session;
pub mod storage;
pub mod universe;

pub use app::BoardWorkspace;
pub use app_results::{
    AdapterSessionListResult, CanvasMutationResult, MutationResult, SceneFileResult,
    SceneJsonResult, ScreenshotResult, SelectedObjectsResult, SelectionContextResult, SessionInfo,
    SessionMigrationResult, SessionResult, SessionServerInfo, SessionWindowInfo, UniverseResult,
    mutation_result, session_info_from_manifest,
};
pub use gui_layout::{
    LayoutAction, LayoutContext, LayoutState, ToolbarDock, apply_contextual_layout_hints,
    apply_layout_action, set_toolbar_cluster_visibility,
};
pub use history::{HistoryEntry, SceneHistory};
pub use ports::{
    FileSystemWorkspaceStorage, NoopWorkspaceEventPublisher, WorkspaceEvent, WorkspaceEventKind,
    WorkspaceEventPublisher, WorkspaceStoragePort,
};
pub use render::{RenderResult, RenderScope, render_scene_png, render_scene_region_png};
pub use runtime::{
    DEFAULT_RUNTIME_ADDR, DEFAULT_RUNTIME_URL, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, METHOD_ADD_CIRCLE, METHOD_ADD_LINE, METHOD_ADD_POLYGON, METHOD_ADD_RECTANGLE,
    METHOD_ADD_STROKE, METHOD_ADD_TEXT, METHOD_ADD_TRIANGLE, METHOD_ASSIGN_OBJECT_TO_CANVAS,
    METHOD_CLEAR_OBJECT_CANVAS_ASSIGNMENT, METHOD_CLEAR_SCENE, METHOD_CREATE_CANVAS,
    METHOD_CREATE_SESSION, METHOD_CREATE_SESSION_FROM_SCENE, METHOD_DELETE_OBJECT,
    METHOD_DUPLICATE_OBJECT, METHOD_EXPORT_SCENE_JSON, METHOD_FILL_OBJECT, METHOD_GET_API_DOCS,
    METHOD_GET_SCENE, METHOD_GET_SELECTED_OBJECTS, METHOD_GET_SELECTION_CONTEXT,
    METHOD_GET_SESSION_INFO, METHOD_GET_TOOL_HELP, METHOD_GET_UNIVERSE, METHOD_LIST_SESSIONS,
    METHOD_LOAD_SCENE_JSON, METHOD_MOVE_OBJECT_BACKWARD, METHOD_MOVE_OBJECT_FORWARD,
    METHOD_MOVE_OBJECT_TO_BACK, METHOD_MOVE_OBJECT_TO_FRONT, METHOD_RENDER_SCENE_PNG,
    METHOD_RENDER_SELECTED_PNG, METHOD_SAVE_SCENE_AS, METHOD_SCENE_GET, METHOD_SCENE_REPLACE,
    METHOD_SCENE_UPDATED, METHOD_SELECT_CANVASES, METHOD_SELECT_OBJECTS, METHOD_SESSION_ATTACH,
    METHOD_SESSION_LIST, METHOD_SESSION_SWITCH, METHOD_SET_ACTIVE_CANVAS, METHOD_SET_OBJECT_LOCKED,
    METHOD_SET_OBJECT_VISIBILITY, METHOD_SWITCH_SCENE, METHOD_SWITCH_SESSION, RUNTIME_METHODS,
    RuntimeSceneSnapshot, SceneReplaceParams, SceneUpdatedParams, SessionAttachParams,
    SessionListEntry, SessionListResult as RuntimeSessionListResult, SessionSwitchParams,
};
pub use runtime_client::runtime_request;
pub use scene::{
    CanvasPoint, ConnectorAnchor, ConnectorEndpoint, ConnectorSpec, DrawableObject, LinkEnd,
    LinkObject, LinkRoutingPreference, ObjectKind, ResolvedConnectorPath,
    ResolvedCubicBezierSegment, SceneColor, SceneConnectorKind, SceneDocument, SceneRect,
    SceneStroke, TextObject,
};
pub use selection_context::{ObjectSelectionContext, SelectionContext, SelectionContextKind};
pub use session::{
    AttachedServer, AttachedWindow, RUNTIME_API_VERSION, SERVER_HEARTBEAT_STALE_MS,
    SESSION_PROTOCOL_VERSION, SessionManifest, WindowState, create_named_session_from_scene,
    load_session_manifest, save_session_manifest, session_manifest_path, unix_time_ms,
};
pub use storage::{
    ensure_scene_file, ensure_universe_file, last_modified, list_session_ids, load_scene,
    load_scene_if_changed, load_universe, resolve_session_path, save_scene, save_universe,
    session_id_from_path, validate_session_id,
};
pub use universe::{
    CanvasExportPlan, CanvasFrame, CanvasMembership, UniverseDocument, UniverseMigration,
    ViewportState,
};

pub const DEFAULT_SCENE_FILE: &str = "boardmesh-scene.json";
pub const DEFAULT_SESSIONS_DIR: &str = "boardmesh-sessions";
