#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarDock {
    Top,
    Left,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutState {
    pub workspace_toolbar_visible: bool,
    pub tools_toolbar_visible: bool,
    pub objects_toolbar_visible: bool,
    pub session_panel_visible: bool,
    pub objects_panel_visible: bool,
    pub inspector_panel_visible: bool,
    pub history_panel_visible: bool,
    pub auto_reveal_objects_panel: bool,
    pub auto_reveal_inspector_panel: bool,
    pub workspace_toolbar_dock: ToolbarDock,
    pub tools_toolbar_dock: ToolbarDock,
    pub objects_toolbar_dock: ToolbarDock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutContext {
    pub object_count: usize,
    pub active_tool_prefers_inspector: bool,
}

impl Default for LayoutState {
    fn default() -> Self {
        Self {
            workspace_toolbar_visible: true,
            tools_toolbar_visible: true,
            objects_toolbar_visible: true,
            session_panel_visible: false,
            objects_panel_visible: false,
            inspector_panel_visible: false,
            history_panel_visible: false,
            auto_reveal_objects_panel: true,
            auto_reveal_inspector_panel: true,
            workspace_toolbar_dock: ToolbarDock::Top,
            tools_toolbar_dock: ToolbarDock::Left,
            objects_toolbar_dock: ToolbarDock::Right,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutAction {
    SetToolbarClusterVisibility(bool),
    ToggleToolbarCluster,
    ToggleWorkspaceToolbar,
    ToggleToolsToolbar,
    ToggleObjectsToolbar,
    ToggleSessionPanel,
    ToggleObjectsPanel,
    ToggleInspectorPanel,
    ToggleHistoryPanel,
    SetWorkspaceToolbarDock(ToolbarDock),
    SetToolsToolbarDock(ToolbarDock),
    SetObjectsToolbarDock(ToolbarDock),
    ResetLayout,
}

pub fn set_toolbar_cluster_visibility(layout: &mut LayoutState, visible: bool) -> bool {
    let changed = layout.workspace_toolbar_visible != visible
        || layout.tools_toolbar_visible != visible
        || layout.objects_toolbar_visible != visible;
    layout.workspace_toolbar_visible = visible;
    layout.tools_toolbar_visible = visible;
    layout.objects_toolbar_visible = visible;
    changed
}

pub fn apply_contextual_layout_hints(layout: &mut LayoutState, context: LayoutContext) -> bool {
    let mut changed = false;

    if layout.auto_reveal_objects_panel && context.object_count > 0 && !layout.objects_panel_visible
    {
        layout.objects_panel_visible = true;
        layout.auto_reveal_objects_panel = false;
        changed = true;
    }

    if layout.auto_reveal_inspector_panel
        && context.active_tool_prefers_inspector
        && !layout.inspector_panel_visible
    {
        layout.inspector_panel_visible = true;
        layout.auto_reveal_inspector_panel = false;
        changed = true;
    }

    changed
}

pub fn apply_layout_action(layout: &mut LayoutState, action: LayoutAction) -> bool {
    match action {
        LayoutAction::SetToolbarClusterVisibility(visible) => {
            set_toolbar_cluster_visibility(layout, visible)
        }
        LayoutAction::ToggleToolbarCluster => {
            let show_all = !(layout.workspace_toolbar_visible
                && layout.tools_toolbar_visible
                && layout.objects_toolbar_visible);
            set_toolbar_cluster_visibility(layout, show_all)
        }
        LayoutAction::ToggleWorkspaceToolbar => {
            layout.workspace_toolbar_visible = !layout.workspace_toolbar_visible;
            true
        }
        LayoutAction::ToggleToolsToolbar => {
            layout.tools_toolbar_visible = !layout.tools_toolbar_visible;
            true
        }
        LayoutAction::ToggleObjectsToolbar => {
            layout.objects_toolbar_visible = !layout.objects_toolbar_visible;
            true
        }
        LayoutAction::ToggleSessionPanel => {
            layout.session_panel_visible = !layout.session_panel_visible;
            true
        }
        LayoutAction::ToggleObjectsPanel => {
            layout.objects_panel_visible = !layout.objects_panel_visible;
            layout.auto_reveal_objects_panel = false;
            true
        }
        LayoutAction::ToggleInspectorPanel => {
            layout.inspector_panel_visible = !layout.inspector_panel_visible;
            layout.auto_reveal_inspector_panel = false;
            true
        }
        LayoutAction::ToggleHistoryPanel => {
            layout.history_panel_visible = !layout.history_panel_visible;
            true
        }
        LayoutAction::SetWorkspaceToolbarDock(direction) => {
            layout.workspace_toolbar_dock = direction;
            true
        }
        LayoutAction::SetToolsToolbarDock(direction) => {
            layout.tools_toolbar_dock = direction;
            true
        }
        LayoutAction::SetObjectsToolbarDock(direction) => {
            layout.objects_toolbar_dock = direction;
            true
        }
        LayoutAction::ResetLayout => {
            *layout = LayoutState::default();
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_actions() -> Vec<LayoutAction> {
        vec![
            LayoutAction::SetToolbarClusterVisibility(false),
            LayoutAction::SetToolbarClusterVisibility(true),
            LayoutAction::ToggleToolbarCluster,
            LayoutAction::ToggleWorkspaceToolbar,
            LayoutAction::ToggleToolsToolbar,
            LayoutAction::ToggleObjectsToolbar,
            LayoutAction::ToggleSessionPanel,
            LayoutAction::ToggleObjectsPanel,
            LayoutAction::ToggleInspectorPanel,
            LayoutAction::ToggleHistoryPanel,
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Top),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Left),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Right),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Bottom),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Top),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Left),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Right),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Bottom),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Top),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Left),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Right),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Bottom),
            LayoutAction::ResetLayout,
        ]
    }

    #[test]
    fn toggling_toolbar_cluster_hides_then_restores_all_toolbars() {
        let mut layout = LayoutState::default();

        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::ToggleToolbarCluster
        ));
        assert!(!layout.workspace_toolbar_visible);
        assert!(!layout.tools_toolbar_visible);
        assert!(!layout.objects_toolbar_visible);

        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::ToggleToolbarCluster
        ));
        assert!(layout.workspace_toolbar_visible);
        assert!(layout.tools_toolbar_visible);
        assert!(layout.objects_toolbar_visible);
    }

    #[test]
    fn setting_toolbar_cluster_visibility_restores_partial_toolbar_state() {
        let mut layout = LayoutState {
            tools_toolbar_visible: false,
            ..LayoutState::default()
        };

        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::SetToolbarClusterVisibility(true)
        ));
        assert!(layout.workspace_toolbar_visible);
        assert!(layout.tools_toolbar_visible);
        assert!(layout.objects_toolbar_visible);
    }

    #[test]
    fn reset_layout_restores_default_docking_and_visibility() {
        let mut layout = LayoutState {
            workspace_toolbar_visible: false,
            tools_toolbar_visible: false,
            session_panel_visible: false,
            workspace_toolbar_dock: ToolbarDock::Bottom,
            tools_toolbar_dock: ToolbarDock::Top,
            ..LayoutState::default()
        };

        assert!(apply_layout_action(&mut layout, LayoutAction::ResetLayout));

        assert!(layout.workspace_toolbar_visible);
        assert!(layout.tools_toolbar_visible);
        assert!(!layout.session_panel_visible);
        assert_eq!(layout.workspace_toolbar_dock, ToolbarDock::Top);
        assert_eq!(layout.tools_toolbar_dock, ToolbarDock::Left);
    }

    #[test]
    fn default_layout_keeps_secondary_panels_hidden() {
        let layout = LayoutState::default();

        assert!(layout.workspace_toolbar_visible);
        assert!(layout.tools_toolbar_visible);
        assert!(layout.objects_toolbar_visible);
        assert!(!layout.session_panel_visible);
        assert!(!layout.objects_panel_visible);
        assert!(!layout.inspector_panel_visible);
        assert!(!layout.history_panel_visible);
    }

    #[test]
    fn every_action_followed_by_reset_returns_default_state() {
        for action in sample_actions() {
            let mut layout = LayoutState::default();
            apply_layout_action(&mut layout, action);
            assert!(apply_layout_action(&mut layout, LayoutAction::ResetLayout));
            assert_eq!(
                layout.workspace_toolbar_visible,
                LayoutState::default().workspace_toolbar_visible
            );
            assert_eq!(
                layout.tools_toolbar_visible,
                LayoutState::default().tools_toolbar_visible
            );
            assert_eq!(
                layout.objects_toolbar_visible,
                LayoutState::default().objects_toolbar_visible
            );
            assert_eq!(
                layout.session_panel_visible,
                LayoutState::default().session_panel_visible
            );
            assert_eq!(
                layout.objects_panel_visible,
                LayoutState::default().objects_panel_visible
            );
            assert_eq!(
                layout.inspector_panel_visible,
                LayoutState::default().inspector_panel_visible
            );
            assert_eq!(
                layout.history_panel_visible,
                LayoutState::default().history_panel_visible
            );
            assert_eq!(
                layout.auto_reveal_objects_panel,
                LayoutState::default().auto_reveal_objects_panel
            );
            assert_eq!(
                layout.auto_reveal_inspector_panel,
                LayoutState::default().auto_reveal_inspector_panel
            );
            assert_eq!(
                layout.workspace_toolbar_dock,
                LayoutState::default().workspace_toolbar_dock
            );
            assert_eq!(
                layout.tools_toolbar_dock,
                LayoutState::default().tools_toolbar_dock
            );
            assert_eq!(
                layout.objects_toolbar_dock,
                LayoutState::default().objects_toolbar_dock
            );
        }
    }

    #[test]
    fn action_pairs_keep_toolbar_cluster_actions_consistent() {
        let actions = sample_actions();
        for first in &actions {
            for second in &actions {
                let mut layout = LayoutState::default();
                apply_layout_action(&mut layout, *first);
                apply_layout_action(&mut layout, *second);

                let mut after_hide = layout;
                apply_layout_action(
                    &mut after_hide,
                    LayoutAction::SetToolbarClusterVisibility(false),
                );
                assert!(!after_hide.workspace_toolbar_visible);
                assert!(!after_hide.tools_toolbar_visible);
                assert!(!after_hide.objects_toolbar_visible);

                let mut after_show = layout;
                apply_layout_action(
                    &mut after_show,
                    LayoutAction::SetToolbarClusterVisibility(true),
                );
                assert!(after_show.workspace_toolbar_visible);
                assert!(after_show.tools_toolbar_visible);
                assert!(after_show.objects_toolbar_visible);
            }
        }
    }

    #[test]
    fn context_hints_reveal_inspector_once_for_drawing_tools() {
        let mut layout = LayoutState::default();

        assert!(apply_contextual_layout_hints(
            &mut layout,
            LayoutContext {
                object_count: 0,
                active_tool_prefers_inspector: true,
            }
        ));
        assert!(layout.inspector_panel_visible);

        layout.inspector_panel_visible = false;
        assert!(!apply_contextual_layout_hints(
            &mut layout,
            LayoutContext {
                object_count: 0,
                active_tool_prefers_inspector: true,
            }
        ));
        assert!(!layout.inspector_panel_visible);
    }

    #[test]
    fn context_hints_reveal_objects_once_when_board_gains_content() {
        let mut layout = LayoutState::default();

        assert!(apply_contextual_layout_hints(
            &mut layout,
            LayoutContext {
                object_count: 1,
                active_tool_prefers_inspector: false,
            }
        ));
        assert!(layout.objects_panel_visible);

        layout.objects_panel_visible = false;
        assert!(!apply_contextual_layout_hints(
            &mut layout,
            LayoutContext {
                object_count: 3,
                active_tool_prefers_inspector: false,
            }
        ));
        assert!(!layout.objects_panel_visible);
    }

    #[test]
    fn manual_panel_toggles_disable_future_auto_reveal() {
        let mut layout = LayoutState::default();

        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::ToggleInspectorPanel
        ));
        assert!(layout.inspector_panel_visible);
        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::ToggleInspectorPanel
        ));
        assert!(!layout.inspector_panel_visible);

        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::ToggleObjectsPanel
        ));
        assert!(layout.objects_panel_visible);
        assert!(apply_layout_action(
            &mut layout,
            LayoutAction::ToggleObjectsPanel
        ));
        assert!(!layout.objects_panel_visible);

        assert!(!apply_contextual_layout_hints(
            &mut layout,
            LayoutContext {
                object_count: 2,
                active_tool_prefers_inspector: true,
            }
        ));
        assert!(!layout.inspector_panel_visible);
        assert!(!layout.objects_panel_visible);
    }
}
