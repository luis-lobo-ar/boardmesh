use super::*;

pub(super) fn show_info_dialog(parent: &Frame, caption: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, caption)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::Centre)
        .build();
    let _ = dialog.show_modal();
}

pub(super) fn show_about_dialog(parent: &Frame) {
    let mut info = AboutDialogInfo::new();
    info.set_name("Boardmesh");
    info.set_version(env!("CARGO_PKG_VERSION"));
    info.set_description(
        "A local board desktop app and MCP server for shared, agent-driven diagramming.",
    );
    info.set_copyright("Copyright 2026 Luis Lobo");
    info.set_website_ex(
        "https://github.com/luislobo/mesh/tree/main/opt/boardmesh",
        "Project repository",
    );
    info.add_developer("Luis Lobo");
    if let Some(icon) = ArtProvider::get_bitmap(ArtId::Information, ArtClient::Dialog, None) {
        info.set_icon(&icon);
    }
    show_about_box(&info, Some(parent));
}

pub(super) fn scene_summary_label(
    session_id: Option<&str>,
    scene_path: &PathBuf,
    server_status: &ServerStatus,
) -> String {
    match session_id {
        Some(session_id) => format!(
            "Session: {session_id}\nScene file:\n{}\n{}",
            scene_path.display(),
            server_status_label(server_status)
        ),
        None => format!(
            "Raw scene file:\n{}\n{}",
            scene_path.display(),
            server_status_label(server_status)
        ),
    }
}

pub(super) fn session_title_fragment(session_id: Option<&str>) -> String {
    session_id
        .map(|session_id| format!(" [{session_id}]"))
        .unwrap_or_default()
}

fn server_status_label(status: &ServerStatus) -> String {
    match status {
        ServerStatus::None => "MCP: not connected".to_string(),
        ServerStatus::Connected {
            app_version,
            runtime_api_version,
        } => format!("MCP: connected v{app_version} (api {runtime_api_version})"),
        ServerStatus::Incompatible {
            app_version,
            runtime_api_version,
        } => format!(
            "MCP: incompatible v{app_version} (api {runtime_api_version}, GUI expects api {RUNTIME_API_VERSION})"
        ),
        ServerStatus::Stale {
            app_version,
            runtime_api_version,
        } => format!("MCP: stale v{app_version} (api {runtime_api_version})"),
        ServerStatus::Error(error) => format!("MCP: status unavailable ({error})"),
    }
}

pub(super) fn short_server_status_label(status: &ServerStatus) -> String {
    match status {
        ServerStatus::None => "MCP offline".to_string(),
        ServerStatus::Connected { app_version, .. } => format!("MCP connected v{app_version}"),
        ServerStatus::Incompatible {
            app_version,
            runtime_api_version,
        } => format!("MCP incompatible v{app_version} api {runtime_api_version}"),
        ServerStatus::Stale { app_version, .. } => format!("MCP stale v{app_version}"),
        ServerStatus::Error(error) => format!("MCP error: {error}"),
    }
}

pub(super) fn selection_summary_text(state: &BoardmeshState) -> String {
    match state.scene.selected_object_ids.len() {
        0 => "Selection: nothing selected".to_string(),
        1 => "Selection: 1 object selected".to_string(),
        count => format!("Selection: {count} objects selected"),
    }
}

pub(super) fn tool_state_summary(state: &BoardmeshState) -> String {
    if matches!(state.active_tool, Tool::Fill) {
        return "State: click a shape to recolor it".to_string();
    }

    if matches!(state.active_tool, Tool::Polygon) {
        if let Some(DraftShape::Polygon { points, .. }) = &state.draft {
            return format!("State: polygon draft with {} points", points.len());
        }
        return "State: click to place vertices, Enter or right click to finish".to_string();
    }

    if matches!(state.active_tool, Tool::Triangle)
        && matches!(state.triangle_mode, TriangleMode::ThreePoint)
    {
        if let Some(DraftShape::TrianglePoints { points, .. }) = &state.draft {
            return format!("State: 3-point triangle draft with {} points", points.len());
        }
        return "State: click 3 points to place a triangle".to_string();
    }

    if state.draft.is_some() {
        return "State: drag on the canvas to finish the active draft".to_string();
    }

    "State: ready".to_string()
}
