use boardmesh::scene::{
    CanvasPoint, DrawableObject, ObjectKind, ResolvedConnectorPath, SceneColor, SceneDocument,
    SceneRect, SceneStroke, TriangleObject,
};
use boardmesh::{
    AttachedServer, AttachedWindow, BoardWorkspace, DEFAULT_RUNTIME_URL, JsonRpcNotification,
    JsonRpcRequest, LayoutAction, LayoutContext, LayoutState, METHOD_SCENE_REPLACE,
    METHOD_SCENE_UPDATED, METHOD_SESSION_ATTACH, METHOD_SESSION_LIST, METHOD_SESSION_SWITCH,
    RUNTIME_API_VERSION, RuntimeSceneSnapshot, RuntimeSessionListResult, SERVER_HEARTBEAT_STALE_MS,
    SESSION_PROTOCOL_VERSION, SceneHistory, SceneReplaceParams, SessionAttachParams,
    SessionManifest, SessionSwitchParams, ToolbarDock, ViewportState, WindowState,
    apply_contextual_layout_hints, apply_layout_action, ensure_scene_file, last_modified,
    list_session_ids, load_scene_if_changed, load_session_manifest, resolve_session_path,
    runtime_request, save_session_manifest, session_manifest_path, unix_time_ms,
    validate_session_id,
};
use mesh_graph::GraphPoint;
use mesh_id::new_prefixed_id;
use std::cell::{Cell, Ref, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Mutex, OnceLock, mpsc};
use std::thread;
use std::time::SystemTime;
use tungstenite::{Message as WsMessage, connect as ws_connect};
use uuid::Uuid;
use wxdragon::menus::menuitem::{ID_ABOUT, ID_EXIT, ItemKind as MenuItemKind};
use wxdragon::prelude::*;

#[path = "boardmesh/boardmesh_controller.rs"]
mod boardmesh_controller;
#[path = "boardmesh/boardmesh_view.rs"]
mod boardmesh_view;

use boardmesh_controller::{
    apply_history_action, apply_selection_box, cancel_active_draft, clear_selection,
    delete_selected_objects, finalize_multi_point_draft, redo_last_change, select_all_objects,
    topmost_selectable_object_id, undo_last_change,
};
use boardmesh_view::{
    scene_summary_label, selection_summary_text, session_title_fragment, short_server_status_label,
    show_about_dialog, show_info_dialog, tool_state_summary,
};

const TOOL_ORDER: [Tool; 8] = [
    Tool::Select,
    Tool::Pen,
    Tool::Line,
    Tool::Rectangle,
    Tool::Circle,
    Tool::Triangle,
    Tool::Polygon,
    Tool::Fill,
];
const FRAME_WIDTH: i32 = 1520;
const FRAME_HEIGHT: i32 = 940;
const SESSION_PANEL_WIDTH: i32 = 320;
const INSPECTOR_PANEL_WIDTH: i32 = 320;
const OBJECTS_PANEL_WIDTH: i32 = 300;
const HISTORY_PANEL_HEIGHT: i32 = 220;
const SECTION_CONTENT_WIDTH: i32 = 276;
const CANVAS_MIN_WIDTH: i32 = 980;
const CANVAS_MIN_HEIGHT: i32 = 760;
const CANVAS_GRID_SPACING: i32 = 40;
const WORKSPACE_TOOLBAR_WIDTH: i32 = 760;
const WORKSPACE_TOOLBAR_HEIGHT: i32 = 64;
const SIDE_TOOLBAR_WIDTH: i32 = 96;
const TOOLS_TOOLBAR_HEIGHT: i32 = 460;
const OBJECTS_TOOLBAR_HEIGHT: i32 = 300;
const TOOLBAR_BUTTON_SIZE: i32 = 38;
const TOOLBAR_ICON_SIZE: i32 = 18;
const MENU_ICON_SIZE: i32 = 16;
const TOOLBAR_SECTION_LABEL_HEIGHT: i32 = 16;
const TOOLBAR_DIVIDER_THICKNESS: i32 = 2;
const UNDO_HISTORY_LIMIT: usize = 100;
const WXK_BACKSPACE: i32 = 8;
const WXK_DELETE: i32 = 127;
const WXK_RETURN: i32 = 13;
const WXK_ESCAPE: i32 = 27;
const WXK_Z: i32 = 90;
const WXK_Y: i32 = 89;
const WXK_0: i32 = 48;
const WXK_1: i32 = 49;
const WXK_2: i32 = 50;
const MENU_ID_OPEN_SESSION: i32 = 4101;
const MENU_ID_PIN_TOOLBARS: i32 = 4102;
const MENU_ID_SHOW_WORKSPACE_TOOLBAR: i32 = 4103;
const MENU_ID_SHOW_TOOLS_TOOLBAR: i32 = 4104;
const MENU_ID_SHOW_OBJECTS_TOOLBAR: i32 = 4105;
const MENU_ID_SHOW_SESSION_PANE: i32 = 4106;
const MENU_ID_SHOW_OBJECTS_PANE: i32 = 4107;
const MENU_ID_SHOW_INSPECTOR_PANE: i32 = 4108;
const MENU_ID_SHOW_HISTORY_PANE: i32 = 4109;
const MENU_ID_RESET_LAYOUT: i32 = 4110;
const MENU_ID_HELP_REFERENCE: i32 = 4111;
const MENU_ID_HELP_SHORTCUTS: i32 = 4112;
const MENU_ID_WORKSPACE_TOOLBAR_TOP: i32 = 4113;
const MENU_ID_WORKSPACE_TOOLBAR_LEFT: i32 = 4114;
const MENU_ID_WORKSPACE_TOOLBAR_RIGHT: i32 = 4115;
const MENU_ID_WORKSPACE_TOOLBAR_BOTTOM: i32 = 4116;
const MENU_ID_TOOLS_TOOLBAR_TOP: i32 = 4117;
const MENU_ID_TOOLS_TOOLBAR_LEFT: i32 = 4118;
const MENU_ID_TOOLS_TOOLBAR_RIGHT: i32 = 4119;
const MENU_ID_TOOLS_TOOLBAR_BOTTOM: i32 = 4120;
const MENU_ID_OBJECTS_TOOLBAR_TOP: i32 = 4121;
const MENU_ID_OBJECTS_TOOLBAR_LEFT: i32 = 4122;
const MENU_ID_OBJECTS_TOOLBAR_RIGHT: i32 = 4123;
const MENU_ID_OBJECTS_TOOLBAR_BOTTOM: i32 = 4124;
const HELP_REFERENCE_TEXT: &str = "Reference\n\n\
- The canvas is the source of truth. Local edits and MCP calls both write to the same scene file.\n\
- Use the View menu to show, hide, or reposition the toolbars around the frame.\n\
- The top workspace toolbar is grouped into Session, History, and Panels sections.\n\
- The left and right toolbars include inline dock controls, so you can move them without opening menus.\n\
- Reset Layout restores the default docked workspace if the panes get rearranged.\n\
- The Tools toolbar changes the active drawing mode.\n\
- The Inspector pane updates with the options for the selected tool.\n\
- The Objects toolbar handles selection and cleanup actions.\n\
- The Objects pane lists the current scene contents and selection state.\n\
- Sessions let multiple windows or agents attach to the same board scene.";

const HELP_SHORTCUTS_TEXT: &str = "Shortcuts\n\n\
- Delete / Backspace: delete selected objects\n\
- Ctrl+Z: undo\n\
- Ctrl+Shift+Z or Ctrl+Y: redo\n\
- Mouse wheel: zoom around the cursor\n\
- Middle drag: pan the board\n\
- Ctrl+0: reset the camera\n\
- Ctrl+1: fit the full board content\n\
- Ctrl+2: fit the current selection\n\
- Enter: finish polygon or 3-point triangle draft\n\
- Escape: cancel the active draft\n\
- Right click: finish polygon or 3-point triangle draft";

#[derive(Clone, Copy)]
struct ToolButtonControl {
    tool: Tool,
    button: BitmapButton,
}

#[derive(Clone, Copy)]
struct InspectorControls {
    panel: Panel,
    title_label: StaticText,
    hint_label: StaticText,
    empty_label: StaticText,
    color_label: StaticText,
    color_picker: ColourPickerCtrl,
    fill_toggle: CheckBox,
    triangle_label: StaticText,
    triangle_choice: Choice,
    selection_label: StaticText,
    state_label: StaticText,
}

#[derive(Clone)]
struct HistoryControls {
    summary_label: StaticText,
    list: ListBox,
    suppress_selection_events: Rc<Cell<bool>>,
}

#[derive(Clone, Copy)]
struct ViewControls {
    pin_toolbars_item: MenuItem,
    show_workspace_toolbar_item: MenuItem,
    show_tools_toolbar_item: MenuItem,
    show_objects_toolbar_item: MenuItem,
    show_session_pane_item: MenuItem,
    show_objects_pane_item: MenuItem,
    show_inspector_pane_item: MenuItem,
    show_history_pane_item: MenuItem,
    pin_toolbar_button: BitmapToggleButton,
    session_panel_button: BitmapButton,
    objects_panel_button: BitmapButton,
    inspector_panel_button: BitmapButton,
    history_panel_button: BitmapButton,
    reset_layout_button: BitmapButton,
    tools_dock_top_button: BitmapButton,
    tools_dock_left_button: BitmapButton,
    tools_dock_right_button: BitmapButton,
    tools_dock_bottom_button: BitmapButton,
    objects_pane_toggle_button: BitmapButton,
    objects_dock_top_button: BitmapButton,
    objects_dock_left_button: BitmapButton,
    objects_dock_right_button: BitmapButton,
    objects_dock_bottom_button: BitmapButton,
    workspace_toolbar: Panel,
    tools_toolbar: Panel,
    objects_toolbar: Panel,
    session_panel: Panel,
    objects_panel: Panel,
    inspector_panel: Panel,
    history_panel: Panel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tool {
    Select,
    Pen,
    Line,
    Rectangle,
    Circle,
    Triangle,
    Polygon,
    Fill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolbarPane {
    Workspace,
    Tools,
    Objects,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TriangleMode {
    Quick,
    ThreePoint,
}

impl TriangleMode {
    fn from_label(label: &str) -> Self {
        match label {
            "3-Point" => Self::ThreePoint,
            _ => Self::Quick,
        }
    }

    fn from_cli_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "3-point" | "three-point" | "points" | "freeform" => Self::ThreePoint,
            _ => Self::Quick,
        }
    }
}

impl Tool {
    fn from_cli_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "select" => Self::Select,
            "line" => Self::Line,
            "rectangle" | "rect" => Self::Rectangle,
            "circle" => Self::Circle,
            "triangle" => Self::Triangle,
            "polygon" | "poly" => Self::Polygon,
            "fill" => Self::Fill,
            _ => Self::Pen,
        }
    }
}

#[derive(Debug, Clone)]
enum DraftShape {
    SelectionBox {
        start: CanvasPoint,
        current: CanvasPoint,
        additive: bool,
    },
    Line {
        start: CanvasPoint,
        current: CanvasPoint,
    },
    Rectangle {
        start: CanvasPoint,
        current: CanvasPoint,
    },
    Circle {
        start: CanvasPoint,
        current: CanvasPoint,
    },
    Triangle {
        start: CanvasPoint,
        current: CanvasPoint,
    },
    TrianglePoints {
        points: Vec<CanvasPoint>,
        current: CanvasPoint,
    },
    Polygon {
        points: Vec<CanvasPoint>,
        current: CanvasPoint,
    },
    Stroke {
        points: Vec<CanvasPoint>,
    },
}

#[derive(Debug, Clone)]
struct BoardmeshState {
    session_root: PathBuf,
    scene_path: PathBuf,
    session_id: Option<String>,
    session_manifest_path: Option<PathBuf>,
    window_id: Option<String>,
    scene: SceneDocument,
    active_tool: Tool,
    triangle_mode: TriangleMode,
    selected_color: SceneColor,
    fill_shapes: bool,
    viewport: ViewportState,
    draft: Option<DraftShape>,
    pan_drag: Option<PanDragState>,
    last_seen: Option<SystemTime>,
    history: SceneHistory,
    last_window_state: Option<WindowState>,
}

#[derive(Debug, Clone, Copy)]
struct PanDragState {
    start_screen: CanvasPoint,
    start_viewport: ViewportState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ServerStatus {
    None,
    Connected {
        app_version: String,
        runtime_api_version: u32,
    },
    Incompatible {
        app_version: String,
        runtime_api_version: u32,
    },
    Stale {
        app_version: String,
        runtime_api_version: u32,
    },
    Error(String),
}

#[derive(Debug, Clone)]
struct AppOptions {
    session_root: PathBuf,
    scene_path: PathBuf,
    session_id: Option<String>,
    runtime_url: Option<String>,
    explicit_scene_mode: bool,
    initial_tool: Tool,
    initial_triangle_mode: TriangleMode,
    initial_fill_shapes: bool,
}

#[derive(Debug)]
enum RuntimeEvent {
    Attached(RuntimeSceneSnapshot),
    SceneUpdated(RuntimeSceneSnapshot),
    Error(String),
}

static GUI_RUNTIME_URL: OnceLock<Mutex<Option<String>>> = OnceLock::new();

impl BoardmeshState {
    fn new(
        session_root: PathBuf,
        scene_path: PathBuf,
        session_id: Option<String>,
        scene: SceneDocument,
        last_seen: Option<SystemTime>,
    ) -> Self {
        Self {
            session_root,
            scene_path,
            session_id,
            session_manifest_path: None,
            window_id: None,
            scene,
            active_tool: Tool::Pen,
            triangle_mode: TriangleMode::Quick,
            selected_color: SceneColor::rgb(15, 23, 42),
            fill_shapes: false,
            viewport: ViewportState::default(),
            draft: None,
            pan_drag: None,
            last_seen,
            history: SceneHistory::new(UNDO_HISTORY_LIMIT),
            last_window_state: None,
        }
    }

    fn stroke(&self) -> SceneStroke {
        SceneStroke {
            color: self.selected_color,
            width: 2,
        }
    }

    fn push_undo_snapshot(&mut self) {
        self.push_undo_snapshot_with_label("Edit");
    }

    fn push_undo_snapshot_with_label(&mut self, label: impl Into<String>) {
        self.history.push(label, self.scene.clone());
    }

    fn undo(&mut self) -> bool {
        let Some(previous_scene) = self.history.undo(self.scene.clone()) else {
            return false;
        };
        self.scene = previous_scene;
        true
    }

    fn redo(&mut self) -> bool {
        let Some(next_scene) = self.history.redo(self.scene.clone()) else {
            return false;
        };
        self.scene = next_scene;
        true
    }

    fn clear_history(&mut self) {
        self.history.clear();
    }

    fn local_workspace(&self) -> BoardWorkspace {
        BoardWorkspace::new(
            self.session_root.clone(),
            self.session_id.clone(),
            self.scene_path.clone(),
            self.scene.clone(),
        )
    }

    fn sync_from_workspace(&mut self, workspace: BoardWorkspace) {
        self.scene = workspace.scene_clone();
        self.scene_path = workspace.scene_path().to_path_buf();
        self.session_id = workspace.active_session_id_owned();
        self.session_manifest_path = self
            .session_id
            .as_deref()
            .map(|session_id| session_manifest_path(&self.session_root, session_id));
        self.last_seen = last_modified(&self.scene_path).ok();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeSceneUpdateKind {
    NoOp,
    ExternalRefresh,
}

fn log_gui(message: &str) {
    eprintln!("[boardmesh-gui] {message}");
}

fn gui_runtime_url_slot() -> &'static Mutex<Option<String>> {
    GUI_RUNTIME_URL.get_or_init(|| Mutex::new(None))
}

fn set_gui_runtime_url(runtime_url: Option<String>) {
    if let Ok(mut slot) = gui_runtime_url_slot().lock() {
        *slot = runtime_url;
    }
}

fn current_gui_runtime_url() -> Option<String> {
    gui_runtime_url_slot()
        .lock()
        .ok()
        .and_then(|runtime_url| runtime_url.clone())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_options()?;
    set_gui_runtime_url(options.runtime_url.clone());
    log_gui(&format!(
        "parsed options session={:?} scene={} sessions_dir={} runtime_url={:?} explicit_scene_mode={}",
        options.session_id,
        options.scene_path.display(),
        options.session_root.display(),
        options.runtime_url,
        options.explicit_scene_mode
    ));
    let initial_tool = options.initial_tool;
    let initial_triangle_mode = options.initial_triangle_mode;
    let initial_fill_shapes = options.initial_fill_shapes;
    let session_root = options.session_root.clone();

    wxdragon::main(move |app| {
        app.set_app_name("boardmesh");
        app.set_app_display_name("Boardmesh");

        let frame = Frame::builder()
            .with_title("Boardmesh")
            .with_size(Size::new(FRAME_WIDTH, FRAME_HEIGHT))
            .build();
        app.set_top_window(&frame);
        frame.centre();
        let status_bar = StatusBar::builder(&frame)
            .with_fields_count(3)
            .with_status_widths(vec![-3, -2, -2])
            .build();

        let startup_target = match resolve_startup_target(&frame, &options) {
            Ok(Some(target)) => target,
            Ok(None) => {
                frame.destroy();
                return;
            }
            Err(error) => {
                show_error_dialog(&frame, "Unable to open session", &error.to_string());
                frame.destroy();
                return;
            }
        };
        log_gui(&format!(
            "startup target session={:?} scene={}",
            startup_target.session_id,
            startup_target.scene_path.display()
        ));
        let scene = ensure_scene_file(&startup_target.scene_path)
            .expect("failed to initialize board scene");
        let runtime_snapshot = options.runtime_url.as_ref().and_then(|runtime_url| {
            runtime_request::<SessionAttachParams, RuntimeSceneSnapshot>(
                runtime_url,
                METHOD_SESSION_ATTACH,
                SessionAttachParams {
                    session_id: startup_target.session_id.clone(),
                    create_if_missing: startup_target.session_id.is_some(),
                },
            )
            .map_err(|error| {
                log_gui(&format!(
                    "runtime attach failed; falling back to local scene: {error:#}"
                ));
                error
            })
            .ok()
        });
        if options.runtime_url.is_some() && runtime_snapshot.is_none() {
            set_gui_runtime_url(None);
        }
        let scene = runtime_snapshot
            .as_ref()
            .map(|snapshot| snapshot.scene.clone())
            .unwrap_or(scene);
        let modified = runtime_snapshot
            .as_ref()
            .and_then(|snapshot| last_modified(&PathBuf::from(&snapshot.scene_path)).ok())
            .or_else(|| last_modified(&startup_target.scene_path).ok());
        let scene_path = runtime_snapshot
            .as_ref()
            .map(|snapshot| PathBuf::from(&snapshot.scene_path))
            .unwrap_or_else(|| startup_target.scene_path.clone());
        let session_id = runtime_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.session_id.clone())
            .or_else(|| startup_target.session_id.clone());
        let session_manifest = if let Some(snapshot) = runtime_snapshot.as_ref() {
            snapshot
                .manifest_path
                .as_ref()
                .and_then(|manifest_path| load_session_manifest(&PathBuf::from(manifest_path)).ok())
        } else {
            startup_target
                .session_id
                .as_deref()
                .map(|session_id| {
                    ensure_gui_session_manifest(&session_root, session_id, &scene_path)
                })
                .transpose()
                .expect("failed to initialize session manifest")
        };
        log_gui(&format!(
            "loaded scene path={} object_count={}",
            scene_path.display(),
            scene.objects.len()
        ));

        let manager = AuiManager::builder(&frame).build();
        let canvas = Panel::builder(&frame).build();
        let session_panel = Panel::builder(&frame)
            .with_size(Size::new(SESSION_PANEL_WIDTH, 360))
            .build();
        let inspector_panel = Panel::builder(&frame)
            .with_size(Size::new(INSPECTOR_PANEL_WIDTH, 420))
            .build();
        let objects_panel = Panel::builder(&frame)
            .with_size(Size::new(OBJECTS_PANEL_WIDTH, 420))
            .build();
        let history_panel = Panel::builder(&frame)
            .with_size(Size::new(720, HISTORY_PANEL_HEIGHT))
            .build();
        let workspace_toolbar = Panel::builder(&frame)
            .with_size(Size::new(WORKSPACE_TOOLBAR_WIDTH, WORKSPACE_TOOLBAR_HEIGHT))
            .build();
        let tools_toolbar = Panel::builder(&frame)
            .with_size(Size::new(SIDE_TOOLBAR_WIDTH, TOOLS_TOOLBAR_HEIGHT))
            .build();
        let objects_toolbar = Panel::builder(&frame)
            .with_size(Size::new(SIDE_TOOLBAR_WIDTH, OBJECTS_TOOLBAR_HEIGHT))
            .build();
        canvas.set_background_style(BackgroundStyle::Paint);
        canvas.set_min_size(Size::new(CANVAS_MIN_WIDTH, CANVAS_MIN_HEIGHT));
        canvas.set_background_color(Colour::new(248, 250, 252, 255));
        session_panel.set_background_color(Colour::new(248, 250, 252, 255));
        inspector_panel.set_background_color(Colour::new(248, 250, 252, 255));
        objects_panel.set_background_color(Colour::new(248, 250, 252, 255));
        history_panel.set_background_color(Colour::new(248, 250, 252, 255));
        workspace_toolbar.set_background_color(Colour::new(255, 255, 255, 255));
        tools_toolbar.set_background_color(Colour::new(255, 255, 255, 255));
        objects_toolbar.set_background_color(Colour::new(255, 255, 255, 255));

        let state = Rc::new(RefCell::new(BoardmeshState::new(
            session_root.clone(),
            scene_path.clone(),
            session_id.clone(),
            scene,
            modified,
        )));
        {
            let mut state = state.borrow_mut();
            state.active_tool = initial_tool;
            state.triangle_mode = initial_triangle_mode;
            state.fill_shapes = initial_fill_shapes;
            if let Some(manifest) = session_manifest.as_ref() {
                state.session_manifest_path =
                    Some(session_manifest_path(&session_root, &manifest.session_id));
                state.window_id = Some(new_window_id());
            }
        }

        if let Some(window_state) = session_manifest.as_ref().and_then(last_window_state) {
            frame.set_size_with_pos(
                window_state.x,
                window_state.y,
                window_state.width,
                window_state.height,
            );
            if window_state.maximized {
                frame.maximize(true);
            }
            if window_state.iconized {
                frame.iconize(true);
            }
        }

        let file_menu = Menu::builder().build();
        let open_session_item = file_menu
            .append(
                MENU_ID_OPEN_SESSION,
                "&Open or Create Session...\tCtrl+O",
                "Open a board session",
                MenuItemKind::Normal,
            )
            .expect("failed to create open session menu item");
        file_menu.append_separator();
        let exit_item = file_menu
            .append(
                ID_EXIT,
                "E&xit\tCtrl+Q",
                "Close Boardmesh",
                MenuItemKind::Normal,
            )
            .expect("failed to create exit menu item");

        let view_menu = Menu::builder().build();
        let pin_toolbars_item = view_menu
            .append(
                MENU_ID_PIN_TOOLBARS,
                "&Show Toolbars\tCtrl+P",
                "Show or hide the toolbar set",
                MenuItemKind::Check,
            )
            .expect("failed to create pin toolbars menu item");
        view_menu.append_separator();
        let show_workspace_toolbar_item = view_menu
            .append(
                MENU_ID_SHOW_WORKSPACE_TOOLBAR,
                "Show &Workspace Toolbar",
                "Show or hide the workspace toolbar",
                MenuItemKind::Check,
            )
            .expect("failed to create workspace toolbar menu item");
        let show_tools_toolbar_item = view_menu
            .append(
                MENU_ID_SHOW_TOOLS_TOOLBAR,
                "Show &Tools Toolbar",
                "Show or hide the drawing tools toolbar",
                MenuItemKind::Check,
            )
            .expect("failed to create tools toolbar menu item");
        let show_objects_toolbar_item = view_menu
            .append(
                MENU_ID_SHOW_OBJECTS_TOOLBAR,
                "Show Ob&jects Toolbar",
                "Show or hide the objects toolbar",
                MenuItemKind::Check,
            )
            .expect("failed to create objects toolbar menu item");
        let workspace_toolbar_menu = Menu::builder().build();
        let _workspace_toolbar_top_item = workspace_toolbar_menu
            .append(
                MENU_ID_WORKSPACE_TOOLBAR_TOP,
                "Dock &Top",
                "Dock the workspace toolbar at the top edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create workspace toolbar top menu item");
        let _workspace_toolbar_left_item = workspace_toolbar_menu
            .append(
                MENU_ID_WORKSPACE_TOOLBAR_LEFT,
                "Dock &Left",
                "Dock the workspace toolbar at the left edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create workspace toolbar left menu item");
        let _workspace_toolbar_right_item = workspace_toolbar_menu
            .append(
                MENU_ID_WORKSPACE_TOOLBAR_RIGHT,
                "Dock &Right",
                "Dock the workspace toolbar at the right edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create workspace toolbar right menu item");
        let _workspace_toolbar_bottom_item = workspace_toolbar_menu
            .append(
                MENU_ID_WORKSPACE_TOOLBAR_BOTTOM,
                "Dock &Bottom",
                "Dock the workspace toolbar at the bottom edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create workspace toolbar bottom menu item");
        view_menu
            .append_submenu(
                workspace_toolbar_menu,
                "Workspace Toolbar &Position",
                "Move the workspace toolbar to another edge",
            )
            .expect("failed to create workspace toolbar submenu");
        let tools_toolbar_menu = Menu::builder().build();
        let _tools_toolbar_top_item = tools_toolbar_menu
            .append(
                MENU_ID_TOOLS_TOOLBAR_TOP,
                "Dock Top",
                "Dock the tools toolbar at the top edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create tools toolbar top menu item");
        let _tools_toolbar_left_item = tools_toolbar_menu
            .append(
                MENU_ID_TOOLS_TOOLBAR_LEFT,
                "Dock Left",
                "Dock the tools toolbar at the left edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create tools toolbar left menu item");
        let _tools_toolbar_right_item = tools_toolbar_menu
            .append(
                MENU_ID_TOOLS_TOOLBAR_RIGHT,
                "Dock Right",
                "Dock the tools toolbar at the right edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create tools toolbar right menu item");
        let _tools_toolbar_bottom_item = tools_toolbar_menu
            .append(
                MENU_ID_TOOLS_TOOLBAR_BOTTOM,
                "Dock Bottom",
                "Dock the tools toolbar at the bottom edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create tools toolbar bottom menu item");
        view_menu
            .append_submenu(
                tools_toolbar_menu,
                "Tools Toolbar P&osition",
                "Move the tools toolbar to another edge",
            )
            .expect("failed to create tools toolbar submenu");
        let objects_toolbar_menu = Menu::builder().build();
        let _objects_toolbar_top_item = objects_toolbar_menu
            .append(
                MENU_ID_OBJECTS_TOOLBAR_TOP,
                "Dock Top",
                "Dock the objects toolbar at the top edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create objects toolbar top menu item");
        let _objects_toolbar_left_item = objects_toolbar_menu
            .append(
                MENU_ID_OBJECTS_TOOLBAR_LEFT,
                "Dock Left",
                "Dock the objects toolbar at the left edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create objects toolbar left menu item");
        let _objects_toolbar_right_item = objects_toolbar_menu
            .append(
                MENU_ID_OBJECTS_TOOLBAR_RIGHT,
                "Dock Right",
                "Dock the objects toolbar at the right edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create objects toolbar right menu item");
        let _objects_toolbar_bottom_item = objects_toolbar_menu
            .append(
                MENU_ID_OBJECTS_TOOLBAR_BOTTOM,
                "Dock Bottom",
                "Dock the objects toolbar at the bottom edge",
                MenuItemKind::Normal,
            )
            .expect("failed to create objects toolbar bottom menu item");
        view_menu
            .append_submenu(
                objects_toolbar_menu,
                "Objects Toolbar Po&sition",
                "Move the objects toolbar to another edge",
            )
            .expect("failed to create objects toolbar submenu");
        view_menu.append_separator();
        let show_session_pane_item = view_menu
            .append(
                MENU_ID_SHOW_SESSION_PANE,
                "Show Session &Panel",
                "Show or hide the session panel",
                MenuItemKind::Check,
            )
            .expect("failed to create session pane menu item");
        let show_objects_pane_item = view_menu
            .append(
                MENU_ID_SHOW_OBJECTS_PANE,
                "Show Objects Pane",
                "Show or hide the objects pane",
                MenuItemKind::Check,
            )
            .expect("failed to create objects pane menu item");
        let show_inspector_pane_item = view_menu
            .append(
                MENU_ID_SHOW_INSPECTOR_PANE,
                "Show &Inspector",
                "Show or hide the inspector pane",
                MenuItemKind::Check,
            )
            .expect("failed to create inspector pane menu item");
        let show_history_pane_item = view_menu
            .append(
                MENU_ID_SHOW_HISTORY_PANE,
                "Show &History",
                "Show or hide the history pane",
                MenuItemKind::Check,
            )
            .expect("failed to create history pane menu item");
        view_menu.append_separator();
        let _reset_layout_item = view_menu
            .append(
                MENU_ID_RESET_LAYOUT,
                "&Reset Layout",
                "Restore the default docked layout",
                MenuItemKind::Normal,
            )
            .expect("failed to create reset layout menu item");

        let help_menu = Menu::builder().build();
        let help_reference_item = help_menu
            .append(
                MENU_ID_HELP_REFERENCE,
                "&Usage Reference",
                "Show the board usage reference",
                MenuItemKind::Normal,
            )
            .expect("failed to create help reference menu item");
        let help_shortcuts_item = help_menu
            .append(
                MENU_ID_HELP_SHORTCUTS,
                "&Keyboard Shortcuts",
                "Show keyboard shortcuts",
                MenuItemKind::Normal,
            )
            .expect("failed to create help shortcuts menu item");
        help_menu.append_separator();
        let about_item = help_menu
            .append(
                ID_ABOUT,
                "&About Boardmesh",
                "Show the about dialog",
                MenuItemKind::Normal,
            )
            .expect("failed to create about menu item");

        let menu_bar = MenuBar::builder()
            .append(file_menu, "&File")
            .append(view_menu, "&View")
            .append(help_menu, "&Help")
            .build();
        frame.set_menu_bar(menu_bar);

        if let Some(bitmap) = load_fontawesome_icon("folder-open", MENU_ICON_SIZE)
            .or_else(|| ArtProvider::get_bitmap(ArtId::FileOpen, ArtClient::Menu, None))
        {
            open_session_item.set_bitmap(&bitmap);
        }
        if let Some(bitmap) = ArtProvider::get_bitmap(ArtId::Quit, ArtClient::Menu, None) {
            exit_item.set_bitmap(&bitmap);
        }
        if let Some(bitmap) = load_fontawesome_icon("question-circle", MENU_ICON_SIZE)
            .or_else(|| ArtProvider::get_bitmap(ArtId::Help, ArtClient::Menu, None))
        {
            help_reference_item.set_bitmap(&bitmap);
            help_shortcuts_item.set_bitmap(&bitmap);
        }
        if let Some(bitmap) = ArtProvider::get_bitmap(ArtId::Information, ArtClient::Menu, None) {
            about_item.set_bitmap(&bitmap);
        }
        pin_toolbars_item.set_bitmap(&make_pin_bitmap(true));

        let session_heading = StaticText::builder(&session_panel)
            .with_label("Workspace")
            .build();
        let scene_label = StaticText::builder(&session_panel)
            .with_label(&scene_summary_label(
                session_id.as_deref(),
                &scene_path,
                &ServerStatus::None,
            ))
            .with_size(Size::new(SECTION_CONTENT_WIDTH, 96))
            .build();
        scene_label.wrap(SECTION_CONTENT_WIDTH);
        let switch_session_button = Button::builder(&session_panel)
            .with_label("Open / Create Session")
            .build();

        let inspector_title_label = StaticText::builder(&inspector_panel)
            .with_label(&format!("{} Tool", tool_name(initial_tool)))
            .build();
        let tool_hint_label = StaticText::builder(&inspector_panel)
            .with_label(tool_hint(initial_tool))
            .with_size(Size::new(SECTION_CONTENT_WIDTH, 56))
            .build();
        tool_hint_label.wrap(SECTION_CONTENT_WIDTH);
        let inspector_empty_label = StaticText::builder(&inspector_panel)
            .with_label("No extra options for the selected tool.")
            .with_size(Size::new(SECTION_CONTENT_WIDTH, 36))
            .build();
        let color_label = StaticText::builder(&inspector_panel)
            .with_label("Stroke / fill color")
            .build();
        let color_picker = ColourPickerCtrl::builder(&inspector_panel)
            .with_initial_colour(as_wx_color(SceneColor::rgb(15, 23, 42)))
            .build();
        let fill_toggle = CheckBox::builder(&inspector_panel)
            .with_label("Filled Shapes")
            .with_value(initial_fill_shapes)
            .build();
        let triangle_mode_label = StaticText::builder(&inspector_panel)
            .with_label("Triangle creation mode")
            .build();
        let triangle_mode_choice = Choice::builder(&inspector_panel)
            .with_choices(vec!["Quick".to_string(), "3-Point".to_string()])
            .with_selection(Some(triangle_mode_index(initial_triangle_mode)))
            .build();
        let inspector_selection_label = StaticText::builder(&inspector_panel)
            .with_label("Selection: nothing selected")
            .with_size(Size::new(SECTION_CONTENT_WIDTH, 40))
            .build();
        inspector_selection_label.wrap(SECTION_CONTENT_WIDTH);
        let inspector_state_label = StaticText::builder(&inspector_panel)
            .with_label("State: ready")
            .with_size(Size::new(SECTION_CONTENT_WIDTH, 56))
            .build();
        inspector_state_label.wrap(SECTION_CONTENT_WIDTH);
        let history_heading = StaticText::builder(&history_panel)
            .with_label("History")
            .build();
        let history_summary_label = StaticText::builder(&history_panel)
            .with_label("Undo: 0 available  |  Redo: 0 available")
            .with_size(Size::new(SECTION_CONTENT_WIDTH * 2, 32))
            .build();
        history_summary_label.wrap(SECTION_CONTENT_WIDTH * 2);
        let history_list = ListBox::builder(&history_panel)
            .with_size(Size::new(SECTION_CONTENT_WIDTH * 2, 180))
            .build();

        let object_list_heading = StaticText::builder(&objects_panel)
            .with_label("Objects")
            .build();
        let object_list = CheckListBox::builder(&objects_panel)
            .with_size(Size::new(SECTION_CONTENT_WIDTH, 280))
            .build();
        let session_sizer = BoxSizer::builder(Orientation::Vertical).build();
        session_sizer.add(&session_heading, 0, SizerFlag::All, 12);
        session_sizer.add(
            &scene_label,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        session_sizer.add(
            &switch_session_button,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        session_sizer.add_stretch_spacer(1);
        session_panel.set_sizer(session_sizer, false);

        let inspector_sizer = BoxSizer::builder(Orientation::Vertical).build();
        inspector_sizer.add(&inspector_title_label, 0, SizerFlag::All, 12);
        inspector_sizer.add(
            &tool_hint_label,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &inspector_empty_label,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &color_label,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &color_picker,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &fill_toggle,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &triangle_mode_label,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &triangle_mode_choice,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &inspector_selection_label,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add(
            &inspector_state_label,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        inspector_sizer.add_stretch_spacer(1);
        inspector_panel.set_sizer(inspector_sizer, false);

        let objects_sizer = BoxSizer::builder(Orientation::Vertical).build();
        objects_sizer.add(&object_list_heading, 0, SizerFlag::All, 12);
        objects_sizer.add(
            &object_list,
            1,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        objects_panel.set_sizer(objects_sizer, false);
        let history_sizer = BoxSizer::builder(Orientation::Vertical).build();
        history_sizer.add(&history_heading, 0, SizerFlag::All, 12);
        history_sizer.add(
            &history_summary_label,
            0,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        history_sizer.add(
            &history_list,
            1,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            12,
        );
        history_panel.set_sizer(history_sizer, false);

        let pin_toolbar_button = BitmapToggleButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_pin_bitmap(true)))
            .with_style(BitmapToggleButtonStyle::ExactFit | BitmapToggleButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        pin_toolbar_button.set_value(true);
        pin_toolbar_button.set_tooltip("Show or hide all toolbars");
        let workspace_session_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("folder-open", 'S')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        workspace_session_button.set_tooltip("Open or create a board session");
        let undo_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("undo-alt", 'U')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        undo_button.set_tooltip("Undo the last change");
        let redo_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("redo-alt", 'R')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        redo_button.set_tooltip("Redo the last undone change");
        let clear_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("trash-alt", 'C')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        clear_button.set_tooltip("Clear the current scene");
        let workspace_help_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("question-circle", '?')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        workspace_help_button.set_tooltip("Open the help reference");
        let session_panel_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_layout_panel_bitmap('S')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        session_panel_button.set_tooltip("Show or hide the session panel");
        let objects_panel_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_layout_panel_bitmap('O')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        objects_panel_button.set_tooltip("Show or hide the objects pane");
        let inspector_panel_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_layout_panel_bitmap('I')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        inspector_panel_button.set_tooltip("Show or hide the inspector pane");
        let history_panel_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("history", 'H')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        history_panel_button.set_tooltip("Show or hide the history pane");
        let reset_layout_button = BitmapButton::builder(&workspace_toolbar)
            .with_bitmap(Some(make_layout_panel_bitmap('L')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        reset_layout_button.set_tooltip("Reset the workspace layout");
        let session_group_label = make_toolbar_section_label(&workspace_toolbar, "Session", 56);
        let history_group_label = make_toolbar_section_label(&workspace_toolbar, "History", 56);
        let panels_group_label = make_toolbar_section_label(&workspace_toolbar, "Panels", 52);
        let workspace_divider_one =
            make_toolbar_divider(&workspace_toolbar, Orientation::Vertical, 28);
        let workspace_divider_two =
            make_toolbar_divider(&workspace_toolbar, Orientation::Vertical, 28);

        let workspace_toolbar_sizer = BoxSizer::builder(Orientation::Horizontal).build();
        workspace_toolbar_sizer.add(
            &session_group_label,
            0,
            SizerFlag::All | SizerFlag::AlignCenterVertical,
            6,
        );
        workspace_toolbar_sizer.add(&workspace_session_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(
            &workspace_divider_one,
            0,
            SizerFlag::All | SizerFlag::AlignCenterVertical,
            8,
        );
        workspace_toolbar_sizer.add(
            &history_group_label,
            0,
            SizerFlag::All | SizerFlag::AlignCenterVertical,
            6,
        );
        workspace_toolbar_sizer.add(&undo_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&redo_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&clear_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(
            &workspace_divider_two,
            0,
            SizerFlag::All | SizerFlag::AlignCenterVertical,
            8,
        );
        workspace_toolbar_sizer.add(
            &panels_group_label,
            0,
            SizerFlag::All | SizerFlag::AlignCenterVertical,
            6,
        );
        workspace_toolbar_sizer.add(&pin_toolbar_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&session_panel_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&objects_panel_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&inspector_panel_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&history_panel_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&reset_layout_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add_stretch_spacer(1);
        workspace_toolbar_sizer.add(&workspace_help_button, 0, SizerFlag::All, 6);
        workspace_toolbar.set_sizer(workspace_toolbar_sizer, false);

        let tool_buttons = TOOL_ORDER
            .iter()
            .copied()
            .map(|tool| {
                let button = BitmapButton::builder(&tools_toolbar)
                    .with_bitmap(Some(make_tool_bitmap(tool)))
                    .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
                    .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
                    .build();
                button.set_background_color(inactive_tool_button_color());
                button.set_tooltip(tool_name(tool));
                ToolButtonControl { tool, button }
            })
            .collect::<Vec<_>>();
        let tools_dock_top_button = BitmapButton::builder(&tools_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Top)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        tools_dock_top_button.set_tooltip("Dock the tools toolbar at the top");
        let tools_dock_left_button = BitmapButton::builder(&tools_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Left)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        tools_dock_left_button.set_tooltip("Dock the tools toolbar at the left");
        let tools_dock_right_button = BitmapButton::builder(&tools_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Right)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        tools_dock_right_button.set_tooltip("Dock the tools toolbar at the right");
        let tools_dock_bottom_button = BitmapButton::builder(&tools_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Bottom)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        tools_dock_bottom_button.set_tooltip("Dock the tools toolbar at the bottom");
        let draw_group_label =
            make_toolbar_section_label(&tools_toolbar, "Draw", SIDE_TOOLBAR_WIDTH - 12);
        let dock_group_label =
            make_toolbar_section_label(&tools_toolbar, "Dock", SIDE_TOOLBAR_WIDTH - 12);
        let tools_divider = make_toolbar_divider(
            &tools_toolbar,
            Orientation::Horizontal,
            SIDE_TOOLBAR_WIDTH - 20,
        );
        let tools_toolbar_sizer = BoxSizer::builder(Orientation::Vertical).build();
        tools_toolbar_sizer.add(
            &draw_group_label,
            0,
            SizerFlag::Top | SizerFlag::Left | SizerFlag::Right | SizerFlag::AlignCenterHorizontal,
            8,
        );
        for tool_button in &tool_buttons {
            tools_toolbar_sizer.add(&tool_button.button, 0, SizerFlag::All, 6);
        }
        tools_toolbar_sizer.add_stretch_spacer(1);
        tools_toolbar_sizer.add(
            &tools_divider,
            0,
            SizerFlag::Left
                | SizerFlag::Right
                | SizerFlag::Bottom
                | SizerFlag::AlignCenterHorizontal,
            8,
        );
        tools_toolbar_sizer.add(
            &dock_group_label,
            0,
            SizerFlag::Left
                | SizerFlag::Right
                | SizerFlag::Bottom
                | SizerFlag::AlignCenterHorizontal,
            6,
        );
        tools_toolbar_sizer.add(&tools_dock_top_button, 0, SizerFlag::All, 6);
        tools_toolbar_sizer.add(&tools_dock_left_button, 0, SizerFlag::All, 6);
        tools_toolbar_sizer.add(&tools_dock_right_button, 0, SizerFlag::All, 6);
        tools_toolbar_sizer.add(&tools_dock_bottom_button, 0, SizerFlag::All, 6);
        tools_toolbar.set_sizer(tools_toolbar_sizer, false);

        let select_all_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_select_all_bitmap()))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        select_all_button.set_tooltip("Select every object in the scene");
        let clear_selection_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_clear_selection_bitmap()))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        clear_selection_button.set_tooltip("Clear the current selection");
        let delete_selected_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_toolbar_action_bitmap("trash-alt", 'D')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        delete_selected_button.set_tooltip("Delete the selected objects");
        let objects_pane_toggle_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_layout_panel_bitmap('O')))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        objects_pane_toggle_button.set_tooltip("Show or hide the objects pane");
        let objects_dock_top_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Top)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        objects_dock_top_button.set_tooltip("Dock the objects toolbar at the top");
        let objects_dock_left_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Left)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        objects_dock_left_button.set_tooltip("Dock the objects toolbar at the left");
        let objects_dock_right_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Right)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        objects_dock_right_button.set_tooltip("Dock the objects toolbar at the right");
        let objects_dock_bottom_button = BitmapButton::builder(&objects_toolbar)
            .with_bitmap(Some(make_direction_bitmap(ToolbarDock::Bottom)))
            .with_style(BitmapButtonStyle::ExactFit | BitmapButtonStyle::NoText)
            .with_size(Size::new(TOOLBAR_BUTTON_SIZE, TOOLBAR_BUTTON_SIZE))
            .build();
        objects_dock_bottom_button.set_tooltip("Dock the objects toolbar at the bottom");
        let selection_group_label =
            make_toolbar_section_label(&objects_toolbar, "Select", SIDE_TOOLBAR_WIDTH - 12);
        let layout_group_label =
            make_toolbar_section_label(&objects_toolbar, "Layout", SIDE_TOOLBAR_WIDTH - 12);
        let objects_divider = make_toolbar_divider(
            &objects_toolbar,
            Orientation::Horizontal,
            SIDE_TOOLBAR_WIDTH - 20,
        );
        let objects_toolbar_sizer = BoxSizer::builder(Orientation::Vertical).build();
        objects_toolbar_sizer.add(
            &selection_group_label,
            0,
            SizerFlag::Top | SizerFlag::Left | SizerFlag::Right | SizerFlag::AlignCenterHorizontal,
            8,
        );
        objects_toolbar_sizer.add(&select_all_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&clear_selection_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&delete_selected_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add_stretch_spacer(1);
        objects_toolbar_sizer.add(
            &objects_divider,
            0,
            SizerFlag::Left
                | SizerFlag::Right
                | SizerFlag::Bottom
                | SizerFlag::AlignCenterHorizontal,
            8,
        );
        objects_toolbar_sizer.add(
            &layout_group_label,
            0,
            SizerFlag::Left
                | SizerFlag::Right
                | SizerFlag::Bottom
                | SizerFlag::AlignCenterHorizontal,
            6,
        );
        objects_toolbar_sizer.add(&objects_pane_toggle_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_dock_top_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_dock_left_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_dock_right_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_dock_bottom_button, 0, SizerFlag::All, 6);
        objects_toolbar.set_sizer(objects_toolbar_sizer, false);

        let inspector = InspectorControls {
            panel: inspector_panel,
            title_label: inspector_title_label,
            hint_label: tool_hint_label,
            empty_label: inspector_empty_label,
            color_label,
            color_picker,
            fill_toggle,
            triangle_label: triangle_mode_label,
            triangle_choice: triangle_mode_choice,
            selection_label: inspector_selection_label,
            state_label: inspector_state_label,
        };
        let history = HistoryControls {
            summary_label: history_summary_label,
            list: history_list,
            suppress_selection_events: Rc::new(Cell::new(false)),
        };

        let layout_state = Rc::new(RefCell::new(LayoutState::default()));

        let view_controls = ViewControls {
            pin_toolbars_item,
            show_workspace_toolbar_item,
            show_tools_toolbar_item,
            show_objects_toolbar_item,
            show_session_pane_item,
            show_objects_pane_item,
            show_inspector_pane_item,
            show_history_pane_item,
            pin_toolbar_button,
            session_panel_button,
            objects_panel_button,
            inspector_panel_button,
            history_panel_button,
            reset_layout_button,
            tools_dock_top_button,
            tools_dock_left_button,
            tools_dock_right_button,
            tools_dock_bottom_button,
            objects_pane_toggle_button,
            objects_dock_top_button,
            objects_dock_left_button,
            objects_dock_right_button,
            objects_dock_bottom_button,
            workspace_toolbar,
            tools_toolbar,
            objects_toolbar,
            session_panel,
            objects_panel,
            inspector_panel,
            history_panel,
        };

        apply_layout(&manager, &canvas, &view_controls, &layout_state.borrow());
        sync_tool_palette(&tool_buttons, initial_tool);
        sync_window_ui_with_layout(
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &object_list,
            &layout_state,
            &manager,
            &canvas,
            &view_controls,
        );

        history.list.on_selection_changed({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                if history.suppress_selection_events.get() {
                    return;
                }

                let Some(selection) = history.list.get_selection() else {
                    return;
                };

                let action = {
                    let state = state.borrow();
                    history_action_for_selection(&state, selection)
                };

                let Some(action) = action else {
                    return;
                };

                let _ = apply_history_action(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                    action,
                );
            }
        });
        {
            let state = state.borrow();
            log_gui(&format!(
                "window initialized session={:?} scene={} object_count={}",
                state.session_id,
                state.scene_path.display(),
                state.scene.objects.len()
            ));
        }
        let runtime_events = options.runtime_url.as_ref().map(|runtime_url| {
            Rc::new(RefCell::new(spawn_runtime_subscription(
                runtime_url.clone(),
                session_id.clone(),
            )))
        });

        for tool_button in &tool_buttons {
            let state = Rc::clone(&state);
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let tool_buttons = tool_buttons.clone();
            let frame = frame;
            let inspector = inspector;
            let status_bar = status_bar;
            let history = history.clone();
            let selected_tool = tool_button.tool;
            let button = tool_button.button;
            button.on_click(move |_| {
                let mut should_release_mouse = false;
                let mut should_refresh = false;
                {
                    let mut state = state.borrow_mut();
                    state.active_tool = selected_tool;
                    sync_tool_palette(&tool_buttons, state.active_tool);
                    if state.draft.is_some() {
                        should_release_mouse = canvas.has_capture();
                        state.draft = None;
                        should_refresh = true;
                    }
                }
                sync_window_ui_with_layout(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &object_list,
                    &layout_state,
                    &manager,
                    &canvas,
                    &view_controls,
                );
                if should_release_mouse {
                    canvas.release_mouse();
                }
                if should_refresh {
                    canvas.refresh(false, None);
                }
            });
        }

        color_picker.on_colour_changed({
            let state = Rc::clone(&state);
            let status_bar = status_bar;
            let inspector = inspector;
            move |_| {
                {
                    let mut state = state.borrow_mut();
                    state.selected_color = from_wx_color(color_picker.get_colour());
                }
                let state = state.borrow();
                update_status_bar(
                    &status_bar,
                    &state,
                    &session_server_status(state.session_manifest_path.as_ref()),
                );
                update_inspector_panel(&inspector, &state);
            }
        });

        fill_toggle.on_toggled({
            let state = Rc::clone(&state);
            let status_bar = status_bar;
            let inspector = inspector;
            move |event| {
                {
                    let mut state = state.borrow_mut();
                    state.fill_shapes = event.is_checked();
                }
                let state = state.borrow();
                update_status_bar(
                    &status_bar,
                    &state,
                    &session_server_status(state.session_manifest_path.as_ref()),
                );
                update_inspector_panel(&inspector, &state);
            }
        });

        triangle_mode_choice.on_selection_changed({
            let state = Rc::clone(&state);
            let canvas = canvas;
            let status_bar = status_bar;
            let inspector = inspector;
            move |_| {
                if let Some(selection) = triangle_mode_choice.get_string_selection() {
                    let mut should_release_mouse = false;
                    let mut should_refresh = false;
                    {
                        let mut state = state.borrow_mut();
                        state.triangle_mode = TriangleMode::from_label(&selection);
                        if matches!(
                            state.draft,
                            Some(DraftShape::Triangle { .. } | DraftShape::TrianglePoints { .. })
                        ) {
                            should_release_mouse = canvas.has_capture();
                            state.draft = None;
                            should_refresh = true;
                        }
                    }
                    {
                        let state = state.borrow();
                        update_status_bar(
                            &status_bar,
                            &state,
                            &session_server_status(state.session_manifest_path.as_ref()),
                        );
                        update_inspector_panel(&inspector, &state);
                    }
                    if should_release_mouse {
                        canvas.release_mouse();
                    }
                    if should_refresh {
                        canvas.refresh(false, None);
                    }
                }
            }
        });

        object_list.on_toggled({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let history = history.clone();
            move |_| {
                {
                    let mut state = state.borrow_mut();
                    let selected_ids = checked_object_ids(&object_list, &state.scene);
                    if let Err(error) = commit_selection_change(&mut state, selected_ids) {
                        eprintln!("failed to save selection change: {error:#}");
                    }
                }
                sync_window_ui_with_layout(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &object_list,
                    &layout_state,
                    &manager,
                    &canvas,
                    &view_controls,
                );
                canvas.refresh(false, None);
            }
        });

        let open_session_action: Rc<dyn Fn()> = Rc::new({
            let state = Rc::clone(&state);
            let frame = frame;
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let object_list = object_list;
            let scene_label = scene_label;
            let status_bar = status_bar;
            let inspector = inspector;
            let history = history.clone();
            let session_root = session_root.clone();
            let runtime_events = runtime_events.clone();
            move || {
                let current_session_id = state.borrow().session_id.clone();
                let runtime_url = current_gui_runtime_url();
                let chosen_session_id = match choose_session_interactively(
                    &frame,
                    &session_root,
                    runtime_url.as_deref(),
                    current_session_id.as_deref(),
                    "Switch Session",
                    "Select a board session to attach this window to.",
                ) {
                    Ok(session_id) => session_id,
                    Err(error) => {
                        show_error_dialog(&frame, "Unable to switch sessions", &error.to_string());
                        return;
                    }
                };

                let Some(chosen_session_id) = chosen_session_id else {
                    return;
                };
                if let Err(error) = switch_window_to_session(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                    &scene_label,
                    &layout_state,
                    &manager,
                    view_controls,
                    &session_root,
                    &chosen_session_id,
                    runtime_events.as_ref(),
                ) {
                    show_error_dialog(&frame, "Unable to switch sessions", &error.to_string());
                }
            }
        });
        switch_session_button.on_click({
            let open_session_action = Rc::clone(&open_session_action);
            move |_| open_session_action()
        });
        workspace_session_button.on_click({
            let open_session_action = Rc::clone(&open_session_action);
            move |_| open_session_action()
        });

        clear_button.on_click({
            let state = Rc::clone(&state);
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let frame = frame;
            let status_bar = status_bar;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                {
                    let mut state = state.borrow_mut();
                    if !state.scene.objects.is_empty() {
                        state.push_undo_snapshot();
                    }
                    if let Err(error) = commit_clear_scene(&mut state) {
                        eprintln!("failed to save cleared scene: {error:#}");
                    }
                }
                sync_window_ui_with_layout(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &object_list,
                    &layout_state,
                    &manager,
                    &canvas,
                    &view_controls,
                );
                canvas.refresh(false, None);
            }
        });

        workspace_help_button.on_click({
            let frame = frame;
            move |_| show_info_dialog(&frame, "Usage Reference", HELP_REFERENCE_TEXT)
        });

        undo_button.on_click({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                let _ = undo_last_change(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                );
            }
        });
        redo_button.on_click({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                let _ = redo_last_change(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                );
            }
        });
        select_all_button.on_click({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                let _ = select_all_objects(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                );
            }
        });
        clear_selection_button.on_click({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                let _ = clear_selection(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                );
            }
        });
        delete_selected_button.on_click({
            let state = Rc::clone(&state);
            let frame = frame;
            let status_bar = status_bar;
            let canvas = canvas;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                let _ = delete_selected_objects(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                );
            }
        });

        bind_keyboard_shortcuts(
            &frame,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &canvas,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &session_panel,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &inspector_panel,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_panel,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &history_panel,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &object_list,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &workspace_toolbar,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &tools_toolbar,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_toolbar,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        for tool_button in &tool_buttons {
            bind_keyboard_shortcuts(
                &tool_button.button,
                &state,
                &frame,
                &status_bar,
                &inspector,
                &history,
                &canvas,
                &object_list,
            );
        }
        bind_keyboard_shortcuts(
            &color_picker,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &fill_toggle,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &triangle_mode_choice,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &history_list,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &switch_session_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &undo_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &redo_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &clear_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &pin_toolbar_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &workspace_help_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &workspace_session_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &select_all_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &clear_selection_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &delete_selected_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &session_panel_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_panel_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &inspector_panel_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &history_panel_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &reset_layout_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &tools_dock_top_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &tools_dock_left_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &tools_dock_right_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &tools_dock_bottom_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_pane_toggle_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_dock_top_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_dock_left_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_dock_right_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );
        bind_keyboard_shortcuts(
            &objects_dock_bottom_button,
            &state,
            &frame,
            &status_bar,
            &inspector,
            &history,
            &canvas,
            &object_list,
        );

        pin_toolbar_button.on_click({
            let manager = manager;
            let view_controls = view_controls;
            let canvas = canvas;
            let layout_state = Rc::clone(&layout_state);
            move |_| {
                let visible = view_controls.pin_toolbar_button.get_value();
                update_layout_state(&layout_state, manager, &canvas, view_controls, |layout| {
                    apply_layout_action(layout, LayoutAction::SetToolbarClusterVisibility(visible))
                });
            }
        });
        bind_layout_action_bitmap_button(
            &session_panel_button,
            LayoutAction::ToggleSessionPanel,
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &objects_panel_button,
            LayoutAction::ToggleObjectsPanel,
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &inspector_panel_button,
            LayoutAction::ToggleInspectorPanel,
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &history_panel_button,
            LayoutAction::ToggleHistoryPanel,
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &reset_layout_button,
            LayoutAction::ResetLayout,
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &tools_dock_top_button,
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Top),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &tools_dock_left_button,
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Left),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &tools_dock_right_button,
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Right),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &tools_dock_bottom_button,
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Bottom),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &objects_pane_toggle_button,
            LayoutAction::ToggleObjectsPanel,
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &objects_dock_top_button,
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Top),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &objects_dock_left_button,
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Left),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &objects_dock_right_button,
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Right),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        bind_layout_action_bitmap_button(
            &objects_dock_bottom_button,
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Bottom),
            manager,
            &canvas,
            &layout_state,
            view_controls,
        );
        frame.on_menu({
            let manager = manager;
            let canvas = canvas;
            let view_controls = view_controls;
            let layout_state = Rc::clone(&layout_state);
            let frame = frame;
            let open_session_action = Rc::clone(&open_session_action);
            move |event| {
                if handle_layout_menu_action(
                    event.get_id(),
                    &layout_state,
                    &manager,
                    &canvas,
                    view_controls,
                ) {
                    return;
                }

                match event.get_id() {
                    MENU_ID_OPEN_SESSION => open_session_action(),
                    MENU_ID_HELP_REFERENCE => {
                        show_info_dialog(&frame, "Usage Reference", HELP_REFERENCE_TEXT)
                    }
                    MENU_ID_HELP_SHORTCUTS => {
                        show_info_dialog(&frame, "Keyboard Shortcuts", HELP_SHORTCUTS_TEXT)
                    }
                    ID_ABOUT => show_about_dialog(&frame),
                    ID_EXIT => frame.close(true),
                    _ => {}
                }
            }
        });
        canvas.on_erase_background(|event| event.skip(false));

        canvas.on_mouse_left_down({
            let state = Rc::clone(&state);
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let frame = frame;
            let status_bar = status_bar;
            let object_list = object_list;
            let history = history.clone();
            move |event| {
                let Some(position) = mouse_position(&event) else {
                    return;
                };
                let screen_point = CanvasPoint::new(position.x, position.y);
                canvas.set_focus();
                let multi_select_active = mouse_command_multi_select_active(&event);

                let mut should_refresh = false;
                let mut should_sync_ui = false;
                let mut should_return = false;
                let mut should_capture_mouse = false;
                {
                    let mut state = state.borrow_mut();
                    let point = state.viewport.screen_to_world_canvas_point(screen_point);
                    if multi_select_active && !matches!(state.active_tool, Tool::Select) {
                        if let Some(object_id) = topmost_selectable_object_id(&state.scene, point) {
                            let mut selected_ids = state.scene.selected_object_ids.clone();
                            if selected_ids
                                .iter()
                                .any(|selected_id| selected_id == &object_id)
                            {
                                selected_ids.retain(|selected_id| selected_id != &object_id);
                            } else {
                                selected_ids.push(object_id);
                            }
                            if let Err(error) = commit_selection_change(&mut state, selected_ids) {
                                eprintln!("failed to save selection change: {error:#}");
                            }
                            should_sync_ui = true;
                            should_refresh = true;
                        }
                        should_return = true;
                    } else {
                        let selected_color = state.selected_color;
                        match state.active_tool {
                            Tool::Fill => {
                                if let Some(object_id) = state
                                    .scene
                                    .topmost_fillable_object_at(point)
                                    .map(|object| object.id.clone())
                                {
                                    state.push_undo_snapshot();
                                    if let Err(error) =
                                        commit_fill_object(&mut state, object_id, selected_color)
                                    {
                                        eprintln!("failed to save fill operation: {error:#}");
                                    }
                                    should_sync_ui = true;
                                    should_refresh = true;
                                }
                            }
                            Tool::Select => {
                                state.draft = Some(DraftShape::SelectionBox {
                                    start: point,
                                    current: point,
                                    additive: multi_select_active,
                                });
                                should_refresh = true;
                                should_capture_mouse = true;
                            }
                            Tool::Pen => {
                                state.draft = Some(DraftShape::Stroke {
                                    points: vec![point],
                                });
                                should_refresh = true;
                                should_capture_mouse = true;
                            }
                            Tool::Line => {
                                state.draft = Some(DraftShape::Line {
                                    start: point,
                                    current: point,
                                });
                                should_refresh = true;
                                should_capture_mouse = true;
                            }
                            Tool::Rectangle => {
                                state.draft = Some(DraftShape::Rectangle {
                                    start: point,
                                    current: point,
                                });
                                should_refresh = true;
                                should_capture_mouse = true;
                            }
                            Tool::Circle => {
                                state.draft = Some(DraftShape::Circle {
                                    start: point,
                                    current: point,
                                });
                                should_refresh = true;
                                should_capture_mouse = true;
                            }
                            Tool::Triangle => match state.triangle_mode {
                                TriangleMode::Quick => {
                                    state.draft = Some(DraftShape::Triangle {
                                        start: point,
                                        current: point,
                                    });
                                    should_refresh = true;
                                    should_capture_mouse = true;
                                }
                                TriangleMode::ThreePoint => {
                                    let mut completed_triangle: Option<[CanvasPoint; 3]> = None;
                                    match state.draft.as_mut() {
                                        Some(DraftShape::TrianglePoints { points, current }) => {
                                            if points.last().copied() != Some(point) {
                                                points.push(point);
                                            }
                                            *current = point;
                                            if points.len() == 3 {
                                                completed_triangle =
                                                    Some([points[0], points[1], points[2]]);
                                            }
                                        }
                                        _ => {
                                            state.draft = Some(DraftShape::TrianglePoints {
                                                points: vec![point],
                                                current: point,
                                            });
                                        }
                                    }

                                    if let Some(points) = completed_triangle {
                                        let stroke = state.stroke();
                                        let fill =
                                            state.fill_shapes.then_some(state.selected_color);
                                        state.push_undo_snapshot();
                                        let triangle = TriangleObject::new(points);
                                        if let Err(error) =
                                            commit_triangle(&mut state, triangle, stroke, fill)
                                        {
                                            eprintln!("failed to save triangle update: {error:#}");
                                            let _ = state.history.pop_undo();
                                        }
                                        state.draft = None;
                                        should_sync_ui = true;
                                    }
                                    should_refresh = true;
                                }
                            },
                            Tool::Polygon => match state.draft.as_mut() {
                                Some(DraftShape::Polygon { points, current }) => {
                                    if points.last().copied() != Some(point) {
                                        points.push(point);
                                    }
                                    *current = point;
                                    should_refresh = true;
                                }
                                _ => {
                                    state.draft = Some(DraftShape::Polygon {
                                        points: vec![point],
                                        current: point,
                                    });
                                    should_refresh = true;
                                }
                            },
                        }
                    }
                }

                if should_sync_ui {
                    sync_window_ui_with_layout(
                        &state,
                        &frame,
                        &status_bar,
                        &inspector,
                        &history,
                        &object_list,
                        &layout_state,
                        &manager,
                        &canvas,
                        &view_controls,
                    );
                }
                if should_capture_mouse {
                    canvas.capture_mouse();
                }
                if should_refresh {
                    canvas.refresh(false, None);
                }
                if should_return {
                    return;
                }
            }
        });

        canvas.on_mouse_motion({
            let state = Rc::clone(&state);
            let canvas = canvas;
            move |event| {
                let Some(position) = mouse_position(&event) else {
                    return;
                };
                let screen_point = CanvasPoint::new(position.x, position.y);

                let mut should_refresh = false;
                {
                    let mut state = state.borrow_mut();
                    if let Some(pan_drag) = state.pan_drag {
                        let dx = (screen_point.x - pan_drag.start_screen.x) as f64;
                        let dy = (screen_point.y - pan_drag.start_screen.y) as f64;
                        state.viewport = pan_drag.start_viewport;
                        state.viewport.pan_by_screen_delta(dx, dy);
                        should_refresh = true;
                    } else {
                        let point = state.viewport.screen_to_world_canvas_point(screen_point);
                        if let Some(draft) = state.draft.as_mut() {
                            match draft {
                                DraftShape::SelectionBox { current, .. }
                                | DraftShape::Line { current, .. }
                                | DraftShape::Rectangle { current, .. }
                                | DraftShape::Circle { current, .. }
                                | DraftShape::Triangle { current, .. }
                                | DraftShape::TrianglePoints { current, .. }
                                | DraftShape::Polygon { current, .. } => {
                                    *current = point;
                                    should_refresh = true;
                                }
                                DraftShape::Stroke { points } => {
                                    if points.last().copied() != Some(point) {
                                        points.push(point);
                                        should_refresh = true;
                                    }
                                }
                            }
                        }
                    }
                }

                if should_refresh {
                    canvas.refresh(false, None);
                }
            }
        });

        canvas.on_mouse_left_up({
            let state = Rc::clone(&state);
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let frame = frame;
            let status_bar = status_bar;
            let object_list = object_list;
            let history = history.clone();
            move |event| {
                let Some(position) = mouse_position(&event) else {
                    return;
                };
                let screen_point = CanvasPoint::new(position.x, position.y);

                let mut should_refresh = false;
                let mut should_sync_ui = false;
                {
                    let mut state = state.borrow_mut();
                    let point = state.viewport.screen_to_world_canvas_point(screen_point);
                    if matches!(
                        state.draft,
                        Some(DraftShape::Polygon { .. } | DraftShape::TrianglePoints { .. })
                    ) {
                        return;
                    }
                    if let Some(draft) = state.draft.take() {
                        let stroke = state.stroke();
                        let fill = state.fill_shapes.then_some(state.selected_color);
                        let selection_only = matches!(draft, DraftShape::SelectionBox { .. });
                        let object_id = match draft {
                            DraftShape::SelectionBox {
                                start, additive, ..
                            } => {
                                apply_selection_box(&mut state, start, point, additive);
                                should_sync_ui = true;
                                should_refresh = true;
                                None
                            }
                            DraftShape::Line { start, .. } => {
                                state.push_undo_snapshot();
                                match commit_line(&mut state, start, point, stroke) {
                                    Ok(object_id) => Some(object_id),
                                    Err(error) => {
                                        eprintln!("failed to save scene update: {error:#}");
                                        let _ = state.history.pop_undo();
                                        None
                                    }
                                }
                            }
                            DraftShape::Rectangle { start, .. } => {
                                state.push_undo_snapshot();
                                match commit_rectangle(
                                    &mut state,
                                    SceneRect::from_points(start, point),
                                    stroke,
                                    fill,
                                ) {
                                    Ok(object_id) => Some(object_id),
                                    Err(error) => {
                                        eprintln!("failed to save scene update: {error:#}");
                                        let _ = state.history.pop_undo();
                                        None
                                    }
                                }
                            }
                            DraftShape::Circle { start, .. } => {
                                state.push_undo_snapshot();
                                let radius = circle_radius(start, point);
                                let center = circle_center(start, point);
                                match commit_circle(&mut state, center, radius, stroke, fill) {
                                    Ok(object_id) => Some(object_id),
                                    Err(error) => {
                                        eprintln!("failed to save scene update: {error:#}");
                                        let _ = state.history.pop_undo();
                                        None
                                    }
                                }
                            }
                            DraftShape::Triangle { start, .. } => {
                                state.push_undo_snapshot();
                                match commit_triangle(
                                    &mut state,
                                    TriangleObject::from_rect(SceneRect::from_points(start, point)),
                                    stroke,
                                    fill,
                                ) {
                                    Ok(object_id) => Some(object_id),
                                    Err(error) => {
                                        eprintln!("failed to save scene update: {error:#}");
                                        let _ = state.history.pop_undo();
                                        None
                                    }
                                }
                            }
                            DraftShape::TrianglePoints { .. } => None,
                            DraftShape::Polygon { .. } => None,
                            DraftShape::Stroke { mut points } => {
                                state.push_undo_snapshot();
                                if points.last().copied() != Some(point) {
                                    points.push(point);
                                }
                                match commit_stroke(&mut state, points, stroke) {
                                    Ok(object_id) => object_id,
                                    Err(error) => {
                                        eprintln!("failed to save scene update: {error:#}");
                                        let _ = state.history.pop_undo();
                                        None
                                    }
                                }
                            }
                        };
                        if object_id.is_some() {
                            should_sync_ui = true;
                            should_refresh = true;
                        } else if !selection_only {
                            let _ = state.history.pop_undo();
                        }
                    }
                }

                let should_release_mouse = canvas.has_capture();
                if should_release_mouse {
                    canvas.release_mouse();
                }
                if should_sync_ui {
                    sync_window_ui_with_layout(
                        &state,
                        &frame,
                        &status_bar,
                        &inspector,
                        &history,
                        &object_list,
                        &layout_state,
                        &manager,
                        &canvas,
                        &view_controls,
                    );
                }
                if should_refresh {
                    canvas.refresh(false, None);
                }
            }
        });

        canvas.on_mouse_right_down({
            let state = Rc::clone(&state);
            let canvas = canvas;
            let frame = frame;
            let status_bar = status_bar;
            let object_list = object_list;
            let inspector = inspector;
            let history = history.clone();
            move |_| {
                if finalize_multi_point_draft(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                ) {
                    canvas.refresh(false, None);
                }
            }
        });

        canvas.on_mouse_middle_down({
            let state = Rc::clone(&state);
            let canvas = canvas;
            move |event| {
                let Some(position) = mouse_position(&event) else {
                    return;
                };
                let screen_point = CanvasPoint::new(position.x, position.y);
                {
                    let mut state = state.borrow_mut();
                    state.pan_drag = Some(PanDragState {
                        start_screen: screen_point,
                        start_viewport: state.viewport,
                    });
                }
                canvas.capture_mouse();
            }
        });

        canvas.on_mouse_middle_up({
            let state = Rc::clone(&state);
            let canvas = canvas;
            move |_| {
                {
                    let mut state = state.borrow_mut();
                    state.pan_drag = None;
                }
                if canvas.has_capture() {
                    canvas.release_mouse();
                }
                canvas.refresh(false, None);
            }
        });

        canvas.on_mouse_wheel({
            let state = Rc::clone(&state);
            let canvas = canvas;
            move |event| {
                let WindowEventData::General(general) = event else {
                    return;
                };
                let Some(position) = general.get_position() else {
                    return;
                };
                let wheel_delta = general.get_wheel_delta().max(1) as f64;
                let steps = general.get_wheel_rotation() as f64 / wheel_delta;
                if steps.abs() < f64::EPSILON {
                    return;
                }
                let focus = GraphPoint::new(position.x as f64, position.y as f64);
                {
                    let mut state = state.borrow_mut();
                    state
                        .viewport
                        .zoom_around_screen_point(focus, 1.1_f64.powf(steps));
                }
                canvas.refresh(false, None);
                general.skip(false);
            }
        });

        canvas.on_paint({
            let state = Rc::clone(&state);
            move |_| {
                let dc = AutoBufferedPaintDC::new(&canvas);
                let Some(state) = try_borrow_paint_state(&state) else {
                    return;
                };
                dc.set_background(as_wx_color(state.scene.background));
                dc.clear();
                draw_canvas_grid(&dc, &state.viewport, canvas.get_client_size());

                for object in &state.scene.objects {
                    draw_object(&dc, &state.scene, object, &state.viewport);
                }

                for selected_object in state.scene.selected_objects() {
                    draw_selection_highlight(&dc, &state.scene, selected_object, &state.viewport);
                }

                if let Some(draft) = &state.draft {
                    draw_draft(
                        &dc,
                        draft,
                        state.selected_color,
                        state.fill_shapes,
                        &state.viewport,
                    );
                }
            }
        });

        let timer = Rc::new(Timer::new(&frame));
        timer.on_tick({
            let state = Rc::clone(&state);
            let canvas = canvas;
            let manager = manager;
            let layout_state = Rc::clone(&layout_state);
            let view_controls = view_controls;
            let frame = frame;
            let object_list = object_list;
            let scene_label = scene_label;
            let status_bar = status_bar;
            let inspector = inspector;
            let keep_alive = Rc::clone(&timer);
            let runtime_events = runtime_events.clone();
            let history = history.clone();
            move |_| {
                let _keep_alive = &keep_alive;
                {
                    let mut state = state.borrow_mut();
                    if let Err(error) = sync_session_window_manifest(&mut state, &frame) {
                        eprintln!("failed to persist session window state: {error:#}");
                    }
                }
                if let Some(runtime_events) = runtime_events.as_ref() {
                    loop {
                        let next_event = runtime_events.borrow_mut().try_recv();
                        match next_event {
                            Ok(RuntimeEvent::Attached(snapshot))
                            | Ok(RuntimeEvent::SceneUpdated(snapshot)) => {
                                let update_kind;
                                let log_message;
                                {
                                    let mut state = state.borrow_mut();
                                    update_kind = apply_runtime_scene_update(&mut state, snapshot);
                                    log_message = format!(
                                        "applied runtime scene update kind={:?} session={:?} scene={} object_count={}",
                                        update_kind,
                                        state.session_id,
                                        state.scene_path.display(),
                                        state.scene.objects.len()
                                    );
                                }
                                sync_window_ui_with_layout(
                                    &state,
                                    &frame,
                                    &status_bar,
                                    &inspector,
                                    &history,
                                    &object_list,
                                    &layout_state,
                                    &manager,
                                    &canvas,
                                    &view_controls,
                                );
                                log_gui(&log_message);
                                if update_kind == RuntimeSceneUpdateKind::ExternalRefresh {
                                    canvas.refresh(false, None);
                                }
                            }
                            Ok(RuntimeEvent::Error(error)) => {
                                log_gui(&format!("runtime subscription error: {error}"));
                            }
                            Err(mpsc::TryRecvError::Empty) => break,
                            Err(mpsc::TryRecvError::Disconnected) => break,
                        }
                    }
                } else {
                    let changed_scene = {
                        let state_ref = state.borrow();
                        load_scene_if_changed(&state_ref.scene_path, state_ref.last_seen)
                    };

                    match changed_scene {
                        Ok(Some((scene, modified))) => {
                            let log_message;
                            {
                                let mut state = state.borrow_mut();
                                state.scene = scene;
                                state.last_seen = Some(modified);
                                state.clear_history();
                                let existing_ids: Vec<String> = state
                                    .scene
                                    .objects
                                    .iter()
                                    .map(|object| object.id.clone())
                                    .collect();
                                state.scene.selected_object_ids.retain(|selected_id| {
                                    existing_ids
                                        .iter()
                                        .any(|object_id| object_id == selected_id)
                                });
                                log_message = format!(
                                    "reloaded scene from disk session={:?} scene={} object_count={} selected_count={}",
                                    state.session_id,
                                    state.scene_path.display(),
                                    state.scene.objects.len(),
                                    state.scene.selected_object_ids.len()
                                );
                            }
                            sync_window_ui_with_layout(
                                &state,
                                &frame,
                                &status_bar,
                                &inspector,
                                &history,
                                &object_list,
                                &layout_state,
                                &manager,
                                &canvas,
                                &view_controls,
                            );
                            log_gui(&log_message);
                            canvas.refresh(false, None);
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("failed to reload scene: {error:#}"),
                    }
                }

                let state = state.borrow();
                update_status_bar(
                    &status_bar,
                    &state,
                    &session_server_status(state.session_manifest_path.as_ref()),
                );
                update_inspector_panel(&inspector, &state);
                scene_label.set_label(&scene_summary_label(
                    state.session_id.as_deref(),
                    &state.scene_path,
                    &session_server_status(state.session_manifest_path.as_ref()),
                ));
                scene_label.wrap(SECTION_CONTENT_WIDTH);
            }
        });
        let _ = timer.start(600, false);

        frame.on_close({
            let state = Rc::clone(&state);
            move |event| {
                let state_ref = state.borrow();
                log_gui(&format!(
                    "window closing session={:?} scene={}",
                    state_ref.session_id,
                    state_ref.scene_path.display()
                ));
                if let Err(error) = remove_session_window_manifest(&state.borrow()) {
                    eprintln!("failed to remove session window state on close: {error:#}");
                }
                event.skip(true);
            }
        });

        frame.show(true);
        frame.set_min_size(Size::new(1200, 820));
        canvas.set_focus();
    })
}

fn spawn_runtime_subscription(
    runtime_url: String,
    requested_session_id: Option<String>,
) -> mpsc::Receiver<RuntimeEvent> {
    let (event_tx, event_rx) = mpsc::channel();
    thread::spawn(move || {
        let requested_session_id_for_subscription = requested_session_id.clone();
        let attach_result = runtime_request::<SessionAttachParams, RuntimeSceneSnapshot>(
            &runtime_url,
            METHOD_SESSION_ATTACH,
            SessionAttachParams {
                session_id: requested_session_id,
                create_if_missing: true,
            },
        );
        match attach_result {
            Ok(snapshot) => {
                let _ = event_tx.send(RuntimeEvent::Attached(snapshot));
            }
            Err(error) => {
                let _ = event_tx.send(RuntimeEvent::Error(error.to_string()));
                return;
            }
        }

        let Ok((mut socket, _)) = ws_connect(runtime_url.as_str()) else {
            let _ = event_tx.send(RuntimeEvent::Error(
                "failed to open runtime websocket subscription".to_string(),
            ));
            return;
        };
        let attach_request = JsonRpcRequest::new(
            1,
            METHOD_SESSION_ATTACH,
            SessionAttachParams {
                session_id: requested_session_id_for_subscription,
                create_if_missing: false,
            },
        );
        let attach_payload = match serde_json::to_string(&attach_request) {
            Ok(payload) => payload,
            Err(error) => {
                let _ = event_tx.send(RuntimeEvent::Error(format!(
                    "failed to encode runtime attach request: {error}"
                )));
                return;
            }
        };
        if let Err(error) = socket.send(WsMessage::Text(attach_payload.into())) {
            let _ = event_tx.send(RuntimeEvent::Error(format!(
                "failed to send runtime attach request: {error}"
            )));
            return;
        }

        loop {
            let message = match socket.read() {
                Ok(message) => message,
                Err(error) => {
                    let _ = event_tx.send(RuntimeEvent::Error(format!(
                        "runtime websocket read failed: {error}"
                    )));
                    return;
                }
            };
            match message {
                WsMessage::Text(text) => {
                    if let Ok(notification) =
                        serde_json::from_str::<JsonRpcNotification>(text.as_ref())
                    {
                        if notification.method == METHOD_SCENE_UPDATED {
                            match decode_runtime_params::<boardmesh::SceneUpdatedParams>(
                                notification.params,
                            ) {
                                Ok(update) => {
                                    let _ =
                                        event_tx.send(RuntimeEvent::SceneUpdated(update.snapshot));
                                }
                                Err(error) => {
                                    let _ = event_tx.send(RuntimeEvent::Error(error.to_string()));
                                    return;
                                }
                            }
                        }
                    }
                }
                WsMessage::Close(_) => return,
                _ => {}
            }
        }
    });
    event_rx
}

fn decode_runtime_params<T: serde::de::DeserializeOwned>(
    params: Option<serde_json::Value>,
) -> anyhow::Result<T> {
    serde_json::from_value(params.unwrap_or(serde_json::Value::Null))
        .map_err(|error| anyhow::anyhow!("invalid runtime params: {error}"))
}

fn apply_runtime_scene_update(
    state: &mut BoardmeshState,
    snapshot: RuntimeSceneSnapshot,
) -> RuntimeSceneUpdateKind {
    let next_scene_path = PathBuf::from(&snapshot.scene_path);
    let next_manifest_path = snapshot.manifest_path.map(PathBuf::from);
    let is_self_echo = state.scene == snapshot.scene
        && state.scene_path == next_scene_path
        && state.session_id == snapshot.session_id
        && state.session_manifest_path == next_manifest_path;

    state.last_seen = last_modified(&next_scene_path).ok();
    if is_self_echo {
        return RuntimeSceneUpdateKind::NoOp;
    }

    state.scene = snapshot.scene;
    state.scene_path = next_scene_path;
    state.session_id = snapshot.session_id;
    state.session_manifest_path = next_manifest_path;
    state.clear_history();
    RuntimeSceneUpdateKind::ExternalRefresh
}

fn parse_options() -> anyhow::Result<AppOptions> {
    let mut session_root = PathBuf::from(boardmesh::DEFAULT_SESSIONS_DIR);
    let mut explicit_scene_path = None;
    let mut session_id = None;
    let mut runtime_url = Some(DEFAULT_RUNTIME_URL.to_string());
    let mut initial_tool = Tool::Pen;
    let mut initial_triangle_mode = TriangleMode::Quick;
    let mut initial_fill_shapes = false;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if argument == "--sessions-dir" {
            if let Some(path) = args.next() {
                session_root = PathBuf::from(path);
            }
            continue;
        }

        if argument == "--scene" {
            if let Some(path) = args.next() {
                explicit_scene_path = Some(PathBuf::from(path));
                session_id = None;
            }
            continue;
        }

        if argument == "--runtime-url" {
            if let Some(url) = args.next() {
                runtime_url = Some(url);
            }
            continue;
        }

        if argument == "--no-runtime" {
            runtime_url = None;
            continue;
        }

        if argument == "--session" {
            if let Some(id) = args.next() {
                session_id = Some(id);
            }
            continue;
        }

        if argument == "--tool" {
            if let Some(tool) = args.next() {
                initial_tool = Tool::from_cli_name(&tool);
            }
            continue;
        }

        if argument == "--triangle-mode" {
            if let Some(mode) = args.next() {
                initial_triangle_mode = TriangleMode::from_cli_name(&mode);
            }
            continue;
        }

        if argument == "--filled-shapes" {
            initial_fill_shapes = true;
        }
    }

    let explicit_scene_mode = explicit_scene_path.is_some();
    let scene_path = match (&session_id, explicit_scene_path) {
        (_, Some(scene_path)) => scene_path,
        (Some(session_id), None) => resolve_session_path(&session_root, session_id)?,
        (None, None) => PathBuf::from(boardmesh::DEFAULT_SCENE_FILE),
    };

    Ok(AppOptions {
        session_root,
        scene_path,
        session_id,
        runtime_url,
        initial_tool,
        initial_triangle_mode,
        initial_fill_shapes,
        explicit_scene_mode,
    })
}

#[derive(Debug, Clone)]
struct StartupTarget {
    scene_path: PathBuf,
    session_id: Option<String>,
}

fn resolve_startup_target(
    frame: &Frame,
    options: &AppOptions,
) -> anyhow::Result<Option<StartupTarget>> {
    if let Some(session_id) = options.session_id.as_deref() {
        return Ok(Some(StartupTarget {
            scene_path: resolve_session_path(&options.session_root, session_id)?,
            session_id: Some(session_id.to_string()),
        }));
    }

    if options.explicit_scene_mode {
        return Ok(Some(StartupTarget {
            scene_path: options.scene_path.clone(),
            session_id: None,
        }));
    }

    let selected_session_id = choose_session_interactively(
        frame,
        &options.session_root,
        options.runtime_url.as_deref(),
        None,
        "Open Board Session",
        "Select a board session to open, or create a new one.",
    )?;

    Ok(selected_session_id.map(|session_id| StartupTarget {
        scene_path: resolve_session_path(&options.session_root, &session_id)
            .expect("validated session id should resolve"),
        session_id: Some(session_id),
    }))
}

fn choose_session_interactively(
    parent: &Frame,
    session_root: &PathBuf,
    runtime_url: Option<&str>,
    current_session_id: Option<&str>,
    caption: &str,
    message: &str,
) -> anyhow::Result<Option<String>> {
    let mut session_ids = runtime_url
        .and_then(|runtime_url| {
            runtime_request::<serde_json::Value, RuntimeSessionListResult>(
                runtime_url,
                METHOD_SESSION_LIST,
                serde_json::json!({}),
            )
            .map(|result| {
                result
                    .sessions
                    .into_iter()
                    .map(|session| session.session_id)
                    .collect::<Vec<_>>()
            })
            .ok()
        })
        .unwrap_or(list_session_ids(session_root)?);
    session_ids.sort();

    if session_ids.is_empty() {
        return prompt_for_new_session_id(parent, session_root, "default-session");
    }

    let create_label = "Create new session...".to_string();
    let mut choices = session_ids.clone();
    choices.push(create_label.clone());
    let choice_refs = choices.iter().map(String::as_str).collect::<Vec<_>>();
    let dialog = SingleChoiceDialog::builder(parent, message, caption, &choice_refs).build();

    if let Some(current_session_id) = current_session_id {
        if let Some(index) = session_ids
            .iter()
            .position(|session_id| session_id == current_session_id)
        {
            dialog.set_selection(index as i32);
        } else {
            dialog.set_selection(0);
        }
    } else {
        dialog.set_selection(0);
    }

    if dialog.show_modal() != ID_OK {
        return Ok(None);
    }

    let selected_value = dialog
        .get_string_selection()
        .or_else(|| {
            let selection = dialog.get_selection();
            (selection >= 0)
                .then(|| choices.get(selection as usize).cloned())
                .flatten()
        })
        .unwrap_or_default();

    if selected_value == create_label {
        prompt_for_new_session_id(
            parent,
            session_root,
            current_session_id.unwrap_or("new-session"),
        )
    } else if selected_value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(selected_value))
    }
}

fn prompt_for_new_session_id(
    parent: &Frame,
    session_root: &PathBuf,
    default_value: &str,
) -> anyhow::Result<Option<String>> {
    let mut current_default = default_value.to_string();

    loop {
        let dialog =
            TextEntryDialog::builder(parent, "Enter a new board session id.", "Create Session")
                .with_default_value(&current_default)
                .build();

        if dialog.show_modal() != ID_OK {
            return Ok(None);
        }

        let entered_value = dialog.get_value().unwrap_or_default().trim().to_string();
        current_default = entered_value.clone();

        if let Err(error) = validate_session_id(&entered_value) {
            show_error_dialog(parent, "Invalid session id", &error.to_string());
            continue;
        }

        let manifest_path = session_manifest_path(session_root, &entered_value);
        if manifest_path.exists() {
            show_error_dialog(
                parent,
                "Session already exists",
                &format!("A session named '{entered_value}' already exists."),
            );
            continue;
        }

        return Ok(Some(entered_value));
    }
}

fn triangle_mode_index(mode: TriangleMode) -> u32 {
    match mode {
        TriangleMode::Quick => 0,
        TriangleMode::ThreePoint => 1,
    }
}

fn tool_hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select objects with click, Ctrl-click, or drag to box-select.",
        Tool::Pen => "Freehand drawing for quick annotations and loose diagrams.",
        Tool::Line => "Drag to connect nodes or sketch straight separators.",
        Tool::Rectangle => "Drag to create cards, swimlanes, and containers.",
        Tool::Circle => "Drag to create circular callouts or nodes.",
        Tool::Triangle => "Use quick drag or 3-point mode for directional markers.",
        Tool::Polygon => "Click to place vertices. Press Enter or right-click to finish.",
        Tool::Fill => "Click a shape to recolor it with the current selected color.",
    }
}

fn tool_name(tool: Tool) -> &'static str {
    match tool {
        Tool::Select => "Select",
        Tool::Pen => "Pen",
        Tool::Line => "Line",
        Tool::Rectangle => "Rectangle",
        Tool::Circle => "Circle",
        Tool::Triangle => "Triangle",
        Tool::Polygon => "Polygon",
        Tool::Fill => "Fill",
    }
}

fn active_tool_button_color() -> Colour {
    Colour::new(221, 238, 255, 255)
}

fn inactive_tool_button_color() -> Colour {
    Colour::new(255, 255, 255, 255)
}

fn toolbar_section_label_color() -> Colour {
    Colour::new(100, 116, 139, 255)
}

fn toolbar_divider_color() -> Colour {
    Colour::new(226, 232, 240, 255)
}

fn toolbar_pane_info(toolbar: ToolbarPane, direction: ToolbarDock) -> AuiPaneInfo {
    let (name, caption, best_width, best_height, min_width, min_height, position) = match toolbar {
        ToolbarPane::Workspace => (
            "workspace-toolbar",
            "Workspace Toolbar",
            WORKSPACE_TOOLBAR_WIDTH,
            WORKSPACE_TOOLBAR_HEIGHT,
            320,
            56,
            0,
        ),
        ToolbarPane::Tools => (
            "tools-toolbar",
            "Tools Toolbar",
            SIDE_TOOLBAR_WIDTH,
            TOOLS_TOOLBAR_HEIGHT,
            SIDE_TOOLBAR_WIDTH,
            240,
            1,
        ),
        ToolbarPane::Objects => (
            "objects-toolbar",
            "Objects Toolbar",
            SIDE_TOOLBAR_WIDTH,
            OBJECTS_TOOLBAR_HEIGHT,
            SIDE_TOOLBAR_WIDTH,
            220,
            2,
        ),
    };

    let pane = AuiPaneInfo::new()
        .with_name(name)
        .with_caption(caption)
        .toolbar_pane()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(false)
        .close_button(false)
        .pin_button(false)
        .caption_visible(false)
        .gripper(false)
        .best_size(best_width, best_height)
        .min_size(min_width, min_height)
        .row(0)
        .position(position);

    match direction {
        ToolbarDock::Left => pane.left(),
        ToolbarDock::Right => pane.right(),
        ToolbarDock::Top => pane.top(),
        ToolbarDock::Bottom => pane.bottom(),
    }
}

fn sync_tool_palette(tool_buttons: &[ToolButtonControl], active_tool: Tool) {
    for tool_button in tool_buttons {
        let is_active = tool_button.tool == active_tool;
        tool_button.button.set_background_color(if is_active {
            active_tool_button_color()
        } else {
            inactive_tool_button_color()
        });
        tool_button.button.refresh(false, None);
        tool_button.button.update();
    }
}

fn reset_camera(state: &Rc<RefCell<BoardmeshState>>, canvas: &Panel) -> bool {
    {
        let mut state = state.borrow_mut();
        state.viewport.reset();
        state.pan_drag = None;
    }
    if canvas.has_capture() {
        canvas.release_mouse();
    }
    canvas.refresh(false, None);
    true
}

fn fit_scene_camera(state: &Rc<RefCell<BoardmeshState>>, canvas: &Panel) -> bool {
    let bounds = {
        let state = state.borrow();
        scene_bounds(&state.scene)
    };
    fit_camera_to_bounds(state, canvas, bounds)
}

fn fit_selection_camera(state: &Rc<RefCell<BoardmeshState>>, canvas: &Panel) -> bool {
    let bounds = {
        let state = state.borrow();
        selected_bounds(&state.scene)
    };
    fit_camera_to_bounds(state, canvas, bounds)
}

fn fit_camera_to_bounds(
    state: &Rc<RefCell<BoardmeshState>>,
    canvas: &Panel,
    bounds: Option<SceneRect>,
) -> bool {
    let Some(bounds) = bounds else {
        return false;
    };
    let canvas_size = canvas.get_client_size();
    let Some(viewport) = fitted_viewport_for_canvas(bounds, canvas_size) else {
        return false;
    };
    {
        let mut state = state.borrow_mut();
        state.viewport = viewport;
        state.pan_drag = None;
    }
    if canvas.has_capture() {
        canvas.release_mouse();
    }
    canvas.refresh(false, None);
    true
}

fn fitted_viewport_for_canvas(bounds: SceneRect, canvas_size: Size) -> Option<ViewportState> {
    ViewportState::fit_world_bounds(bounds, canvas_size.width, canvas_size.height, 48.0)
}

fn persist(state: &mut BoardmeshState) -> anyhow::Result<()> {
    if let Some(runtime_url) = current_gui_runtime_url() {
        let snapshot = runtime_request::<SceneReplaceParams, RuntimeSceneSnapshot>(
            &runtime_url,
            METHOD_SCENE_REPLACE,
            SceneReplaceParams {
                session_id: state.session_id.clone(),
                scene: state.scene.clone(),
            },
        )?;
        state.scene = snapshot.scene;
        state.scene_path = PathBuf::from(&snapshot.scene_path);
        state.session_id = snapshot.session_id;
        state.session_manifest_path = snapshot.manifest_path.map(PathBuf::from);
        state.last_seen = last_modified(&state.scene_path).ok();
    } else {
        let mut workspace = state.local_workspace();
        workspace.replace_scene(state.scene.clone())?;
        state.sync_from_workspace(workspace);
    }
    log_gui(&format!(
        "persisted scene session={:?} scene={} object_count={} selected_count={}",
        state.session_id,
        state.scene_path.display(),
        state.scene.objects.len(),
        state.scene.selected_object_ids.len()
    ));
    Ok(())
}

fn apply_local_workspace<T>(
    state: &mut BoardmeshState,
    action: impl FnOnce(&mut BoardWorkspace) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let mut workspace = state.local_workspace();
    let result = action(&mut workspace)?;
    state.sync_from_workspace(workspace);
    Ok(result)
}

fn commit_selection_change(
    state: &mut BoardmeshState,
    selected_ids: Vec<String>,
) -> anyhow::Result<()> {
    if current_gui_runtime_url().is_some() {
        let _ = state.scene.select_objects(selected_ids);
        persist(state)
    } else {
        let _ = apply_local_workspace(state, move |workspace| {
            workspace.select_objects(selected_ids)
        })?;
        Ok(())
    }
}

fn commit_clear_scene(state: &mut BoardmeshState) -> anyhow::Result<()> {
    state.draft = None;
    if current_gui_runtime_url().is_some() {
        state.scene.clear();
        persist(state)
    } else {
        apply_local_workspace(state, |workspace| {
            workspace.clear_scene()?;
            Ok(())
        })?;
        Ok(())
    }
}

fn commit_delete_selected(state: &mut BoardmeshState) -> anyhow::Result<usize> {
    let selected_ids = state.scene.selected_object_ids.clone();
    if current_gui_runtime_url().is_some() {
        let deleted = state.scene.delete_objects(&selected_ids);
        if deleted > 0 {
            persist(state)?;
        }
        Ok(deleted)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.delete_objects(&selected_ids)
        })
    }
}

fn commit_fill_object(
    state: &mut BoardmeshState,
    object_id: String,
    fill: SceneColor,
) -> anyhow::Result<bool> {
    if current_gui_runtime_url().is_some() {
        let updated = state.scene.fill_object(&object_id, fill);
        if updated {
            let _ = state.scene.select_objects(vec![object_id]);
            persist(state)?;
        }
        Ok(updated)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.fill_object(&object_id, fill)
        })
    }
}

fn commit_triangle(
    state: &mut BoardmeshState,
    triangle: TriangleObject,
    stroke: SceneStroke,
    fill: Option<SceneColor>,
) -> anyhow::Result<String> {
    if current_gui_runtime_url().is_some() {
        let object_id = state.scene.add_triangle(triangle, stroke, fill);
        let _ = state.scene.select_objects(vec![object_id.clone()]);
        persist(state)?;
        Ok(object_id)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.add_triangle(triangle, stroke, fill)
        })
    }
}

fn commit_line(
    state: &mut BoardmeshState,
    start: CanvasPoint,
    end: CanvasPoint,
    stroke: SceneStroke,
) -> anyhow::Result<String> {
    if current_gui_runtime_url().is_some() {
        let object_id = state.scene.add_line(start, end, stroke);
        let _ = state.scene.select_objects(vec![object_id.clone()]);
        persist(state)?;
        Ok(object_id)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.add_line(start, end, stroke)
        })
    }
}

fn commit_rectangle(
    state: &mut BoardmeshState,
    rect: SceneRect,
    stroke: SceneStroke,
    fill: Option<SceneColor>,
) -> anyhow::Result<String> {
    if current_gui_runtime_url().is_some() {
        let object_id = state.scene.add_rectangle(rect, stroke, fill);
        let _ = state.scene.select_objects(vec![object_id.clone()]);
        persist(state)?;
        Ok(object_id)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.add_rectangle(rect, stroke, fill)
        })
    }
}

fn commit_circle(
    state: &mut BoardmeshState,
    center: CanvasPoint,
    radius: i32,
    stroke: SceneStroke,
    fill: Option<SceneColor>,
) -> anyhow::Result<String> {
    if current_gui_runtime_url().is_some() {
        let object_id = state.scene.add_circle(center, radius, stroke, fill);
        let _ = state.scene.select_objects(vec![object_id.clone()]);
        persist(state)?;
        Ok(object_id)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.add_circle(center, radius, stroke, fill)
        })
    }
}

fn commit_stroke(
    state: &mut BoardmeshState,
    points: Vec<CanvasPoint>,
    stroke: SceneStroke,
) -> anyhow::Result<Option<String>> {
    if current_gui_runtime_url().is_some() {
        let object_id = state.scene.add_stroke(points, stroke);
        if let Some(object_id) = object_id.as_ref() {
            let _ = state.scene.select_objects(vec![object_id.clone()]);
            persist(state)?;
        }
        Ok(object_id)
    } else {
        apply_local_workspace(state, move |workspace| workspace.add_stroke(points, stroke))
    }
}

fn commit_polygon(
    state: &mut BoardmeshState,
    points: Vec<CanvasPoint>,
    stroke: SceneStroke,
    fill: Option<SceneColor>,
) -> anyhow::Result<Option<String>> {
    if current_gui_runtime_url().is_some() {
        let object_id = state.scene.add_polygon(points, stroke, fill);
        if let Some(object_id) = object_id.as_ref() {
            let _ = state.scene.select_objects(vec![object_id.clone()]);
            persist(state)?;
        }
        Ok(object_id)
    } else {
        apply_local_workspace(state, move |workspace| {
            workspace.add_polygon(points, stroke, fill)
        })
    }
}

fn switch_window_to_session(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
    scene_label: &StaticText,
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: &AuiManager,
    view_controls: ViewControls,
    session_root: &PathBuf,
    session_id: &str,
    runtime_events: Option<&Rc<RefCell<mpsc::Receiver<RuntimeEvent>>>>,
) -> anyhow::Result<bool> {
    if let Some(runtime_url) = current_gui_runtime_url() {
        let snapshot = runtime_request::<SessionSwitchParams, RuntimeSceneSnapshot>(
            &runtime_url,
            METHOD_SESSION_SWITCH,
            SessionSwitchParams {
                session_id: session_id.to_string(),
                create_if_missing: true,
            },
        )?;
        let scene_summary;
        {
            let mut state = state.borrow_mut();
            if state.session_id.as_deref() == Some(session_id)
                && state.scene_path == PathBuf::from(&snapshot.scene_path)
            {
                return Ok(false);
            }
            remove_session_window_manifest(&state)?;
            state.scene_path = PathBuf::from(&snapshot.scene_path);
            state.session_id = snapshot.session_id;
            state.session_manifest_path = snapshot.manifest_path.map(PathBuf::from);
            if state.window_id.is_none() {
                state.window_id = Some(new_window_id());
            }
            state.scene = snapshot.scene;
            state.last_seen = last_modified(&state.scene_path).ok();
            state.last_window_state = None;
            state.draft = None;
            state.clear_history();
            sync_session_window_manifest(&mut state, frame)?;
            log_gui(&format!(
                "switched window via runtime to session={} scene={} object_count={}",
                session_id,
                state.scene_path.display(),
                state.scene.objects.len()
            ));
            scene_summary = scene_summary_label(
                state.session_id.as_deref(),
                &state.scene_path,
                &session_server_status(state.session_manifest_path.as_ref()),
            );
        }
        sync_window_ui_with_layout(
            state,
            frame,
            status_bar,
            inspector,
            history,
            object_list,
            layout_state,
            manager,
            canvas,
            &view_controls,
        );
        scene_label.set_label(&scene_summary);
        scene_label.wrap(SECTION_CONTENT_WIDTH);
        if let Some(runtime_events) = runtime_events {
            let requested_session_id = state.borrow().session_id.clone();
            *runtime_events.borrow_mut() =
                spawn_runtime_subscription(runtime_url, requested_session_id);
        }
        canvas.refresh(false, None);
        return Ok(true);
    }

    let scene_summary;
    {
        let mut state = state.borrow_mut();
        let scene_path = resolve_session_path(session_root, session_id)?;
        if state.session_id.as_deref() == Some(session_id) && state.scene_path == scene_path {
            return Ok(false);
        }

        remove_session_window_manifest(&state)?;
        let mut workspace = state.local_workspace();
        workspace.activate_session(Some(session_id.to_string()), scene_path.clone())?;
        state.sync_from_workspace(workspace);
        if state.window_id.is_none() {
            state.window_id = Some(new_window_id());
        }
        state.last_window_state = None;
        state.draft = None;
        state.clear_history();
        sync_session_window_manifest(&mut state, frame)?;
        log_gui(&format!(
            "switched window to session={} scene={} object_count={}",
            session_id,
            state.scene_path.display(),
            state.scene.objects.len()
        ));
        scene_summary = scene_summary_label(
            state.session_id.as_deref(),
            &state.scene_path,
            &session_server_status(state.session_manifest_path.as_ref()),
        );
    }

    sync_window_ui_with_layout(
        state,
        frame,
        status_bar,
        inspector,
        history,
        object_list,
        layout_state,
        manager,
        canvas,
        &view_controls,
    );
    scene_label.set_label(&scene_summary);
    scene_label.wrap(SECTION_CONTENT_WIDTH);
    canvas.refresh(false, None);
    Ok(true)
}

fn show_error_dialog(parent: &Frame, caption: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, caption)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::Centre)
        .build();
    let _ = dialog.show_modal();
}

fn bind_keyboard_shortcuts<T: WindowEvents>(
    widget: &T,
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) {
    let state = Rc::clone(state);
    let frame = *frame;
    let status_bar = *status_bar;
    let inspector = *inspector;
    let history = history.clone();
    let canvas = *canvas;
    let object_list = *object_list;
    widget.on_key_down(move |event| {
        let WindowEventData::Keyboard(key) = event else {
            return;
        };

        let Some(key_code) = key.get_key_code() else {
            key.event.skip(true);
            return;
        };

        let command_down = key.control_down() || key.cmd_down();
        let handled = if matches!(key_code, WXK_RETURN) {
            finalize_multi_point_draft(
                &state,
                &frame,
                &status_bar,
                &inspector,
                &history,
                &canvas,
                &object_list,
            )
        } else if matches!(key_code, WXK_ESCAPE) {
            cancel_active_draft(&state, &canvas)
        } else if matches!(key_code, WXK_DELETE | WXK_BACKSPACE) {
            delete_selected_objects(
                &state,
                &frame,
                &status_bar,
                &inspector,
                &history,
                &canvas,
                &object_list,
            )
        } else if command_down && key_code == WXK_Z {
            if key.shift_down() {
                redo_last_change(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                )
            } else {
                undo_last_change(
                    &state,
                    &frame,
                    &status_bar,
                    &inspector,
                    &history,
                    &canvas,
                    &object_list,
                )
            }
        } else if command_down && key_code == WXK_Y {
            redo_last_change(
                &state,
                &frame,
                &status_bar,
                &inspector,
                &history,
                &canvas,
                &object_list,
            )
        } else if command_down && key_code == WXK_0 {
            reset_camera(&state, &canvas)
        } else if command_down && key_code == WXK_1 {
            fit_scene_camera(&state, &canvas)
        } else if command_down && key_code == WXK_2 {
            fit_selection_camera(&state, &canvas)
        } else {
            false
        };

        key.event.skip(!handled);
    });
}

fn try_borrow_paint_state(state: &Rc<RefCell<BoardmeshState>>) -> Option<Ref<'_, BoardmeshState>> {
    state.try_borrow().ok()
}

fn layout_context_for_state(state: &BoardmeshState) -> LayoutContext {
    LayoutContext {
        object_count: state.scene.object_count(),
        active_tool_prefers_inspector: !matches!(state.active_tool, Tool::Select),
    }
}

fn apply_contextual_layout_hints_if_needed(
    state: &Rc<RefCell<BoardmeshState>>,
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: &AuiManager,
    canvas: &Panel,
    view_controls: &ViewControls,
) {
    let context = {
        let state = state.borrow();
        layout_context_for_state(&state)
    };

    let changed = {
        let mut layout = layout_state.borrow_mut();
        apply_contextual_layout_hints(&mut layout, context)
    };

    if changed {
        apply_layout(manager, canvas, view_controls, &layout_state.borrow());
        sync_view_controls(view_controls, &layout_state.borrow());
    }
}

fn sync_window_ui(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    object_list: &CheckListBox,
) {
    let state = state.borrow();
    update_window_chrome(frame, status_bar, inspector, history, &state);
    sync_object_list(object_list, &state);
}

fn sync_window_ui_with_layout(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    object_list: &CheckListBox,
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: &AuiManager,
    canvas: &Panel,
    view_controls: &ViewControls,
) {
    apply_contextual_layout_hints_if_needed(state, layout_state, manager, canvas, view_controls);
    let state = state.borrow();
    update_window_chrome(frame, status_bar, inspector, history, &state);
    sync_object_list(object_list, &state);
}

fn update_title(frame: &Frame, state: &BoardmeshState) {
    frame.set_title(&format!(
        "Boardmesh{}  |  {} objects  |  {} selected  |  {}",
        session_title_fragment(state.session_id.as_deref()),
        state.scene.object_count(),
        state.scene.selected_object_ids.len(),
        state.scene_path.display()
    ));
}

fn update_window_chrome(
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    state: &BoardmeshState,
) {
    update_title(frame, state);
    update_status_bar(
        status_bar,
        state,
        &session_server_status(state.session_manifest_path.as_ref()),
    );
    update_inspector_panel(inspector, state);
    sync_history_panel(history, state);
}

fn update_status_bar(status_bar: &StatusBar, state: &BoardmeshState, server_status: &ServerStatus) {
    let board_label = state
        .session_id
        .as_deref()
        .map(|session_id| format!("Session {session_id}"))
        .unwrap_or_else(|| "Raw scene mode".to_string());
    let tool_summary = format!(
        "Tool: {}  |  Objects: {}  |  Selected: {}",
        tool_name(state.active_tool),
        state.scene.object_count(),
        state.scene.selected_object_ids.len()
    );
    status_bar.set_status_text(&board_label, 0);
    status_bar.set_status_text(&tool_summary, 1);
    status_bar.set_status_text(&short_server_status_label(server_status), 2);
}

fn update_inspector_panel(inspector: &InspectorControls, state: &BoardmeshState) {
    inspector
        .title_label
        .set_label(&format!("{} Tool", tool_name(state.active_tool)));
    inspector.hint_label.set_label(tool_hint(state.active_tool));
    inspector.hint_label.wrap(SECTION_CONTENT_WIDTH);
    inspector
        .color_picker
        .set_colour(as_wx_color(state.selected_color));
    inspector.fill_toggle.set_value(state.fill_shapes);
    inspector
        .triangle_choice
        .set_selection(triangle_mode_index(state.triangle_mode));
    inspector
        .selection_label
        .set_label(&selection_summary_text(state));
    inspector.selection_label.wrap(SECTION_CONTENT_WIDTH);
    inspector.state_label.set_label(&tool_state_summary(state));
    inspector.state_label.wrap(SECTION_CONTENT_WIDTH);

    let show_color = tool_supports_color(state.active_tool);
    let show_fill = tool_supports_fill_toggle(state.active_tool);
    let show_triangle = state.active_tool == Tool::Triangle;
    let show_empty = !show_fill && !show_triangle && !show_color;

    inspector.empty_label.show(show_empty);
    inspector.color_label.show(show_color);
    inspector.color_picker.show(show_color);
    inspector.fill_toggle.show(show_fill);
    inspector.triangle_label.show(show_triangle);
    inspector.triangle_choice.show(show_triangle);
    inspector.panel.layout();
    inspector.panel.refresh(false, None);
    inspector.panel.update();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryRowAction {
    Undo(usize),
    Redo(usize),
}

fn history_summary_text(state: &BoardmeshState) -> String {
    format!(
        "Undo: {} available  |  Redo: {} available",
        state.history.undo_len(),
        state.history.redo_len()
    )
}

fn history_row_entries(state: &BoardmeshState) -> Vec<(String, Option<HistoryRowAction>)> {
    let mut rows = Vec::new();
    for (depth, entry) in state.history.undo_entries().iter().rev().enumerate() {
        rows.push((
            format!("Undo: {}", entry.label),
            Some(HistoryRowAction::Undo(depth + 1)),
        ));
    }
    rows.push(("Current scene".to_string(), None));
    for (depth, entry) in state.history.redo_entries().iter().rev().enumerate() {
        rows.push((
            format!("Redo: {}", entry.label),
            Some(HistoryRowAction::Redo(depth + 1)),
        ));
    }
    rows
}

fn history_action_for_selection(state: &BoardmeshState, index: u32) -> Option<HistoryRowAction> {
    history_row_entries(state)
        .get(index as usize)
        .and_then(|(_, action)| *action)
}

fn apply_history_action_to_state(state: &mut BoardmeshState, action: HistoryRowAction) -> bool {
    match action {
        HistoryRowAction::Undo(depth) => (0..depth).all(|_| state.undo()),
        HistoryRowAction::Redo(depth) => (0..depth).all(|_| state.redo()),
    }
}

fn sync_history_panel(history: &HistoryControls, state: &BoardmeshState) {
    history.suppress_selection_events.set(true);
    history
        .summary_label
        .set_label(&history_summary_text(state));
    history.summary_label.wrap(SECTION_CONTENT_WIDTH);
    history.list.clear();

    let rows = history_row_entries(state);
    for (index, (label, _)) in rows.iter().enumerate() {
        history.list.append(label);
        if matches!(label.as_str(), "Current scene") {
            history.list.set_selection(index as u32, true);
            history.list.ensure_visible(index as i32);
        }
    }
    history.suppress_selection_events.set(false);
}

fn tool_supports_color(tool: Tool) -> bool {
    !matches!(tool, Tool::Select)
}

fn tool_supports_fill_toggle(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::Rectangle | Tool::Circle | Tool::Triangle | Tool::Polygon
    )
}

fn canvas_pane_info() -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("canvas")
        .center_pane()
        .caption_visible(false)
        .pane_border(false)
        .dockable(false)
}

fn session_pane_info(visible: bool) -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("session-panel")
        .with_caption("Session")
        .left()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .maximize_button(false)
        .pin_button(false)
        .best_size(SESSION_PANEL_WIDTH, 320)
        .min_size(260, 220)
        .row(1)
        .position(0)
        .show(visible)
}

fn objects_pane_info(visible: bool) -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("objects-panel")
        .with_caption("Objects")
        .right()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .maximize_button(false)
        .pin_button(false)
        .best_size(OBJECTS_PANEL_WIDTH, 340)
        .min_size(240, 220)
        .row(1)
        .position(0)
        .show(visible)
}

fn inspector_pane_info(visible: bool) -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("inspector-panel")
        .with_caption("Inspector")
        .right()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .maximize_button(false)
        .pin_button(false)
        .best_size(INSPECTOR_PANEL_WIDTH, 360)
        .min_size(260, 220)
        .row(1)
        .position(1)
        .show(visible)
}

fn history_pane_info(visible: bool) -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("history-panel")
        .with_caption("History")
        .bottom()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .maximize_button(false)
        .pin_button(false)
        .best_size(640, HISTORY_PANEL_HEIGHT)
        .min_size(320, 160)
        .row(1)
        .position(0)
        .show(visible)
}

fn apply_layout(
    manager: &AuiManager,
    canvas: &Panel,
    view_controls: &ViewControls,
    layout: &LayoutState,
) {
    let _ = manager.detach_pane(canvas);
    let _ = manager.detach_pane(&view_controls.workspace_toolbar);
    let _ = manager.detach_pane(&view_controls.tools_toolbar);
    let _ = manager.detach_pane(&view_controls.objects_toolbar);
    let _ = manager.detach_pane(&view_controls.session_panel);
    let _ = manager.detach_pane(&view_controls.objects_panel);
    let _ = manager.detach_pane(&view_controls.inspector_panel);
    let _ = manager.detach_pane(&view_controls.history_panel);

    let _ = manager.add_pane_with_info(canvas, canvas_pane_info());
    let _ = manager.add_pane_with_info(
        &view_controls.workspace_toolbar,
        toolbar_pane_info(ToolbarPane::Workspace, layout.workspace_toolbar_dock)
            .show(layout.workspace_toolbar_visible),
    );
    let _ = manager.add_pane_with_info(
        &view_controls.tools_toolbar,
        toolbar_pane_info(ToolbarPane::Tools, layout.tools_toolbar_dock)
            .show(layout.tools_toolbar_visible),
    );
    let _ = manager.add_pane_with_info(
        &view_controls.objects_toolbar,
        toolbar_pane_info(ToolbarPane::Objects, layout.objects_toolbar_dock)
            .show(layout.objects_toolbar_visible),
    );
    if layout.session_panel_visible {
        view_controls.session_panel.show(true);
        let _ = manager.add_pane_with_info(&view_controls.session_panel, session_pane_info(true));
    } else {
        view_controls.session_panel.hide();
    }
    let _ = manager.add_pane_with_info(
        &view_controls.objects_panel,
        objects_pane_info(layout.objects_panel_visible),
    );
    let _ = manager.add_pane_with_info(
        &view_controls.inspector_panel,
        inspector_pane_info(layout.inspector_panel_visible),
    );
    let _ = manager.add_pane_with_info(
        &view_controls.history_panel,
        history_pane_info(layout.history_panel_visible),
    );
    let _ = manager.update();
}

fn update_layout_state<F>(
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: AuiManager,
    canvas: &Panel,
    view_controls: ViewControls,
    update: F,
) where
    F: FnOnce(&mut LayoutState) -> bool,
{
    let canvas = *canvas;
    let changed = {
        let mut layout = layout_state.borrow_mut();
        update(&mut layout)
    };

    if changed {
        apply_layout(&manager, &canvas, &view_controls, &layout_state.borrow());
        sync_view_controls(&view_controls, &layout_state.borrow());
    }
}

fn bind_layout_action_bitmap_button(
    button: &BitmapButton,
    action: LayoutAction,
    manager: AuiManager,
    canvas: &Panel,
    layout_state: &Rc<RefCell<LayoutState>>,
    view_controls: ViewControls,
) {
    let button = *button;
    let canvas = *canvas;
    let layout_state = Rc::clone(layout_state);
    button.on_click(move |_| {
        update_layout_state(&layout_state, manager, &canvas, view_controls, |layout| {
            apply_layout_action(layout, action)
        });
    });
}

fn layout_action_from_menu_id(menu_id: i32) -> Option<LayoutAction> {
    match menu_id {
        MENU_ID_PIN_TOOLBARS => Some(LayoutAction::ToggleToolbarCluster),
        MENU_ID_SHOW_WORKSPACE_TOOLBAR => Some(LayoutAction::ToggleWorkspaceToolbar),
        MENU_ID_SHOW_TOOLS_TOOLBAR => Some(LayoutAction::ToggleToolsToolbar),
        MENU_ID_SHOW_OBJECTS_TOOLBAR => Some(LayoutAction::ToggleObjectsToolbar),
        MENU_ID_SHOW_SESSION_PANE => Some(LayoutAction::ToggleSessionPanel),
        MENU_ID_SHOW_OBJECTS_PANE => Some(LayoutAction::ToggleObjectsPanel),
        MENU_ID_SHOW_INSPECTOR_PANE => Some(LayoutAction::ToggleInspectorPanel),
        MENU_ID_SHOW_HISTORY_PANE => Some(LayoutAction::ToggleHistoryPanel),
        MENU_ID_WORKSPACE_TOOLBAR_TOP => {
            Some(LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Top))
        }
        MENU_ID_WORKSPACE_TOOLBAR_LEFT => {
            Some(LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Left))
        }
        MENU_ID_WORKSPACE_TOOLBAR_RIGHT => {
            Some(LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Right))
        }
        MENU_ID_WORKSPACE_TOOLBAR_BOTTOM => {
            Some(LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Bottom))
        }
        MENU_ID_TOOLS_TOOLBAR_TOP => Some(LayoutAction::SetToolsToolbarDock(ToolbarDock::Top)),
        MENU_ID_TOOLS_TOOLBAR_LEFT => Some(LayoutAction::SetToolsToolbarDock(ToolbarDock::Left)),
        MENU_ID_TOOLS_TOOLBAR_RIGHT => Some(LayoutAction::SetToolsToolbarDock(ToolbarDock::Right)),
        MENU_ID_TOOLS_TOOLBAR_BOTTOM => {
            Some(LayoutAction::SetToolsToolbarDock(ToolbarDock::Bottom))
        }
        MENU_ID_OBJECTS_TOOLBAR_TOP => Some(LayoutAction::SetObjectsToolbarDock(ToolbarDock::Top)),
        MENU_ID_OBJECTS_TOOLBAR_LEFT => {
            Some(LayoutAction::SetObjectsToolbarDock(ToolbarDock::Left))
        }
        MENU_ID_OBJECTS_TOOLBAR_RIGHT => {
            Some(LayoutAction::SetObjectsToolbarDock(ToolbarDock::Right))
        }
        MENU_ID_OBJECTS_TOOLBAR_BOTTOM => {
            Some(LayoutAction::SetObjectsToolbarDock(ToolbarDock::Bottom))
        }
        MENU_ID_RESET_LAYOUT => Some(LayoutAction::ResetLayout),
        _ => None,
    }
}

fn handle_layout_menu_action(
    menu_id: i32,
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: &AuiManager,
    canvas: &Panel,
    view_controls: ViewControls,
) -> bool {
    let Some(action) = layout_action_from_menu_id(menu_id) else {
        return false;
    };
    update_layout_state(layout_state, *manager, canvas, view_controls, |layout| {
        apply_layout_action(layout, action)
    });
    true
}

fn sync_view_controls(view_controls: &ViewControls, layout: &LayoutState) {
    view_controls
        .show_workspace_toolbar_item
        .check(layout.workspace_toolbar_visible);
    view_controls
        .show_tools_toolbar_item
        .check(layout.tools_toolbar_visible);
    view_controls
        .show_objects_toolbar_item
        .check(layout.objects_toolbar_visible);
    view_controls
        .show_session_pane_item
        .check(layout.session_panel_visible);
    view_controls
        .show_objects_pane_item
        .check(layout.objects_panel_visible);
    view_controls
        .show_inspector_pane_item
        .check(layout.inspector_panel_visible);
    view_controls
        .show_history_pane_item
        .check(layout.history_panel_visible);

    let all_toolbars_visible = layout.workspace_toolbar_visible
        && layout.tools_toolbar_visible
        && layout.objects_toolbar_visible;
    view_controls.pin_toolbars_item.check(all_toolbars_visible);
    view_controls
        .pin_toolbar_button
        .set_value(all_toolbars_visible);
    view_controls
        .pin_toolbar_button
        .set_bitmap_label(&make_pin_bitmap(all_toolbars_visible));
    sync_layout_helper_button(
        &view_controls.session_panel_button,
        layout.session_panel_visible,
    );
    sync_layout_helper_button(
        &view_controls.objects_panel_button,
        layout.objects_panel_visible,
    );
    sync_layout_helper_button(
        &view_controls.inspector_panel_button,
        layout.inspector_panel_visible,
    );
    sync_layout_helper_button(
        &view_controls.history_panel_button,
        layout.history_panel_visible,
    );
    sync_layout_helper_button(
        &view_controls.tools_dock_top_button,
        layout.tools_toolbar_dock == ToolbarDock::Top,
    );
    sync_layout_helper_button(
        &view_controls.tools_dock_left_button,
        layout.tools_toolbar_dock == ToolbarDock::Left,
    );
    sync_layout_helper_button(
        &view_controls.tools_dock_right_button,
        layout.tools_toolbar_dock == ToolbarDock::Right,
    );
    sync_layout_helper_button(
        &view_controls.tools_dock_bottom_button,
        layout.tools_toolbar_dock == ToolbarDock::Bottom,
    );
    sync_layout_helper_button(
        &view_controls.objects_pane_toggle_button,
        layout.objects_panel_visible,
    );
    sync_layout_helper_button(
        &view_controls.objects_dock_top_button,
        layout.objects_toolbar_dock == ToolbarDock::Top,
    );
    sync_layout_helper_button(
        &view_controls.objects_dock_left_button,
        layout.objects_toolbar_dock == ToolbarDock::Left,
    );
    sync_layout_helper_button(
        &view_controls.objects_dock_right_button,
        layout.objects_toolbar_dock == ToolbarDock::Right,
    );
    sync_layout_helper_button(
        &view_controls.objects_dock_bottom_button,
        layout.objects_toolbar_dock == ToolbarDock::Bottom,
    );
    sync_layout_helper_button(&view_controls.reset_layout_button, false);
}

fn sync_layout_helper_button(button: &BitmapButton, active: bool) {
    button.set_background_color(if active {
        active_tool_button_color()
    } else {
        inactive_tool_button_color()
    });
    button.refresh(false, None);
    button.update();
}

fn session_server_status(manifest_path: Option<&PathBuf>) -> ServerStatus {
    let Some(manifest_path) = manifest_path else {
        return ServerStatus::None;
    };
    if !manifest_path.exists() {
        return ServerStatus::None;
    }

    let manifest = match load_session_manifest(manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => return ServerStatus::Error(error.to_string()),
    };

    let Some(server) = manifest.latest_server() else {
        return ServerStatus::None;
    };
    server_status_from_entry(server)
}

fn server_status_from_entry(server: &AttachedServer) -> ServerStatus {
    if server.runtime_api_version != RUNTIME_API_VERSION {
        return ServerStatus::Incompatible {
            app_version: server.app_version.clone(),
            runtime_api_version: server.runtime_api_version,
        };
    }

    let age_ms = unix_time_ms().saturating_sub(server.last_seen_ms);
    if age_ms > SERVER_HEARTBEAT_STALE_MS {
        return ServerStatus::Stale {
            app_version: server.app_version.clone(),
            runtime_api_version: server.runtime_api_version,
        };
    }

    ServerStatus::Connected {
        app_version: server.app_version.clone(),
        runtime_api_version: server.runtime_api_version,
    }
}

fn ensure_gui_session_manifest(
    session_root: &PathBuf,
    session_id: &str,
    scene_path: &PathBuf,
) -> anyhow::Result<SessionManifest> {
    let manifest_path = session_manifest_path(session_root, session_id);
    let mut manifest = if manifest_path.exists() {
        load_session_manifest(&manifest_path)?
    } else {
        SessionManifest::new(session_id.to_string(), scene_path.clone())
    };
    manifest.scene_path = scene_path.display().to_string();
    manifest.touch();
    save_session_manifest(&manifest_path, &manifest)?;
    Ok(manifest)
}

fn last_window_state(manifest: &SessionManifest) -> Option<WindowState> {
    manifest
        .attached_windows
        .iter()
        .max_by_key(|window| window.last_seen_ms)
        .and_then(|window| window.window_state.clone())
}

fn current_window_state(frame: &Frame) -> WindowState {
    let position = frame.get_position();
    let size = frame.get_size();
    WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        maximized: frame.is_maximized(),
        iconized: frame.is_iconized(),
        desktop_id: None,
    }
}

fn new_window_id() -> String {
    new_prefixed_id("win")
}

fn sync_session_window_manifest(state: &mut BoardmeshState, frame: &Frame) -> anyhow::Result<()> {
    let (Some(manifest_path), Some(window_id)) = (
        state.session_manifest_path.as_ref(),
        state.window_id.as_ref(),
    ) else {
        return Ok(());
    };

    let window_state = current_window_state(frame);
    if state.last_window_state.as_ref() == Some(&window_state) {
        return Ok(());
    }

    let mut manifest = if manifest_path.exists() {
        load_session_manifest(manifest_path)?
    } else {
        SessionManifest::new(
            state
                .session_id
                .clone()
                .unwrap_or_else(|| "default".to_string()),
            state.scene_path.clone(),
        )
    };
    manifest.scene_path = state.scene_path.display().to_string();
    manifest.update_window(AttachedWindow {
        window_id: window_id.clone(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        protocol_version: SESSION_PROTOCOL_VERSION,
        pid: Some(std::process::id()),
        last_seen_ms: unix_time_ms(),
        window_state: Some(window_state.clone()),
    });
    save_session_manifest(manifest_path, &manifest)?;
    state.last_window_state = Some(window_state);
    log_gui(&format!(
        "synced window manifest session={:?} manifest={} window_id={}",
        state.session_id,
        manifest_path.display(),
        window_id
    ));
    Ok(())
}

fn remove_session_window_manifest(state: &BoardmeshState) -> anyhow::Result<()> {
    let (Some(manifest_path), Some(window_id)) = (
        state.session_manifest_path.as_ref(),
        state.window_id.as_ref(),
    ) else {
        return Ok(());
    };

    if !manifest_path.exists() {
        return Ok(());
    }

    let mut manifest = load_session_manifest(manifest_path)?;
    manifest.remove_window(window_id);
    save_session_manifest(manifest_path, &manifest)?;
    log_gui(&format!(
        "removed window manifest entry session={:?} manifest={} window_id={}",
        state.session_id,
        manifest_path.display(),
        window_id
    ));
    Ok(())
}

fn sync_object_list(object_list: &CheckListBox, state: &BoardmeshState) {
    object_list.clear();
    let mut focused_index = None;

    for (index, object) in state.scene.objects.iter().enumerate() {
        object_list.append(&object_label(index, object));
        if state
            .scene
            .selected_object_ids
            .iter()
            .any(|selected_id| selected_id == &object.id)
        {
            object_list.check(index as u32, true);
        }
        if state.scene.selected_object_ids.last() == Some(&object.id) {
            focused_index = Some(index as u32);
        }
    }

    if let Some(index) = focused_index {
        object_list.set_selection(index, true);
        object_list.ensure_visible(index as i32);
    }
}

fn checked_object_ids(object_list: &CheckListBox, scene: &SceneDocument) -> Vec<String> {
    scene
        .objects
        .iter()
        .enumerate()
        .filter_map(|(index, object)| {
            object_list
                .is_checked(index as u32)
                .then_some(object.id.clone())
        })
        .collect()
}

fn object_label(index: usize, object: &DrawableObject) -> String {
    let kind = match &object.kind {
        ObjectKind::Line { .. } => "Line",
        ObjectKind::Connector { .. } => "Connector",
        ObjectKind::Link { .. } => "Link",
        ObjectKind::Rectangle { .. } => "Rectangle",
        ObjectKind::Circle { .. } => "Circle",
        ObjectKind::Triangle { .. } => "Triangle",
        ObjectKind::Polygon { .. } => "Polygon",
        ObjectKind::Stroke { .. } => "Stroke",
        ObjectKind::Text { text } => {
            return format!("{}. Text: {}", index + 1, truncate_label(&text.content));
        }
    };
    format!("{}. {}", index + 1, kind)
}

fn truncate_label(text: &str) -> String {
    const LIMIT: usize = 18;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    let shortened: String = text.chars().take(LIMIT).collect();
    format!("{shortened}...")
}

fn circle_center(start: CanvasPoint, end: CanvasPoint) -> CanvasPoint {
    CanvasPoint::new((start.x + end.x) / 2, (start.y + end.y) / 2)
}

fn circle_radius(start: CanvasPoint, end: CanvasPoint) -> i32 {
    let dx = (end.x - start.x).abs();
    let dy = (end.y - start.y).abs();
    (dx.min(dy) / 2).max(1)
}

fn font_awesome_solid_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/icons/fontawesome-free/solid")
}

fn load_fontawesome_icon(icon_name: &str, size: i32) -> Option<Bitmap> {
    let path = font_awesome_solid_dir().join(format!("{icon_name}.svg"));
    if !path.exists() {
        return None;
    }
    BitmapBundle::from_svg_file(&path, Size::new(size, size))
        .and_then(|bundle| bundle.get_bitmap(Size::new(size, size)))
}

fn make_toolbar_action_bitmap(icon_name: &str, fallback: char) -> Bitmap {
    load_fontawesome_icon(icon_name, TOOLBAR_ICON_SIZE)
        .unwrap_or_else(|| make_fallback_action_bitmap(fallback))
}

fn make_toolbar_section_label(parent: &Panel, label: &str, width: i32) -> StaticText {
    let section_label = StaticText::builder(parent)
        .with_label(label)
        .with_style(StaticTextStyle::AlignCenterHorizontal)
        .with_size(Size::new(width, TOOLBAR_SECTION_LABEL_HEIGHT))
        .build();
    section_label.set_foreground_color(toolbar_section_label_color());
    if let Some(font) = FontBuilder::default()
        .with_point_size(8)
        .with_family(FontFamily::Swiss)
        .with_weight(FontWeight::Bold)
        .build()
    {
        section_label.set_font(&font);
    }
    section_label
}

fn make_toolbar_divider(parent: &Panel, orientation: Orientation, span: i32) -> Panel {
    let size = match orientation {
        Orientation::Horizontal => Size::new(span, TOOLBAR_DIVIDER_THICKNESS),
        Orientation::Vertical => Size::new(TOOLBAR_DIVIDER_THICKNESS, span),
        _ => Size::new(TOOLBAR_DIVIDER_THICKNESS, span),
    };
    let divider = Panel::builder(parent).with_size(size).build();
    divider.set_min_size(size);
    divider.set_background_color(toolbar_divider_color());
    divider
}

fn make_tool_bitmap(tool: Tool) -> Bitmap {
    let fontawesome_icon = match tool {
        Tool::Select => Some("mouse-pointer"),
        Tool::Pen => Some("pencil-alt"),
        Tool::Line => Some("minus"),
        Tool::Rectangle => Some("square"),
        Tool::Circle => Some("circle"),
        Tool::Triangle => Some("play"),
        Tool::Polygon => Some("draw-polygon"),
        Tool::Fill => Some("fill-drip"),
    };
    if let Some(bitmap) =
        fontawesome_icon.and_then(|icon_name| load_fontawesome_icon(icon_name, TOOLBAR_ICON_SIZE))
    {
        return bitmap;
    }

    let pattern = match tool {
        Tool::Select => &[
            "#...........",
            "##..........",
            "#.#.........",
            "#..#........",
            "#...#.......",
            "#....#......",
            "#.....#.....",
            "#..####.....",
            "#..#........",
            "...#........",
            "...#........",
            "............",
        ][..],
        Tool::Pen => &[
            "...........#",
            "..........##",
            ".........##.",
            "........##..",
            ".......##...",
            "......##....",
            ".....##.....",
            "....##......",
            "...##.......",
            "..##........",
            ".##.........",
            "##..........",
        ][..],
        Tool::Line => &[
            "............",
            "##..........",
            "..##........",
            "....##......",
            "......##....",
            "........##..",
            "..........##",
            "............",
            "............",
            "............",
            "............",
            "............",
        ][..],
        Tool::Rectangle => &[
            "............",
            ".##########.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".##########.",
            "............",
            "............",
        ][..],
        Tool::Circle => &[
            "............",
            "...######...",
            "..##....##..",
            ".##......##.",
            ".##......##.",
            ".##......##.",
            ".##......##.",
            ".##......##.",
            "..##....##..",
            "...######...",
            "............",
            "............",
        ][..],
        Tool::Triangle => &[
            "......#.....",
            ".....###....",
            ".....#.#....",
            "....##.##...",
            "...##...##..",
            "...#.....#..",
            "..##.....##.",
            ".##########.",
            "............",
            "............",
            "............",
            "............",
        ][..],
        Tool::Polygon => &[
            "............",
            "....####....",
            "..##....##..",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            "..#......#..",
            "...##..##...",
            ".....##.....",
            "............",
            "............",
            "............",
        ][..],
        Tool::Fill => &[
            "......##....",
            ".....####...",
            "....##..##..",
            "...##....##.",
            "..##......##",
            "..##########",
            "..##########",
            "...++++++...",
            "....++++....",
            ".....++.....",
            "............",
            "............",
        ][..],
    };
    bitmap_from_pattern(
        pattern,
        SceneColor::rgb(30, 41, 59),
        SceneColor::rgb(14, 165, 233),
        2,
    )
}

fn make_pin_bitmap(active: bool) -> Bitmap {
    if let Some(bitmap) = load_fontawesome_icon("thumbtack", TOOLBAR_ICON_SIZE) {
        return bitmap;
    }

    let accent = if active {
        SceneColor::rgb(14, 165, 233)
    } else {
        SceneColor::rgb(148, 163, 184)
    };
    bitmap_from_pattern(
        &[
            "....++++....",
            "...++++++...",
            "..++####++..",
            "...++##++...",
            ".....##.....",
            ".....##.....",
            ".....##.....",
            "....####....",
            "...##..##...",
            "............",
            "............",
            "............",
        ],
        SceneColor::rgb(30, 41, 59),
        accent,
        2,
    )
}

fn make_select_all_bitmap() -> Bitmap {
    if let Some(bitmap) = load_fontawesome_icon("check-square", TOOLBAR_ICON_SIZE) {
        return bitmap;
    }

    bitmap_from_pattern(
        &[
            "............",
            ".##########.",
            ".#........#.",
            ".#..++++..#.",
            ".#..++++..#.",
            ".#........#.",
            ".#..++++..#.",
            ".#..++++..#.",
            ".#........#.",
            ".##########.",
            "............",
            "............",
        ],
        SceneColor::rgb(30, 41, 59),
        SceneColor::rgb(16, 185, 129),
        2,
    )
}

fn make_clear_selection_bitmap() -> Bitmap {
    if let Some(bitmap) = load_fontawesome_icon("times", TOOLBAR_ICON_SIZE) {
        return bitmap;
    }

    bitmap_from_pattern(
        &[
            "............",
            ".##......##.",
            "..##....##..",
            "...##..##...",
            "....####....",
            ".....##.....",
            "....####....",
            "...##..##...",
            "..##....##..",
            ".##......##.",
            "............",
            "............",
        ],
        SceneColor::rgb(30, 41, 59),
        SceneColor::rgb(239, 68, 68),
        2,
    )
}

fn make_layout_panel_bitmap(panel: char) -> Bitmap {
    let fontawesome_icon = match panel {
        'I' => Some("sliders-h"),
        'L' => Some("border-all"),
        'O' => Some("list-ul"),
        'S' => Some("window-maximize"),
        _ => Some("window-maximize"),
    };
    if let Some(bitmap) =
        fontawesome_icon.and_then(|icon_name| load_fontawesome_icon(icon_name, TOOLBAR_ICON_SIZE))
    {
        return bitmap;
    }

    let accent = match panel {
        'I' => SceneColor::rgb(245, 158, 11),
        'O' => SceneColor::rgb(16, 185, 129),
        'L' => SceneColor::rgb(99, 102, 241),
        'S' => SceneColor::rgb(14, 165, 233),
        _ => SceneColor::rgb(14, 165, 233),
    };
    let pattern = match panel {
        'S' => &[
            "............",
            ".##########.",
            ".#++++++++#.",
            ".#+......+#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#++++++++#.",
            ".##########.",
            "............",
            "............",
        ][..],
        'I' => &[
            "............",
            ".##########.",
            ".#..##....#.",
            ".#..##....#.",
            ".#......###.",
            ".#......###.",
            ".###......#.",
            ".###......#.",
            ".#....##..#.",
            ".##########.",
            "............",
            "............",
        ][..],
        'L' => &[
            "............",
            ".##########.",
            ".#++++++++#.",
            ".#++....++#.",
            ".#..++++..#.",
            ".#..++++..#.",
            ".#++....++#.",
            ".#++++++++#.",
            ".#........#.",
            ".##########.",
            "............",
            "............",
        ][..],
        'O' => &[
            "............",
            ".##########.",
            ".#........#.",
            ".#++..++..#.",
            ".#........#.",
            ".#++..++..#.",
            ".#........#.",
            ".#++..++..#.",
            ".#........#.",
            ".##########.",
            "............",
            "............",
        ][..],
        _ => &[
            "............",
            ".##########.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".#........#.",
            ".##########.",
            "............",
            "............",
        ][..],
    };
    bitmap_from_pattern(pattern, SceneColor::rgb(30, 41, 59), accent, 2)
}

fn make_direction_bitmap(direction: ToolbarDock) -> Bitmap {
    let fontawesome_icon = match direction {
        ToolbarDock::Top => Some("arrow-up"),
        ToolbarDock::Left => Some("arrow-left"),
        ToolbarDock::Right => Some("arrow-right"),
        ToolbarDock::Bottom => Some("arrow-down"),
    };
    if let Some(bitmap) =
        fontawesome_icon.and_then(|icon_name| load_fontawesome_icon(icon_name, TOOLBAR_ICON_SIZE))
    {
        return bitmap;
    }

    let pattern = match direction {
        ToolbarDock::Top => &[
            ".....##.....",
            "....####....",
            "...##..##...",
            "..##....##..",
            ".....##.....",
            ".....##.....",
            ".....##.....",
            ".....##.....",
            "............",
            "............",
            "............",
            "............",
        ][..],
        ToolbarDock::Left => &[
            "............",
            "....##......",
            "..####......",
            "######......",
            "##########..",
            "######......",
            "..####......",
            "....##......",
            "............",
            "............",
            "............",
            "............",
        ][..],
        ToolbarDock::Right => &[
            "............",
            "......##....",
            "......####..",
            "......######",
            "..##########",
            "......######",
            "......####..",
            "......##....",
            "............",
            "............",
            "............",
            "............",
        ][..],
        ToolbarDock::Bottom => &[
            "............",
            "............",
            "............",
            "............",
            ".....##.....",
            ".....##.....",
            ".....##.....",
            "..##....##..",
            "...##..##...",
            "....####....",
            ".....##.....",
            "............",
        ][..],
    };
    bitmap_from_pattern(
        pattern,
        SceneColor::rgb(30, 41, 59),
        SceneColor::rgb(14, 165, 233),
        2,
    )
}

fn make_fallback_action_bitmap(letter: char) -> Bitmap {
    let fontawesome_icon = match letter {
        'U' => Some("undo-alt"),
        'R' => Some("redo-alt"),
        'C' | 'D' => Some("trash-alt"),
        '?' => Some("question-circle"),
        _ => None,
    };
    if let Some(bitmap) =
        fontawesome_icon.and_then(|icon_name| load_fontawesome_icon(icon_name, TOOLBAR_ICON_SIZE))
    {
        return bitmap;
    }

    let mut rows = ["............"; 12];
    rows[1] = ".##########.";
    rows[2] = ".#++++++++#.";
    rows[3] = ".#++++++++#.";
    rows[4] = ".#++++++++#.";
    rows[5] = ".#++++++++#.";
    rows[6] = ".#++++++++#.";
    rows[7] = ".#++++++++#.";
    rows[8] = ".#++++++++#.";
    rows[9] = ".##########.";
    let accent = match letter {
        'D' | 'C' => SceneColor::rgb(239, 68, 68),
        'R' => SceneColor::rgb(16, 185, 129),
        _ => SceneColor::rgb(14, 165, 233),
    };
    bitmap_from_pattern(&rows, SceneColor::rgb(30, 41, 59), accent, 2)
}

fn bitmap_from_pattern(
    pattern: &[&str],
    foreground: SceneColor,
    accent: SceneColor,
    scale: usize,
) -> Bitmap {
    let width = pattern.first().map(|row| row.len()).unwrap_or(0);
    let height = pattern.len();
    let scaled_width = width * scale;
    let scaled_height = height * scale;
    let mut rgba = vec![0u8; scaled_width * scaled_height * 4];

    for (row_index, row) in pattern.iter().enumerate() {
        for (column_index, cell) in row.chars().enumerate() {
            let color = match cell {
                '#' => Some(foreground),
                '+' => Some(accent),
                _ => None,
            };
            let Some(color) = color else {
                continue;
            };
            for dx in 0..scale {
                for dy in 0..scale {
                    let x = column_index * scale + dx;
                    let y = row_index * scale + dy;
                    let offset = (y * scaled_width + x) * 4;
                    rgba[offset] = color.r;
                    rgba[offset + 1] = color.g;
                    rgba[offset + 2] = color.b;
                    rgba[offset + 3] = color.a;
                }
            }
        }
    }

    Bitmap::from_rgba(&rgba, scaled_width as u32, scaled_height as u32)
        .expect("pattern bitmap should be valid")
}

fn draw_canvas_grid(dc: &impl DeviceContext, viewport: &ViewportState, canvas_size: Size) {
    let major = SceneColor::rgb(203, 213, 225);
    let minor = SceneColor::rgb(226, 232, 240);
    let step = visible_grid_step(viewport.zoom);
    let world_top_left = viewport.screen_to_world(GraphPoint::new(0.0, 0.0));
    let world_bottom_right = viewport.screen_to_world(GraphPoint::new(
        canvas_size.width as f64,
        canvas_size.height as f64,
    ));
    let start_x = (world_top_left.x / step).floor() as i32 - 1;
    let end_x = (world_bottom_right.x / step).ceil() as i32 + 1;
    let start_y = (world_top_left.y / step).floor() as i32 - 1;
    let end_y = (world_bottom_right.y / step).ceil() as i32 + 1;

    for index in start_x..=end_x {
        let world_x = index as f64 * step;
        let screen_x = viewport
            .world_to_screen(GraphPoint::new(world_x, 0.0))
            .x
            .round() as i32;
        let color = if index % 5 == 0 { major } else { minor };
        dc.set_pen(as_wx_color(color), 1, PenStyle::Solid);
        dc.draw_line(screen_x, 0, screen_x, canvas_size.height);
    }

    for index in start_y..=end_y {
        let world_y = index as f64 * step;
        let screen_y = viewport
            .world_to_screen(GraphPoint::new(0.0, world_y))
            .y
            .round() as i32;
        let color = if index % 5 == 0 { major } else { minor };
        dc.set_pen(as_wx_color(color), 1, PenStyle::Solid);
        dc.draw_line(0, screen_y, canvas_size.width, screen_y);
    }
}

fn draw_object(
    dc: &impl DeviceContext,
    scene: &SceneDocument,
    object: &DrawableObject,
    viewport: &ViewportState,
) {
    dc.set_pen(
        as_wx_color(object.stroke.color),
        screen_stroke_width(object.stroke.width, viewport.zoom),
        PenStyle::Solid,
    );
    dc.set_brush(
        as_wx_color(object.fill.unwrap_or(SceneColor::transparent())),
        if object.fill.is_some() {
            BrushStyle::Solid
        } else {
            BrushStyle::Transparent
        },
    );

    match &object.kind {
        ObjectKind::Line { start, end } => {
            let start = viewport.world_to_screen_canvas_point(*start);
            let end = viewport.world_to_screen_canvas_point(*end);
            dc.draw_line(start.x, start.y, end.x, end.y)
        }
        ObjectKind::Connector { .. } | ObjectKind::Link { .. } => {
            if let Some(path) = scene.resolved_connector_path(&object.id) {
                draw_connector_path(dc, path, viewport);
            }
        }
        ObjectKind::Rectangle { rect } => draw_screen_rect(dc, *rect, viewport),
        ObjectKind::Circle { center, radius } => {
            let center = viewport.world_to_screen_canvas_point(*center);
            let radius = ((*radius as f64) * viewport.zoom).round() as i32;
            dc.draw_circle(center.x, center.y, radius.max(1));
        }
        ObjectKind::Triangle { triangle } => draw_triangle(dc, triangle.points, viewport),
        ObjectKind::Polygon { points } => draw_polygon_shape(dc, points, viewport),
        ObjectKind::Stroke { points } => draw_stroke(dc, points, viewport),
        ObjectKind::Text { text } => {
            dc.set_text_foreground(as_wx_color(object.stroke.color));
            let position = viewport.world_to_screen_canvas_point(text.position);
            dc.draw_text(&text.content, position.x, position.y);
        }
    }
}

fn draw_draft(
    dc: &impl DeviceContext,
    draft: &DraftShape,
    color: SceneColor,
    _fill_shapes: bool,
    viewport: &ViewportState,
) {
    dc.set_pen(as_wx_color(color), 2, PenStyle::Dot);
    dc.set_brush(as_wx_color(color), BrushStyle::Transparent);

    match draft {
        DraftShape::SelectionBox { start, current, .. } => {
            let rect = screen_rect(SceneRect::from_points(*start, *current), viewport);
            dc.set_pen(
                as_wx_color(SceneColor::rgb(37, 99, 235)),
                2,
                PenStyle::ShortDash,
            );
            dc.set_brush(
                as_wx_color(SceneColor::transparent()),
                BrushStyle::Transparent,
            );
            dc.draw_rectangle(rect.x, rect.y, rect.width.max(1), rect.height.max(1));
        }
        DraftShape::Line { start, current } => {
            let start = viewport.world_to_screen_canvas_point(*start);
            let current = viewport.world_to_screen_canvas_point(*current);
            dc.draw_line(start.x, start.y, current.x, current.y)
        }
        DraftShape::Rectangle { start, current } => {
            draw_screen_rect(dc, SceneRect::from_points(*start, *current), viewport);
        }
        DraftShape::Circle { start, current } => {
            let radius = circle_radius(*start, *current);
            let center = viewport.world_to_screen_canvas_point(circle_center(*start, *current));
            let screen_radius = ((radius as f64) * viewport.zoom).round() as i32;
            dc.draw_circle(center.x, center.y, screen_radius.max(1));
        }
        DraftShape::Triangle { start, current } => {
            let triangle = TriangleObject::from_rect(SceneRect::from_points(*start, *current));
            draw_triangle(dc, triangle.points, viewport);
        }
        DraftShape::TrianglePoints { points, current } => {
            draw_polygon_draft(dc, points, *current, viewport)
        }
        DraftShape::Polygon { points, current } => {
            draw_polygon_draft(dc, points, *current, viewport)
        }
        DraftShape::Stroke { points } => draw_stroke(dc, points, viewport),
    }
}

fn draw_selection_highlight(
    dc: &impl DeviceContext,
    scene: &SceneDocument,
    object: &DrawableObject,
    viewport: &ViewportState,
) {
    let bounds = screen_rect(object_bounds(scene, object), viewport);
    dc.set_pen(
        as_wx_color(SceneColor::rgb(37, 99, 235)),
        2,
        PenStyle::ShortDash,
    );
    dc.set_brush(
        as_wx_color(SceneColor::transparent()),
        BrushStyle::Transparent,
    );
    dc.draw_rectangle(
        bounds.x - 4,
        bounds.y - 4,
        bounds.width.max(1) + 8,
        bounds.height.max(1) + 8,
    );
}

fn draw_connector_path(
    dc: &impl DeviceContext,
    path: ResolvedConnectorPath,
    viewport: &ViewportState,
) {
    match path {
        ResolvedConnectorPath::Polyline(points) => draw_stroke(dc, &points, viewport),
        ResolvedConnectorPath::CubicBezier(segments) => {
            for segment in segments {
                let points = approximate_bezier(segment);
                draw_stroke(dc, &points, viewport);
            }
        }
    }
}

fn object_bounds(scene: &SceneDocument, object: &DrawableObject) -> SceneRect {
    match &object.kind {
        ObjectKind::Connector { .. } | ObjectKind::Link { .. } => scene
            .resolved_connector_path(&object.id)
            .map(connector_path_bounds)
            .unwrap_or_else(|| SceneRect::new(0, 0, 0, 0)),
        _ => object.bounds(),
    }
}

fn scene_bounds(scene: &SceneDocument) -> Option<SceneRect> {
    let mut objects = scene.objects.iter();
    let first = objects.next()?;
    let mut bounds = object_bounds(scene, first);
    for object in objects {
        bounds = bounds.union(object_bounds(scene, object));
    }
    Some(bounds)
}

fn selected_bounds(scene: &SceneDocument) -> Option<SceneRect> {
    let mut objects = scene.selected_objects().into_iter();
    let first = objects.next()?;
    let mut bounds = object_bounds(scene, first);
    for object in objects {
        bounds = bounds.union(object_bounds(scene, object));
    }
    Some(bounds)
}

fn connector_path_bounds(path: ResolvedConnectorPath) -> SceneRect {
    match path {
        ResolvedConnectorPath::Polyline(points) => points_bounds(&points),
        ResolvedConnectorPath::CubicBezier(segments) => {
            let points = segments
                .into_iter()
                .flat_map(|segment| {
                    [
                        segment.start,
                        segment.control_start,
                        segment.control_end,
                        segment.end,
                    ]
                })
                .collect::<Vec<_>>();
            points_bounds(&points)
        }
    }
}

fn visible_grid_step(zoom: f64) -> f64 {
    let mut step = CANVAS_GRID_SPACING as f64;
    while step * zoom < 20.0 {
        step *= 2.0;
    }
    while step * zoom > 140.0 && step > 1.0 {
        step /= 2.0;
    }
    step
}

fn screen_stroke_width(width: i32, zoom: f64) -> i32 {
    ((width.max(1) as f64) * zoom).round().max(1.0) as i32
}

fn screen_rect(rect: SceneRect, viewport: &ViewportState) -> SceneRect {
    let top_left = viewport.world_to_screen_canvas_point(CanvasPoint::new(rect.x, rect.y));
    let bottom_right = viewport
        .world_to_screen_canvas_point(CanvasPoint::new(rect.x + rect.width, rect.y + rect.height));
    SceneRect::from_points(top_left, bottom_right)
}

fn draw_screen_rect(dc: &impl DeviceContext, rect: SceneRect, viewport: &ViewportState) {
    let rect = screen_rect(rect, viewport);
    dc.draw_rectangle(rect.x, rect.y, rect.width.max(1), rect.height.max(1));
}

fn draw_stroke(dc: &impl DeviceContext, points: &[CanvasPoint], viewport: &ViewportState) {
    match points {
        [] => {}
        [point] => {
            let point = viewport.world_to_screen_canvas_point(*point);
            dc.draw_point(point.x, point.y)
        }
        _ => {
            let points: Vec<wxdragon::dc::Point> = points
                .iter()
                .map(|point| viewport.world_to_screen_canvas_point(*point))
                .map(|point| wxdragon::dc::Point::new(point.x, point.y))
                .collect();
            dc.draw_lines(&points, 0, 0);
        }
    }
}

fn approximate_bezier(segment: boardmesh::ResolvedCubicBezierSegment) -> Vec<CanvasPoint> {
    const STEPS: usize = 24;
    (0..=STEPS)
        .map(|step| cubic_point(segment, step as f64 / STEPS as f64))
        .collect()
}

fn cubic_point(segment: boardmesh::ResolvedCubicBezierSegment, t: f64) -> CanvasPoint {
    let inv = 1.0 - t;
    let start_weight = inv * inv * inv;
    let control_start_weight = 3.0 * inv * inv * t;
    let control_end_weight = 3.0 * inv * t * t;
    let end_weight = t * t * t;

    CanvasPoint::new(
        (segment.start.x as f64 * start_weight
            + segment.control_start.x as f64 * control_start_weight
            + segment.control_end.x as f64 * control_end_weight
            + segment.end.x as f64 * end_weight)
            .round() as i32,
        (segment.start.y as f64 * start_weight
            + segment.control_start.y as f64 * control_start_weight
            + segment.control_end.y as f64 * control_end_weight
            + segment.end.y as f64 * end_weight)
            .round() as i32,
    )
}

fn points_bounds(points: &[CanvasPoint]) -> SceneRect {
    if points.is_empty() {
        return SceneRect::new(0, 0, 0, 0);
    }

    let mut left = i32::MAX;
    let mut top = i32::MAX;
    let mut right = i32::MIN;
    let mut bottom = i32::MIN;

    for point in points {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }

    SceneRect::new(left, top, right - left, bottom - top)
}

fn draw_triangle(dc: &impl DeviceContext, points: [CanvasPoint; 3], viewport: &ViewportState) {
    draw_polygon_points(dc, &points, viewport);
}

fn draw_polygon_shape(dc: &impl DeviceContext, points: &[CanvasPoint], viewport: &ViewportState) {
    draw_polygon_points(dc, points, viewport);
}

fn draw_polygon_points(dc: &impl DeviceContext, points: &[CanvasPoint], viewport: &ViewportState) {
    let points = points
        .iter()
        .map(|point| viewport.world_to_screen_canvas_point(*point))
        .map(|point| wxdragon::dc::Point::new(point.x, point.y))
        .collect::<Vec<_>>();
    dc.draw_polygon(&points, 0, 0, wxdragon::dc::PolygonFillMode::OddEven);
}

fn draw_polygon_draft(
    dc: &impl DeviceContext,
    points: &[CanvasPoint],
    current: CanvasPoint,
    viewport: &ViewportState,
) {
    match points {
        [] => {}
        [start] => {
            let start = viewport.world_to_screen_canvas_point(*start);
            let current = viewport.world_to_screen_canvas_point(current);
            dc.draw_line(start.x, start.y, current.x, current.y)
        }
        _ => {
            let mut preview_points = points.to_vec();
            if preview_points.last().copied() != Some(current) {
                preview_points.push(current);
            }
            let preview = preview_points
                .iter()
                .map(|point| viewport.world_to_screen_canvas_point(*point))
                .map(|point| wxdragon::dc::Point::new(point.x, point.y))
                .collect::<Vec<_>>();
            dc.draw_lines(&preview, 0, 0);
            let first = viewport.world_to_screen_canvas_point(preview_points[0]);
            let last = viewport.world_to_screen_canvas_point(*preview_points.last().unwrap());
            dc.draw_line(last.x, last.y, first.x, first.y);
        }
    }
}

fn as_wx_color(color: SceneColor) -> Colour {
    Colour::new(color.r, color.g, color.b, color.a)
}

fn from_wx_color(color: Colour) -> SceneColor {
    SceneColor {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

fn mouse_position(event: &WindowEventData) -> Option<Point> {
    match event {
        WindowEventData::MouseButton(mouse) => mouse.get_position(),
        WindowEventData::MouseMotion(mouse) => mouse.get_position(),
        WindowEventData::MouseEnter(mouse) => mouse.get_position(),
        WindowEventData::MouseLeave(mouse) => mouse.get_position(),
        _ => None,
    }
}

fn mouse_command_multi_select_active(event: &WindowEventData) -> bool {
    match event {
        WindowEventData::MouseButton(mouse) => mouse.event.control_down() || mouse.event.cmd_down(),
        WindowEventData::MouseMotion(mouse) => mouse.event.control_down() || mouse.event.cmd_down(),
        WindowEventData::MouseEnter(mouse) => mouse.event.control_down() || mouse.event.cmd_down(),
        WindowEventData::MouseLeave(mouse) => mouse.event.control_down() || mouse.event.cmd_down(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boardmesh::{TextObject, load_scene};

    fn sample_state(active_tool: Tool) -> BoardmeshState {
        let mut state = BoardmeshState::new(
            PathBuf::from("/tmp"),
            PathBuf::from("/tmp/scene.json"),
            None,
            SceneDocument::default(),
            None,
        );
        state.active_tool = active_tool;
        state
    }

    #[test]
    fn selection_summary_uses_human_readable_counts() {
        let mut state = sample_state(Tool::Select);
        assert_eq!(
            selection_summary_text(&state),
            "Selection: nothing selected"
        );

        state.scene.selected_object_ids = vec!["a".to_string()];
        assert_eq!(
            selection_summary_text(&state),
            "Selection: 1 object selected"
        );

        state.scene.selected_object_ids = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            selection_summary_text(&state),
            "Selection: 2 objects selected"
        );
    }

    #[test]
    fn fill_toggle_visibility_matches_shape_tools() {
        assert!(tool_supports_fill_toggle(Tool::Rectangle));
        assert!(tool_supports_fill_toggle(Tool::Circle));
        assert!(tool_supports_fill_toggle(Tool::Triangle));
        assert!(tool_supports_fill_toggle(Tool::Polygon));
        assert!(!tool_supports_fill_toggle(Tool::Select));
        assert!(!tool_supports_fill_toggle(Tool::Pen));
        assert!(!tool_supports_fill_toggle(Tool::Line));
        assert!(!tool_supports_fill_toggle(Tool::Fill));
    }

    #[test]
    fn tool_state_summary_reports_multi_point_drafts() {
        let mut polygon_state = sample_state(Tool::Polygon);
        polygon_state.draft = Some(DraftShape::Polygon {
            points: vec![CanvasPoint::new(0, 0), CanvasPoint::new(10, 10)],
            current: CanvasPoint::new(20, 20),
        });
        assert_eq!(
            tool_state_summary(&polygon_state),
            "State: polygon draft with 2 points"
        );

        let mut triangle_state = sample_state(Tool::Triangle);
        triangle_state.triangle_mode = TriangleMode::ThreePoint;
        triangle_state.draft = Some(DraftShape::TrianglePoints {
            points: vec![CanvasPoint::new(0, 0)],
            current: CanvasPoint::new(10, 10),
        });
        assert_eq!(
            tool_state_summary(&triangle_state),
            "State: 3-point triangle draft with 1 points"
        );
    }

    #[test]
    fn history_selection_maps_rows_to_actions() {
        let mut state = sample_state(Tool::Select);

        let mut scene_a = SceneDocument::default();
        scene_a.selected_object_ids.push("a".to_string());
        let mut scene_b = SceneDocument::default();
        scene_b.selected_object_ids.push("b".to_string());
        let mut scene_c = SceneDocument::default();
        scene_c.selected_object_ids.push("c".to_string());

        state.history.push("First", scene_a);
        state.history.push("Second", scene_b);
        state.history.push_redo("Third", scene_c);

        assert_eq!(
            history_action_for_selection(&state, 0),
            Some(HistoryRowAction::Undo(1))
        );
        assert_eq!(
            history_action_for_selection(&state, 1),
            Some(HistoryRowAction::Undo(2))
        );
        assert_eq!(history_action_for_selection(&state, 2), None);
        assert_eq!(
            history_action_for_selection(&state, 3),
            Some(HistoryRowAction::Redo(1))
        );
    }

    #[test]
    fn apply_history_action_replays_multiple_steps() {
        let mut state = sample_state(Tool::Select);

        let mut scene_a = SceneDocument::default();
        scene_a.selected_object_ids.push("a".to_string());
        let mut scene_b = SceneDocument::default();
        scene_b.selected_object_ids.push("b".to_string());
        let mut scene_c = SceneDocument::default();
        scene_c.selected_object_ids.push("c".to_string());

        state.history.push("A", scene_a.clone());
        state.history.push("B", scene_b.clone());
        state.scene = scene_c.clone();

        assert!(apply_history_action_to_state(
            &mut state,
            HistoryRowAction::Undo(2)
        ));
        assert_eq!(state.scene, scene_a);
        assert_eq!(state.history.undo_len(), 0);
        assert_eq!(state.history.redo_len(), 2);

        assert!(apply_history_action_to_state(
            &mut state,
            HistoryRowAction::Redo(2)
        ));
        assert_eq!(state.scene, scene_c);
        assert_eq!(state.history.undo_len(), 2);
        assert_eq!(state.history.redo_len(), 0);
    }

    #[test]
    fn self_echoed_runtime_updates_preserve_history() {
        let mut state = sample_state(Tool::Select);
        state.scene.add_text(
            TextObject {
                position: CanvasPoint::new(10, 20),
                content: "first".to_string(),
            },
            SceneColor::rgb(1, 2, 3),
        );
        state.push_undo_snapshot();
        let expected_undo_depth = state.history.undo_len();

        let snapshot = RuntimeSceneSnapshot {
            session_id: None,
            scene_path: state.scene_path.display().to_string(),
            manifest_path: None,
            scene: state.scene.clone(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            runtime_api_version: RUNTIME_API_VERSION,
        };

        let update_kind = apply_runtime_scene_update(&mut state, snapshot);

        assert_eq!(update_kind, RuntimeSceneUpdateKind::NoOp);
        assert_eq!(state.history.undo_len(), expected_undo_depth);
        assert_eq!(state.history.redo_len(), 0);
    }

    #[test]
    fn new_window_id_uses_prefixed_shape() {
        let window_id = new_window_id();
        assert!(window_id.starts_with("win-"));
        assert_eq!(window_id["win-".len()..].len(), 26);
        assert!(
            window_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        );
    }

    #[test]
    fn external_runtime_updates_replace_scene_and_clear_history() {
        let mut state = sample_state(Tool::Select);
        state.scene.add_text(
            TextObject {
                position: CanvasPoint::new(10, 20),
                content: "before".to_string(),
            },
            SceneColor::rgb(1, 2, 3),
        );
        state.push_undo_snapshot();
        state.history.push_redo("stale", SceneDocument::default());

        let mut replacement = SceneDocument::default();
        replacement.add_text(
            TextObject {
                position: CanvasPoint::new(30, 40),
                content: "after".to_string(),
            },
            SceneColor::rgb(9, 8, 7),
        );
        let snapshot = RuntimeSceneSnapshot {
            session_id: Some("default".to_string()),
            scene_path: "/tmp/other-scene.json".to_string(),
            manifest_path: Some("/tmp/default.manifest.json".to_string()),
            scene: replacement.clone(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            runtime_api_version: RUNTIME_API_VERSION,
        };

        let update_kind = apply_runtime_scene_update(&mut state, snapshot);

        assert_eq!(update_kind, RuntimeSceneUpdateKind::ExternalRefresh);
        assert_eq!(state.scene, replacement);
        assert_eq!(state.history.undo_len(), 0);
        assert_eq!(state.history.redo_len(), 0);
        assert_eq!(state.session_id.as_deref(), Some("default"));
        assert_eq!(state.scene_path, PathBuf::from("/tmp/other-scene.json"));
    }

    #[test]
    fn paint_state_borrow_skips_reentrant_paint() {
        let state = Rc::new(RefCell::new(sample_state(Tool::Select)));
        let state_borrow = state.borrow_mut();

        assert!(try_borrow_paint_state(&state).is_none());

        drop(state_borrow);
        assert!(try_borrow_paint_state(&state).is_some());
    }

    #[test]
    fn screen_rect_uses_viewport_pan_and_zoom() {
        let viewport = ViewportState {
            pan_x: 100.0,
            pan_y: -50.0,
            zoom: 2.0,
        };

        let rect = screen_rect(SceneRect::new(10, 20, 30, 40), &viewport);

        assert_eq!(rect, SceneRect::new(120, -10, 60, 80));
    }

    #[test]
    fn visible_grid_step_expands_when_zoomed_out() {
        assert_eq!(visible_grid_step(1.0), 40.0);
        assert!(visible_grid_step(0.1) > 40.0);
        assert!(visible_grid_step(4.0) <= 40.0);
    }

    #[test]
    fn fitted_viewport_for_canvas_centers_bounds_with_padding() {
        let viewport =
            fitted_viewport_for_canvas(SceneRect::new(100, 80, 400, 200), Size::new(1000, 800))
                .expect("fitted viewport");

        let top_left = viewport.world_to_screen(GraphPoint::new(100.0, 80.0));
        let bottom_right = viewport.world_to_screen(GraphPoint::new(500.0, 280.0));

        assert!(top_left.x >= 47.0);
        assert!(top_left.y >= 47.0);
        assert!(bottom_right.x <= 953.0);
        assert!(bottom_right.y <= 753.0);
    }

    #[test]
    fn scene_bounds_unions_all_objects() {
        let mut scene = SceneDocument::default();
        scene.add_rectangle(SceneRect::new(10, 20, 30, 40), SceneStroke::default(), None);
        scene.add_circle(CanvasPoint::new(200, 150), 25, SceneStroke::default(), None);

        assert_eq!(scene_bounds(&scene), Some(SceneRect::new(10, 20, 215, 155)));
    }

    #[test]
    fn persist_routes_local_scene_updates_through_workspace() {
        let root = std::env::temp_dir().join(format!("boardmesh-gui-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp root");
        let scene_path = root.join("local.json");
        let scene = ensure_scene_file(&scene_path).expect("scene");
        let last_seen = last_modified(&scene_path).ok();
        let mut state =
            BoardmeshState::new(root.clone(), scene_path.clone(), None, scene, last_seen);

        state.scene.add_text(
            TextObject {
                position: CanvasPoint::new(10, 20),
                content: "persisted".to_string(),
            },
            SceneColor::rgb(3, 4, 5),
        );

        persist(&mut state).expect("persist local scene");

        let persisted = load_scene(&scene_path).expect("persisted scene");
        assert_eq!(persisted.object_count(), 1);
        assert_eq!(state.scene.object_count(), 1);

        let _ = std::fs::remove_dir_all(root);
    }
}
