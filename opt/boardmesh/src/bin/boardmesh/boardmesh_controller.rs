use super::*;

pub(super) fn finalize_multi_point_draft(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) -> bool {
    {
        let mut state = state.borrow_mut();
        let (points, create_triangle) = match state.draft.clone() {
            Some(DraftShape::Polygon { points, .. }) => {
                if points.len() < 3 {
                    return false;
                }
                (points, false)
            }
            Some(DraftShape::TrianglePoints { points, .. }) => {
                if points.len() != 3 {
                    return false;
                }
                (points, true)
            }
            _ => return false,
        };

        state.push_undo_snapshot();
        let fill = state.fill_shapes.then_some(state.selected_color);
        let stroke = state.stroke();
        if create_triangle {
            if let Err(error) = commit_triangle(
                &mut state,
                TriangleObject::new([points[0], points[1], points[2]]),
                stroke,
                fill,
            ) {
                eprintln!("failed to save multi-point shape update: {error:#}");
                let _ = state.history.pop_undo();
                return false;
            }
        } else {
            let Some(_) = (match commit_polygon(&mut state, points, stroke, fill) {
                Ok(object_id) => object_id,
                Err(error) => {
                    eprintln!("failed to save multi-point shape update: {error:#}");
                    let _ = state.history.pop_undo();
                    return false;
                }
            }) else {
                let _ = state.history.pop_undo();
                return false;
            };
        }
        state.draft = None;
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}

pub(super) fn apply_selection_box(
    state: &mut BoardmeshState,
    start: CanvasPoint,
    end: CanvasPoint,
    additive: bool,
) {
    let rect = SceneRect::from_points(start, end);
    let click_selection = rect.width <= 3 && rect.height <= 3;

    let selected_ids = if click_selection {
        match topmost_selectable_object_id(&state.scene, end) {
            Some(object_id) if additive => {
                let _ = state.scene.toggle_object_selection(&object_id);
                state.scene.selected_object_ids.clone()
            }
            Some(object_id) => vec![object_id],
            None if additive => state.scene.selected_object_ids.clone(),
            None => Vec::new(),
        }
    } else if additive {
        state.scene.select_objects_in_rect(rect, true)
    } else {
        state.scene.select_objects_in_rect(rect, false)
    };

    if let Err(error) = commit_selection_change(state, selected_ids) {
        eprintln!("failed to save selection change: {error:#}");
    }
}

pub(super) fn topmost_selectable_object_id(
    scene: &SceneDocument,
    point: CanvasPoint,
) -> Option<String> {
    scene
        .topmost_object_at(point)
        .map(|object| object.id.clone())
        .or_else(|| {
            let probe = SceneRect::new(point.x - 6, point.y - 6, 12, 12);
            scene
                .objects
                .iter()
                .rev()
                .find(|object| object.bounds().intersects(probe))
                .map(|object| object.id.clone())
        })
}

pub(super) fn cancel_active_draft(state: &Rc<RefCell<BoardmeshState>>, canvas: &Panel) -> bool {
    let should_release_mouse = canvas.has_capture();
    {
        let mut state = state.borrow_mut();
        if state.draft.is_none() {
            return false;
        }
        state.draft = None;
    }
    if should_release_mouse {
        canvas.release_mouse();
    }
    canvas.refresh(false, None);
    true
}

pub(super) fn delete_selected_objects(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) -> bool {
    {
        let mut state = state.borrow_mut();
        if state.scene.selected_object_ids.is_empty() {
            return false;
        }

        state.push_undo_snapshot();
        let deleted = match commit_delete_selected(&mut state) {
            Ok(deleted) => deleted,
            Err(error) => {
                eprintln!("failed to save deleted objects: {error:#}");
                let _ = state.history.pop_undo();
                return false;
            }
        };
        if deleted == 0 {
            return false;
        }
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}

pub(super) fn select_all_objects(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) -> bool {
    {
        let mut state = state.borrow_mut();
        if state.scene.objects.is_empty() {
            return false;
        }

        let selected_ids = state
            .scene
            .objects
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>();
        if let Err(error) = commit_selection_change(&mut state, selected_ids) {
            eprintln!("failed to save select all change: {error:#}");
        }
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}

pub(super) fn clear_selection(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) -> bool {
    {
        let mut state = state.borrow_mut();
        if state.scene.selected_object_ids.is_empty() {
            return false;
        }

        if let Err(error) = commit_selection_change(&mut state, Vec::new()) {
            eprintln!("failed to save clear selection change: {error:#}");
        }
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}

pub(super) fn undo_last_change(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) -> bool {
    {
        let mut state = state.borrow_mut();
        if !state.undo() {
            return false;
        }

        if let Err(error) = persist(&mut state) {
            eprintln!("failed to save undo operation: {error:#}");
        }
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}

pub(super) fn redo_last_change(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
) -> bool {
    {
        let mut state = state.borrow_mut();
        if !state.redo() {
            return false;
        }

        if let Err(error) = persist(&mut state) {
            eprintln!("failed to save redo operation: {error:#}");
        }
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}

pub(super) fn apply_history_action(
    state: &Rc<RefCell<BoardmeshState>>,
    frame: &Frame,
    status_bar: &StatusBar,
    inspector: &InspectorControls,
    history: &HistoryControls,
    canvas: &Panel,
    object_list: &CheckListBox,
    action: HistoryRowAction,
) -> bool {
    {
        let mut state = state.borrow_mut();
        if !apply_history_action_to_state(&mut state, action) {
            return false;
        }

        if let Err(error) = persist(&mut state) {
            eprintln!("failed to save history navigation: {error:#}");
        }
    }
    sync_window_ui(state, frame, status_bar, inspector, history, object_list);
    canvas.refresh(false, None);
    true
}
