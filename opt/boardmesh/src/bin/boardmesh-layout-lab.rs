use boardmesh::{LayoutAction, LayoutState, ToolbarDock, apply_layout_action};
use serde::Serialize;
use std::cell::RefCell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use wxdragon::menus::menuitem::{ID_ABOUT, ID_EXIT, ItemKind as MenuItemKind};
use wxdragon::prelude::*;

const FRAME_WIDTH: i32 = 1280;
const FRAME_HEIGHT: i32 = 860;
const SESSION_PANEL_WIDTH: i32 = 280;
const INSPECTOR_PANEL_WIDTH: i32 = 280;
const OBJECTS_PANEL_WIDTH: i32 = 260;
const LOG_PANEL_HEIGHT: i32 = 180;
const MENU_ID_PIN_TOOLBARS: i32 = 5101;
const MENU_ID_SHOW_WORKSPACE_TOOLBAR: i32 = 5102;
const MENU_ID_SHOW_TOOLS_TOOLBAR: i32 = 5103;
const MENU_ID_SHOW_OBJECTS_TOOLBAR: i32 = 5104;
const MENU_ID_SHOW_SESSION_PANE: i32 = 5105;
const MENU_ID_SHOW_OBJECTS_PANE: i32 = 5106;
const MENU_ID_SHOW_INSPECTOR_PANE: i32 = 5107;
const MENU_ID_RESET_LAYOUT: i32 = 5108;
const MENU_ID_WORKSPACE_TOOLBAR_TOP: i32 = 5109;
const MENU_ID_WORKSPACE_TOOLBAR_LEFT: i32 = 5110;
const MENU_ID_WORKSPACE_TOOLBAR_RIGHT: i32 = 5111;
const MENU_ID_WORKSPACE_TOOLBAR_BOTTOM: i32 = 5112;
const MENU_ID_TOOLS_TOOLBAR_TOP: i32 = 5113;
const MENU_ID_TOOLS_TOOLBAR_LEFT: i32 = 5114;
const MENU_ID_TOOLS_TOOLBAR_RIGHT: i32 = 5115;
const MENU_ID_TOOLS_TOOLBAR_BOTTOM: i32 = 5116;
const MENU_ID_OBJECTS_TOOLBAR_TOP: i32 = 5117;
const MENU_ID_OBJECTS_TOOLBAR_LEFT: i32 = 5118;
const MENU_ID_OBJECTS_TOOLBAR_RIGHT: i32 = 5119;
const MENU_ID_OBJECTS_TOOLBAR_BOTTOM: i32 = 5120;
const HELP_TEXT: &str = "Usage: boardmesh-layout-lab [--exercise SCENARIO] [--report PATH]\n\n\
                     Scenarios:\n\
                     - all: run the full layout action matrix and write an optional JSON report";

#[derive(Debug, Clone)]
struct LabOptions {
    exercise: Option<String>,
    report_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
struct ExerciseStepReport {
    label: String,
    summary: String,
}

#[derive(Debug, Clone, Serialize)]
struct ExerciseReport {
    scenario: String,
    steps: Vec<ExerciseStepReport>,
    final_summary: String,
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
    workspace_toolbar: Panel,
    tools_toolbar: Panel,
    objects_toolbar: Panel,
    session_panel: Panel,
    objects_panel: Panel,
    inspector_panel: Panel,
    log_panel: Panel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolbarPane {
    Workspace,
    Tools,
    Objects,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = parse_args().map_err(std::io::Error::other)?;

    wxdragon::main(move |app| {
        app.set_app_name("boardmesh-layout-lab");
        app.set_app_display_name("Boardmesh Layout Lab");

        let frame = Frame::builder()
            .with_title("Boardmesh Layout Lab")
            .with_size(Size::new(FRAME_WIDTH, FRAME_HEIGHT))
            .build();
        app.set_top_window(&frame);
        frame.centre();

        let status_bar = StatusBar::builder(&frame)
            .with_fields_count(2)
            .with_status_widths(vec![-3, -4])
            .build();
        let manager = AuiManager::builder(&frame).build();
        let canvas = Panel::builder(&frame).build();
        let session_panel = Panel::builder(&frame)
            .with_size(Size::new(SESSION_PANEL_WIDTH, 320))
            .build();
        let objects_panel = Panel::builder(&frame)
            .with_size(Size::new(OBJECTS_PANEL_WIDTH, 320))
            .build();
        let inspector_panel = Panel::builder(&frame)
            .with_size(Size::new(INSPECTOR_PANEL_WIDTH, 320))
            .build();
        let workspace_toolbar = Panel::builder(&frame).with_size(Size::new(520, 60)).build();
        let tools_toolbar = Panel::builder(&frame)
            .with_size(Size::new(110, 260))
            .build();
        let objects_toolbar = Panel::builder(&frame)
            .with_size(Size::new(120, 260))
            .build();
        let log_panel = Panel::builder(&frame)
            .with_size(Size::new(600, LOG_PANEL_HEIGHT))
            .build();

        canvas.set_background_color(Colour::new(247, 248, 250, 255));
        session_panel.set_background_color(Colour::new(244, 247, 255, 255));
        objects_panel.set_background_color(Colour::new(248, 250, 244, 255));
        inspector_panel.set_background_color(Colour::new(255, 248, 242, 255));
        workspace_toolbar.set_background_color(Colour::new(255, 255, 255, 255));
        tools_toolbar.set_background_color(Colour::new(255, 255, 255, 255));
        objects_toolbar.set_background_color(Colour::new(255, 255, 255, 255));
        log_panel.set_background_color(Colour::new(255, 255, 255, 255));

        let file_menu = Menu::builder().build();
        let _exit_item = file_menu
            .append(
                ID_EXIT,
                "E&xit\tCtrl+Q",
                "Close the lab window",
                MenuItemKind::Normal,
            )
            .expect("failed to create exit item");
        let view_menu = Menu::builder().build();
        let pin_toolbars_item = view_menu
            .append(
                MENU_ID_PIN_TOOLBARS,
                "&Show Toolbars\tCtrl+P",
                "Show or hide all toolbars",
                MenuItemKind::Check,
            )
            .expect("failed to create show toolbars item");
        view_menu.append_separator();
        let show_workspace_toolbar_item = view_menu
            .append(
                MENU_ID_SHOW_WORKSPACE_TOOLBAR,
                "Show &Workspace Toolbar",
                "Show or hide the workspace toolbar",
                MenuItemKind::Check,
            )
            .expect("failed to create workspace toolbar item");
        let show_tools_toolbar_item = view_menu
            .append(
                MENU_ID_SHOW_TOOLS_TOOLBAR,
                "Show &Tools Toolbar",
                "Show or hide the tools toolbar",
                MenuItemKind::Check,
            )
            .expect("failed to create tools toolbar item");
        let show_objects_toolbar_item = view_menu
            .append(
                MENU_ID_SHOW_OBJECTS_TOOLBAR,
                "Show Ob&jects Toolbar",
                "Show or hide the objects toolbar",
                MenuItemKind::Check,
            )
            .expect("failed to create objects toolbar item");
        view_menu.append_separator();

        let workspace_toolbar_menu = build_toolbar_position_menu(
            MENU_ID_WORKSPACE_TOOLBAR_TOP,
            MENU_ID_WORKSPACE_TOOLBAR_LEFT,
            MENU_ID_WORKSPACE_TOOLBAR_RIGHT,
            MENU_ID_WORKSPACE_TOOLBAR_BOTTOM,
            "workspace toolbar",
        );
        view_menu
            .append_submenu(
                workspace_toolbar_menu,
                "Workspace Toolbar &Position",
                "Move the workspace toolbar",
            )
            .expect("failed to create workspace toolbar submenu");

        let tools_toolbar_menu = build_toolbar_position_menu(
            MENU_ID_TOOLS_TOOLBAR_TOP,
            MENU_ID_TOOLS_TOOLBAR_LEFT,
            MENU_ID_TOOLS_TOOLBAR_RIGHT,
            MENU_ID_TOOLS_TOOLBAR_BOTTOM,
            "tools toolbar",
        );
        view_menu
            .append_submenu(
                tools_toolbar_menu,
                "Tools Toolbar P&osition",
                "Move the tools toolbar",
            )
            .expect("failed to create tools toolbar submenu");

        let objects_toolbar_menu = build_toolbar_position_menu(
            MENU_ID_OBJECTS_TOOLBAR_TOP,
            MENU_ID_OBJECTS_TOOLBAR_LEFT,
            MENU_ID_OBJECTS_TOOLBAR_RIGHT,
            MENU_ID_OBJECTS_TOOLBAR_BOTTOM,
            "objects toolbar",
        );
        view_menu
            .append_submenu(
                objects_toolbar_menu,
                "Objects Toolbar Po&sition",
                "Move the objects toolbar",
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
            .expect("failed to create session pane item");
        let show_objects_pane_item = view_menu
            .append(
                MENU_ID_SHOW_OBJECTS_PANE,
                "Show Objects Pane",
                "Show or hide the objects pane",
                MenuItemKind::Check,
            )
            .expect("failed to create objects pane item");
        let show_inspector_pane_item = view_menu
            .append(
                MENU_ID_SHOW_INSPECTOR_PANE,
                "Show &Inspector",
                "Show or hide the inspector pane",
                MenuItemKind::Check,
            )
            .expect("failed to create inspector item");
        view_menu.append_separator();
        let _reset_layout_item = view_menu
            .append(
                MENU_ID_RESET_LAYOUT,
                "&Reset Layout",
                "Restore the default lab layout",
                MenuItemKind::Normal,
            )
            .expect("failed to create reset item");

        let help_menu = Menu::builder().build();
        let _about_item = help_menu
            .append(
                ID_ABOUT,
                "&About Layout Lab",
                "Explain the layout lab",
                MenuItemKind::Normal,
            )
            .expect("failed to create about item");

        let menu_bar = MenuBar::builder()
            .append(file_menu, "&File")
            .append(view_menu, "&View")
            .append(help_menu, "&Help")
            .build();
        frame.set_menu_bar(menu_bar);

        let session_title = StaticText::builder(&session_panel)
            .with_label("Session Pane")
            .build();
        let session_copy = StaticText::builder(&session_panel)
            .with_label("Use this pane to verify simple show / hide behavior from the View menu.")
            .with_size(Size::new(240, 72))
            .build();
        session_copy.wrap(240);
        let session_sizer = BoxSizer::builder(Orientation::Vertical).build();
        session_sizer.add(&session_title, 0, SizerFlag::All, 10);
        session_sizer.add(
            &session_copy,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            10,
        );
        session_panel.set_sizer(session_sizer, false);

        let objects_title = StaticText::builder(&objects_panel)
            .with_label("Objects Pane")
            .build();
        let objects_copy = StaticText::builder(&objects_panel)
            .with_label("This pane is just a placeholder for docking and visibility tests.")
            .with_size(Size::new(220, 72))
            .build();
        objects_copy.wrap(220);
        let objects_sizer = BoxSizer::builder(Orientation::Vertical).build();
        objects_sizer.add(&objects_title, 0, SizerFlag::All, 10);
        objects_sizer.add(
            &objects_copy,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            10,
        );
        objects_panel.set_sizer(objects_sizer, false);

        let inspector_title = StaticText::builder(&inspector_panel)
            .with_label("Inspector Pane")
            .build();
        let inspector_copy = StaticText::builder(&inspector_panel)
            .with_label("The lab keeps this pane simple so layout issues are easier to isolate.")
            .with_size(Size::new(240, 72))
            .build();
        inspector_copy.wrap(240);
        let inspector_sizer = BoxSizer::builder(Orientation::Vertical).build();
        inspector_sizer.add(&inspector_title, 0, SizerFlag::All, 10);
        inspector_sizer.add(
            &inspector_copy,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            10,
        );
        inspector_panel.set_sizer(inspector_sizer, false);

        let canvas_title = StaticText::builder(&canvas)
            .with_label("Layout Lab Canvas")
            .build();
        let canvas_copy = StaticText::builder(&canvas)
            .with_label(
                "This center pane does not draw anything.\nUse the menus and toolbar buttons to stress-test docking, visibility, and state sync.",
            )
            .with_size(Size::new(520, 96))
            .build();
        canvas_copy.wrap(520);
        let canvas_sizer = BoxSizer::builder(Orientation::Vertical).build();
        canvas_sizer.add_spacer(16);
        canvas_sizer.add(
            &canvas_title,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            16,
        );
        canvas_sizer.add(&canvas_copy, 0, SizerFlag::Left | SizerFlag::Right, 16);
        canvas.set_sizer(canvas_sizer, false);

        let log_title = StaticText::builder(&log_panel)
            .with_label("Event Log")
            .build();
        let log_list = ListBox::builder(&log_panel)
            .with_size(Size::new(640, 120))
            .build();
        let clear_log_button = Button::builder(&log_panel).with_label("Clear Log").build();
        let log_sizer = BoxSizer::builder(Orientation::Vertical).build();
        log_sizer.add(&log_title, 0, SizerFlag::All, 8);
        log_sizer.add(
            &log_list,
            1,
            SizerFlag::Expand | SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            8,
        );
        log_sizer.add(
            &clear_log_button,
            0,
            SizerFlag::Left | SizerFlag::Right | SizerFlag::Bottom,
            8,
        );
        log_panel.set_sizer(log_sizer, false);

        let toggle_toolbars_button = Button::builder(&workspace_toolbar)
            .with_label("Toggle Toolbars")
            .build();
        let reset_layout_button = Button::builder(&workspace_toolbar)
            .with_label("Reset Layout")
            .build();
        let toggle_session_button = Button::builder(&workspace_toolbar)
            .with_label("Toggle Session")
            .build();
        let toggle_inspector_button = Button::builder(&workspace_toolbar)
            .with_label("Toggle Inspector")
            .build();
        let workspace_toolbar_sizer = BoxSizer::builder(Orientation::Horizontal).build();
        workspace_toolbar_sizer.add(&toggle_toolbars_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&reset_layout_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&toggle_session_button, 0, SizerFlag::All, 6);
        workspace_toolbar_sizer.add(&toggle_inspector_button, 0, SizerFlag::All, 6);
        workspace_toolbar.set_sizer(workspace_toolbar_sizer, false);

        let tools_left_button = Button::builder(&tools_toolbar).with_label("Left").build();
        let tools_right_button = Button::builder(&tools_toolbar).with_label("Right").build();
        let tools_top_button = Button::builder(&tools_toolbar).with_label("Top").build();
        let tools_bottom_button = Button::builder(&tools_toolbar).with_label("Bottom").build();
        let tools_toolbar_sizer = BoxSizer::builder(Orientation::Vertical).build();
        tools_toolbar_sizer.add(
            &StaticText::builder(&tools_toolbar)
                .with_label("Tools Toolbar")
                .build(),
            0,
            SizerFlag::All,
            6,
        );
        tools_toolbar_sizer.add(&tools_left_button, 0, SizerFlag::All, 6);
        tools_toolbar_sizer.add(&tools_right_button, 0, SizerFlag::All, 6);
        tools_toolbar_sizer.add(&tools_top_button, 0, SizerFlag::All, 6);
        tools_toolbar_sizer.add(&tools_bottom_button, 0, SizerFlag::All, 6);
        tools_toolbar.set_sizer(tools_toolbar_sizer, false);

        let toggle_objects_button = Button::builder(&objects_toolbar)
            .with_label("Objects Pane")
            .build();
        let objects_left_button = Button::builder(&objects_toolbar)
            .with_label("Dock Left")
            .build();
        let objects_right_button = Button::builder(&objects_toolbar)
            .with_label("Dock Right")
            .build();
        let objects_top_button = Button::builder(&objects_toolbar)
            .with_label("Dock Top")
            .build();
        let objects_bottom_button = Button::builder(&objects_toolbar)
            .with_label("Dock Bottom")
            .build();
        let objects_toolbar_sizer = BoxSizer::builder(Orientation::Vertical).build();
        objects_toolbar_sizer.add(
            &StaticText::builder(&objects_toolbar)
                .with_label("Objects Toolbar")
                .build(),
            0,
            SizerFlag::All,
            6,
        );
        objects_toolbar_sizer.add(&toggle_objects_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_left_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_right_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_top_button, 0, SizerFlag::All, 6);
        objects_toolbar_sizer.add(&objects_bottom_button, 0, SizerFlag::All, 6);
        objects_toolbar.set_sizer(objects_toolbar_sizer, false);

        let view_controls = ViewControls {
            pin_toolbars_item,
            show_workspace_toolbar_item,
            show_tools_toolbar_item,
            show_objects_toolbar_item,
            show_session_pane_item,
            show_objects_pane_item,
            show_inspector_pane_item,
            workspace_toolbar,
            tools_toolbar,
            objects_toolbar,
            session_panel,
            objects_panel,
            inspector_panel,
            log_panel,
        };
        let layout_state = Rc::new(RefCell::new(LayoutState::default()));
        apply_layout(&manager, &canvas, &view_controls, &layout_state.borrow());
        sync_view_controls(&view_controls, &layout_state.borrow());
        refresh_status(&status_bar, &layout_state.borrow(), "Ready");
        append_log(&log_list, "Lab started");

        clear_log_button.on_click({
            let log_list = log_list;
            move |_| log_list.clear()
        });

        bind_button_action(
            &toggle_toolbars_button,
            "Button: toggle toolbars",
            LayoutAction::ToggleToolbarCluster,
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &reset_layout_button,
            "Button: reset layout",
            LayoutAction::ResetLayout,
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &toggle_session_button,
            "Button: toggle session panel",
            LayoutAction::ToggleSessionPanel,
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &toggle_inspector_button,
            "Button: toggle inspector panel",
            LayoutAction::ToggleInspectorPanel,
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &tools_left_button,
            "Button: tools toolbar dock left",
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Left),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &tools_right_button,
            "Button: tools toolbar dock right",
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Right),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &tools_top_button,
            "Button: tools toolbar dock top",
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Top),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &tools_bottom_button,
            "Button: tools toolbar dock bottom",
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Bottom),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &toggle_objects_button,
            "Button: toggle objects pane",
            LayoutAction::ToggleObjectsPanel,
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &objects_left_button,
            "Button: objects toolbar dock left",
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Left),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &objects_right_button,
            "Button: objects toolbar dock right",
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Right),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &objects_top_button,
            "Button: objects toolbar dock top",
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Top),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );
        bind_button_action(
            &objects_bottom_button,
            "Button: objects toolbar dock bottom",
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Bottom),
            &layout_state,
            manager,
            &canvas,
            view_controls,
            &status_bar,
            &log_list,
        );

        frame.on_menu({
            let manager = manager;
            let canvas = canvas;
            let view_controls = view_controls;
            let layout_state = Rc::clone(&layout_state);
            let status_bar = status_bar;
            let log_list = log_list;
            let frame = frame;
            move |event| {
                if let Some(action) = layout_action_from_menu_id(event.get_id()) {
                    let message = format!("Menu: {}", menu_action_label(event.get_id()));
                    update_layout_state(&layout_state, manager, &canvas, view_controls, |layout| {
                        apply_layout_action(layout, action)
                    });
                    refresh_status(&status_bar, &layout_state.borrow(), &message);
                    append_log(&log_list, &message);
                    return;
                }

                match event.get_id() {
                    ID_ABOUT => show_info_dialog(
                        &frame,
                        "About Layout Lab",
                        "Boardmesh Layout Lab is a stripped-down wxDragon harness for testing menu-driven pane visibility, toolbar docking, and layout state synchronization without scene or drawing code.",
                    ),
                    ID_EXIT => frame.close(true),
                    _ => {}
                }
            }
        });

        frame.show(true);

        if let Some(exercise_name) = options.exercise.clone() {
            let timer = Rc::new(Timer::new(&frame));
            let frame = frame;
            let manager = manager;
            let canvas = canvas;
            let view_controls = view_controls;
            let layout_state = Rc::clone(&layout_state);
            let status_bar = status_bar;
            let log_list = log_list;
            let report_path = options.report_path.clone();
            let timer_for_callback = Rc::clone(&timer);
            timer.on_tick(move |_| {
                timer_for_callback.stop();
                match run_exercise_suite(
                    &exercise_name,
                    &layout_state,
                    manager,
                    &canvas,
                    view_controls,
                    &status_bar,
                    &log_list,
                    report_path.as_ref(),
                ) {
                    Ok(report) => {
                        append_log(&log_list, "Exercise completed");
                        refresh_status(&status_bar, &layout_state.borrow(), "Exercise completed");
                        eprintln!(
                            "layout-lab exercise '{}' completed: {}",
                            report.scenario, report.final_summary
                        );
                        frame.close(true);
                    }
                    Err(error) => {
                        append_log(&log_list, &format!("Exercise failed: {error}"));
                        refresh_status(&status_bar, &layout_state.borrow(), "Exercise failed");
                        eprintln!("layout-lab exercise failed: {error}");
                        frame.close(true);
                    }
                }
            });
            let _ = timer.start(1, true);
        }
    })?;

    Ok(())
}

fn build_toolbar_position_menu(
    top_id: i32,
    left_id: i32,
    right_id: i32,
    bottom_id: i32,
    label: &str,
) -> Menu {
    let menu = Menu::builder().build();
    menu.append(
        top_id,
        "Dock Top",
        &format!("Dock the {label} at the top edge"),
        MenuItemKind::Normal,
    )
    .expect("failed to create top dock item");
    menu.append(
        left_id,
        "Dock Left",
        &format!("Dock the {label} at the left edge"),
        MenuItemKind::Normal,
    )
    .expect("failed to create left dock item");
    menu.append(
        right_id,
        "Dock Right",
        &format!("Dock the {label} at the right edge"),
        MenuItemKind::Normal,
    )
    .expect("failed to create right dock item");
    menu.append(
        bottom_id,
        "Dock Bottom",
        &format!("Dock the {label} at the bottom edge"),
        MenuItemKind::Normal,
    )
    .expect("failed to create bottom dock item");
    menu
}

fn parse_args() -> Result<LabOptions, String> {
    let mut args = env::args().skip(1);
    let mut exercise = None;
    let mut report_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exercise" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--exercise requires a scenario value".to_string())?;
                exercise = Some(value);
            }
            "--report" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--report requires a file path".to_string())?;
                report_path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                println!("{HELP_TEXT}");
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
    }

    Ok(LabOptions {
        exercise,
        report_path,
    })
}

#[cfg(test)]
mod tests {
    use super::HELP_TEXT;

    #[test]
    fn help_text_uses_boardmesh_name() {
        assert!(HELP_TEXT.contains("boardmesh-layout-lab"));
        assert!(!HELP_TEXT.contains("whiteboard-layout-lab"));
    }
}

fn bind_button_action(
    button: &Button,
    message: &'static str,
    action: LayoutAction,
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: AuiManager,
    canvas: &Panel,
    view_controls: ViewControls,
    status_bar: &StatusBar,
    log_list: &ListBox,
) {
    let button = *button;
    let canvas = *canvas;
    let status_bar = *status_bar;
    let log_list = *log_list;
    let layout_state = Rc::clone(layout_state);
    button.on_click(move |_| {
        update_layout_state(&layout_state, manager, &canvas, view_controls, |layout| {
            apply_layout_action(layout, action)
        });
        refresh_status(&status_bar, &layout_state.borrow(), message);
        append_log(&log_list, message);
    });
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

fn layout_action_from_menu_id(menu_id: i32) -> Option<LayoutAction> {
    match menu_id {
        MENU_ID_PIN_TOOLBARS => Some(LayoutAction::ToggleToolbarCluster),
        MENU_ID_SHOW_WORKSPACE_TOOLBAR => Some(LayoutAction::ToggleWorkspaceToolbar),
        MENU_ID_SHOW_TOOLS_TOOLBAR => Some(LayoutAction::ToggleToolsToolbar),
        MENU_ID_SHOW_OBJECTS_TOOLBAR => Some(LayoutAction::ToggleObjectsToolbar),
        MENU_ID_SHOW_SESSION_PANE => Some(LayoutAction::ToggleSessionPanel),
        MENU_ID_SHOW_OBJECTS_PANE => Some(LayoutAction::ToggleObjectsPanel),
        MENU_ID_SHOW_INSPECTOR_PANE => Some(LayoutAction::ToggleInspectorPanel),
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

fn menu_action_label(menu_id: i32) -> &'static str {
    match menu_id {
        MENU_ID_PIN_TOOLBARS => "show toolbars",
        MENU_ID_SHOW_WORKSPACE_TOOLBAR => "show workspace toolbar",
        MENU_ID_SHOW_TOOLS_TOOLBAR => "show tools toolbar",
        MENU_ID_SHOW_OBJECTS_TOOLBAR => "show objects toolbar",
        MENU_ID_SHOW_SESSION_PANE => "show session panel",
        MENU_ID_SHOW_OBJECTS_PANE => "show objects pane",
        MENU_ID_SHOW_INSPECTOR_PANE => "show inspector pane",
        MENU_ID_WORKSPACE_TOOLBAR_TOP => "workspace toolbar dock top",
        MENU_ID_WORKSPACE_TOOLBAR_LEFT => "workspace toolbar dock left",
        MENU_ID_WORKSPACE_TOOLBAR_RIGHT => "workspace toolbar dock right",
        MENU_ID_WORKSPACE_TOOLBAR_BOTTOM => "workspace toolbar dock bottom",
        MENU_ID_TOOLS_TOOLBAR_TOP => "tools toolbar dock top",
        MENU_ID_TOOLS_TOOLBAR_LEFT => "tools toolbar dock left",
        MENU_ID_TOOLS_TOOLBAR_RIGHT => "tools toolbar dock right",
        MENU_ID_TOOLS_TOOLBAR_BOTTOM => "tools toolbar dock bottom",
        MENU_ID_OBJECTS_TOOLBAR_TOP => "objects toolbar dock top",
        MENU_ID_OBJECTS_TOOLBAR_LEFT => "objects toolbar dock left",
        MENU_ID_OBJECTS_TOOLBAR_RIGHT => "objects toolbar dock right",
        MENU_ID_OBJECTS_TOOLBAR_BOTTOM => "objects toolbar dock bottom",
        MENU_ID_RESET_LAYOUT => "reset layout",
        _ => "unknown action",
    }
}

fn canvas_pane_info() -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("lab-canvas")
        .center_pane()
        .caption_visible(false)
        .pane_border(false)
        .dockable(false)
}

fn toolbar_pane_info(toolbar: ToolbarPane, direction: ToolbarDock) -> AuiPaneInfo {
    let (name, caption, best_width, best_height, min_width, min_height, position) = match toolbar {
        ToolbarPane::Workspace => (
            "lab-workspace-toolbar",
            "Workspace Toolbar",
            520,
            60,
            360,
            50,
            0,
        ),
        ToolbarPane::Tools => ("lab-tools-toolbar", "Tools Toolbar", 120, 260, 96, 180, 1),
        ToolbarPane::Objects => (
            "lab-objects-toolbar",
            "Objects Toolbar",
            120,
            260,
            96,
            180,
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

fn session_pane_info() -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("lab-session-panel")
        .with_caption("Session")
        .left()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .pin_button(false)
        .best_size(SESSION_PANEL_WIDTH, 320)
        .min_size(220, 200)
        .row(1)
        .position(0)
}

fn objects_pane_info(visible: bool) -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("lab-objects-panel")
        .with_caption("Objects")
        .right()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .pin_button(false)
        .best_size(OBJECTS_PANEL_WIDTH, 320)
        .min_size(220, 200)
        .row(1)
        .position(0)
        .show(visible)
}

fn inspector_pane_info(visible: bool) -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("lab-inspector-panel")
        .with_caption("Inspector")
        .right()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .pin_button(false)
        .best_size(INSPECTOR_PANEL_WIDTH, 320)
        .min_size(220, 200)
        .row(1)
        .position(1)
        .show(visible)
}

fn log_pane_info() -> AuiPaneInfo {
    AuiPaneInfo::new()
        .with_name("lab-log-panel")
        .with_caption("Event Log")
        .bottom()
        .floatable(false)
        .dockable(true)
        .movable(false)
        .resizable(true)
        .close_button(false)
        .pin_button(false)
        .best_size(640, LOG_PANEL_HEIGHT)
        .min_size(320, 140)
        .row(2)
        .position(0)
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
    let _ = manager.detach_pane(&view_controls.log_panel);

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
        let _ = manager.add_pane_with_info(&view_controls.session_panel, session_pane_info());
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
    let _ = manager.add_pane_with_info(&view_controls.log_panel, log_pane_info());
    let _ = manager.update();
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
    view_controls.pin_toolbars_item.check(
        layout.workspace_toolbar_visible
            && layout.tools_toolbar_visible
            && layout.objects_toolbar_visible,
    );
}

fn refresh_status(status_bar: &StatusBar, layout: &LayoutState, message: &str) {
    status_bar.set_status_text(message, 0);
    status_bar.set_status_text(&layout_summary(layout), 1);
}

fn layout_summary(layout: &LayoutState) -> String {
    format!(
        "toolbars[W:{} T:{} O:{}] panes[S:{} O:{} I:{}] dock[W:{:?} T:{:?} O:{:?}]",
        on_off(layout.workspace_toolbar_visible),
        on_off(layout.tools_toolbar_visible),
        on_off(layout.objects_toolbar_visible),
        on_off(layout.session_panel_visible),
        on_off(layout.objects_panel_visible),
        on_off(layout.inspector_panel_visible),
        layout.workspace_toolbar_dock,
        layout.tools_toolbar_dock,
        layout.objects_toolbar_dock,
    )
}

fn on_off(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn append_log(log_list: &ListBox, message: &str) {
    log_list.append(message);
}

fn show_info_dialog(parent: &Frame, caption: &str, message: &str) {
    let dialog = MessageDialog::builder(parent, message, caption)
        .with_style(MessageDialogStyle::OK | MessageDialogStyle::Centre)
        .build();
    let _ = dialog.show_modal();
}

fn run_exercise_suite(
    scenario: &str,
    layout_state: &Rc<RefCell<LayoutState>>,
    manager: AuiManager,
    canvas: &Panel,
    view_controls: ViewControls,
    status_bar: &StatusBar,
    log_list: &ListBox,
    report_path: Option<&PathBuf>,
) -> Result<ExerciseReport, String> {
    let steps = match scenario {
        "all" => build_exercise_steps(),
        other => return Err(format!("unknown exercise scenario: {other}")),
    };

    let mut report_steps = Vec::new();
    for (label, action) in steps {
        update_layout_state(layout_state, manager, canvas, view_controls, |layout| {
            apply_layout_action(layout, action)
        });
        let layout = *layout_state.borrow();
        assert_view_matches_layout(&view_controls, &layout, &label)?;
        let summary = layout_summary(&layout);
        append_log(log_list, &format!("{label}: {summary}"));
        refresh_status(status_bar, &layout, &label);
        report_steps.push(ExerciseStepReport { label, summary });
    }

    let report = ExerciseReport {
        scenario: scenario.to_string(),
        final_summary: layout_summary(&layout_state.borrow()),
        steps: report_steps,
    };

    if let Some(path) = report_path {
        let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
        fs::write(path, format!("{json}\n")).map_err(|error| error.to_string())?;
    }

    Ok(report)
}

fn build_exercise_steps() -> Vec<(String, LayoutAction)> {
    let base_actions = vec![
        (
            "toggle workspace toolbar".to_string(),
            LayoutAction::ToggleWorkspaceToolbar,
        ),
        (
            "toggle tools toolbar".to_string(),
            LayoutAction::ToggleToolsToolbar,
        ),
        (
            "toggle objects toolbar".to_string(),
            LayoutAction::ToggleObjectsToolbar,
        ),
        (
            "toggle session panel".to_string(),
            LayoutAction::ToggleSessionPanel,
        ),
        (
            "toggle objects panel".to_string(),
            LayoutAction::ToggleObjectsPanel,
        ),
        (
            "toggle inspector panel".to_string(),
            LayoutAction::ToggleInspectorPanel,
        ),
        (
            "hide toolbar cluster".to_string(),
            LayoutAction::SetToolbarClusterVisibility(false),
        ),
        (
            "show toolbar cluster".to_string(),
            LayoutAction::SetToolbarClusterVisibility(true),
        ),
        (
            "workspace dock top".to_string(),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Top),
        ),
        (
            "workspace dock left".to_string(),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Left),
        ),
        (
            "workspace dock right".to_string(),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Right),
        ),
        (
            "workspace dock bottom".to_string(),
            LayoutAction::SetWorkspaceToolbarDock(ToolbarDock::Bottom),
        ),
        (
            "tools dock top".to_string(),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Top),
        ),
        (
            "tools dock left".to_string(),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Left),
        ),
        (
            "tools dock right".to_string(),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Right),
        ),
        (
            "tools dock bottom".to_string(),
            LayoutAction::SetToolsToolbarDock(ToolbarDock::Bottom),
        ),
        (
            "objects dock top".to_string(),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Top),
        ),
        (
            "objects dock left".to_string(),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Left),
        ),
        (
            "objects dock right".to_string(),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Right),
        ),
        (
            "objects dock bottom".to_string(),
            LayoutAction::SetObjectsToolbarDock(ToolbarDock::Bottom),
        ),
    ];

    let mut steps = vec![
        ("reset layout".to_string(), LayoutAction::ResetLayout),
        (
            "toggle toolbar cluster".to_string(),
            LayoutAction::ToggleToolbarCluster,
        ),
        (
            "toggle toolbar cluster again".to_string(),
            LayoutAction::ToggleToolbarCluster,
        ),
    ];

    for (label, action) in &base_actions {
        steps.push((format!("single: {label}"), *action));
        steps.push(("single: reset".to_string(), LayoutAction::ResetLayout));
    }

    for (index, (first_label, first_action)) in base_actions.iter().enumerate() {
        for (_, (second_label, second_action)) in base_actions.iter().enumerate().skip(index) {
            steps.push((format!("pair start: {first_label}"), *first_action));
            steps.push((format!("pair follow: {second_label}"), *second_action));
            steps.push((
                format!("pair reset: {first_label} + {second_label}"),
                LayoutAction::ResetLayout,
            ));
        }
    }

    steps
}

fn assert_view_matches_layout(
    view_controls: &ViewControls,
    layout: &LayoutState,
    label: &str,
) -> Result<(), String> {
    if view_controls.show_workspace_toolbar_item.is_checked() != layout.workspace_toolbar_visible {
        return Err(format!(
            "{label}: workspace toolbar menu check state drifted"
        ));
    }
    if view_controls.show_tools_toolbar_item.is_checked() != layout.tools_toolbar_visible {
        return Err(format!("{label}: tools toolbar menu check state drifted"));
    }
    if view_controls.show_objects_toolbar_item.is_checked() != layout.objects_toolbar_visible {
        return Err(format!("{label}: objects toolbar menu check state drifted"));
    }
    if view_controls.show_session_pane_item.is_checked() != layout.session_panel_visible {
        return Err(format!("{label}: session panel menu check state drifted"));
    }
    if view_controls.show_objects_pane_item.is_checked() != layout.objects_panel_visible {
        return Err(format!("{label}: objects pane menu check state drifted"));
    }
    if view_controls.show_inspector_pane_item.is_checked() != layout.inspector_panel_visible {
        return Err(format!("{label}: inspector pane menu check state drifted"));
    }

    if view_controls.workspace_toolbar.is_shown() != layout.workspace_toolbar_visible {
        return Err(format!("{label}: workspace toolbar visibility drifted"));
    }
    if view_controls.tools_toolbar.is_shown() != layout.tools_toolbar_visible {
        return Err(format!("{label}: tools toolbar visibility drifted"));
    }
    if view_controls.objects_toolbar.is_shown() != layout.objects_toolbar_visible {
        return Err(format!("{label}: objects toolbar visibility drifted"));
    }
    if view_controls.session_panel.is_shown() != layout.session_panel_visible {
        return Err(format!("{label}: session panel visibility drifted"));
    }
    if view_controls.objects_panel.is_shown() != layout.objects_panel_visible {
        return Err(format!("{label}: objects panel visibility drifted"));
    }
    if view_controls.inspector_panel.is_shown() != layout.inspector_panel_visible {
        return Err(format!("{label}: inspector panel visibility drifted"));
    }

    Ok(())
}
