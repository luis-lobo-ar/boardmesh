use crate::scene::{SceneColor, SceneDocument, SceneRect};
use mesh_graph::GraphPoint;
use mesh_id::new_prefixed_id;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ViewportState {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
        }
    }
}

impl ViewportState {
    pub const MIN_ZOOM: f64 = 0.1;
    pub const MAX_ZOOM: f64 = 8.0;

    pub fn world_to_screen(&self, point: GraphPoint) -> GraphPoint {
        GraphPoint::new(
            point.x * self.zoom + self.pan_x,
            point.y * self.zoom + self.pan_y,
        )
    }

    pub fn screen_to_world(&self, point: GraphPoint) -> GraphPoint {
        let zoom = self.zoom.max(Self::MIN_ZOOM);
        GraphPoint::new((point.x - self.pan_x) / zoom, (point.y - self.pan_y) / zoom)
    }

    pub fn world_to_screen_canvas_point(
        &self,
        point: crate::scene::CanvasPoint,
    ) -> crate::scene::CanvasPoint {
        let screen = self.world_to_screen(GraphPoint::new(point.x as f64, point.y as f64));
        crate::scene::CanvasPoint::new(screen.x.round() as i32, screen.y.round() as i32)
    }

    pub fn screen_to_world_canvas_point(
        &self,
        point: crate::scene::CanvasPoint,
    ) -> crate::scene::CanvasPoint {
        let world = self.screen_to_world(GraphPoint::new(point.x as f64, point.y as f64));
        crate::scene::CanvasPoint::new(world.x.round() as i32, world.y.round() as i32)
    }

    pub fn pan_by_screen_delta(&mut self, dx: f64, dy: f64) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    pub fn zoom_around_screen_point(&mut self, screen_point: GraphPoint, zoom_factor: f64) {
        if zoom_factor <= 0.0 {
            return;
        }
        let world_focus = self.screen_to_world(screen_point);
        self.zoom = (self.zoom * zoom_factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        self.pan_x = screen_point.x - world_focus.x * self.zoom;
        self.pan_y = screen_point.y - world_focus.y * self.zoom;
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn fit_world_bounds(
        bounds: SceneRect,
        viewport_width: i32,
        viewport_height: i32,
        padding: f64,
    ) -> Option<Self> {
        if bounds.width <= 0 || bounds.height <= 0 || viewport_width <= 0 || viewport_height <= 0 {
            return None;
        }

        let usable_width = (viewport_width as f64 - padding * 2.0).max(1.0);
        let usable_height = (viewport_height as f64 - padding * 2.0).max(1.0);
        let zoom_x = usable_width / bounds.width as f64;
        let zoom_y = usable_height / bounds.height as f64;
        let zoom = zoom_x.min(zoom_y).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);

        let bounds_center = GraphPoint::new(
            bounds.x as f64 + bounds.width as f64 / 2.0,
            bounds.y as f64 + bounds.height as f64 / 2.0,
        );
        let viewport_center =
            GraphPoint::new(viewport_width as f64 / 2.0, viewport_height as f64 / 2.0);

        Some(Self {
            pan_x: viewport_center.x - bounds_center.x * zoom,
            pan_y: viewport_center.y - bounds_center.y * zoom,
            zoom,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CanvasFrame {
    pub id: String,
    pub name: String,
    pub rect: SceneRect,
    pub visible: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CanvasMembership {
    pub object_id: String,
    pub canvas_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct UniverseDocument {
    pub version: u32,
    pub background: SceneColor,
    pub viewport: ViewportState,
    pub active_canvas_id: Option<String>,
    #[serde(default)]
    pub selected_canvas_ids: Vec<String>,
    pub canvases: Vec<CanvasFrame>,
    pub memberships: Vec<CanvasMembership>,
    pub scene: SceneDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct UniverseMigration {
    pub universe: UniverseDocument,
    pub migrated_from_scene: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CanvasExportPlan {
    pub canvas_id: String,
    pub bounds: SceneRect,
    pub object_ids: Vec<String>,
    pub includes_global_objects: bool,
}

impl Default for UniverseDocument {
    fn default() -> Self {
        Self {
            version: 1,
            background: SceneDocument::default().background,
            viewport: ViewportState::default(),
            active_canvas_id: None,
            selected_canvas_ids: Vec::new(),
            canvases: Vec::new(),
            memberships: Vec::new(),
            scene: SceneDocument::default(),
        }
    }
}

impl UniverseDocument {
    pub fn object_canvas_id(&self, object_id: &str) -> Option<&str> {
        self.memberships
            .iter()
            .find(|membership| membership.object_id == object_id)
            .map(|membership| membership.canvas_id.as_str())
    }

    pub fn from_scene(scene: SceneDocument) -> UniverseMigration {
        UniverseMigration {
            universe: UniverseDocument {
                background: scene.background,
                scene,
                ..UniverseDocument::default()
            },
            migrated_from_scene: true,
        }
    }

    pub fn assign_object_to_canvas(&mut self, object_id: &str, canvas_id: &str) -> bool {
        let object_exists = self
            .scene
            .objects
            .iter()
            .any(|object| object.id == object_id);
        let canvas_exists = self.canvases.iter().any(|canvas| canvas.id == canvas_id);
        if !object_exists || !canvas_exists {
            return false;
        }

        if let Some(membership) = self
            .memberships
            .iter_mut()
            .find(|membership| membership.object_id == object_id)
        {
            membership.canvas_id = canvas_id.to_string();
            return true;
        }

        self.memberships.push(CanvasMembership {
            object_id: object_id.to_string(),
            canvas_id: canvas_id.to_string(),
        });
        true
    }

    pub fn clear_object_canvas_assignment(&mut self, object_id: &str) -> bool {
        let len_before = self.memberships.len();
        self.memberships
            .retain(|membership| membership.object_id != object_id);
        len_before != self.memberships.len()
    }

    pub fn create_canvas(&mut self, name: impl Into<String>, rect: SceneRect) -> String {
        let canvas_id = new_prefixed_id("canvas");
        self.canvases.push(CanvasFrame {
            id: canvas_id.clone(),
            name: name.into(),
            rect,
            visible: true,
            locked: false,
        });
        self.active_canvas_id = Some(canvas_id.clone());
        canvas_id
    }

    pub fn rename_canvas(&mut self, canvas_id: &str, name: impl Into<String>) -> bool {
        let Some(canvas) = self
            .canvases
            .iter_mut()
            .find(|canvas| canvas.id == canvas_id)
        else {
            return false;
        };
        canvas.name = name.into();
        true
    }

    pub fn set_active_canvas(&mut self, canvas_id: Option<&str>) -> bool {
        match canvas_id {
            Some(canvas_id) => {
                if !self.canvases.iter().any(|canvas| canvas.id == canvas_id) {
                    return false;
                }
                self.active_canvas_id = Some(canvas_id.to_string());
            }
            None => self.active_canvas_id = None,
        }
        true
    }

    pub fn canvas_ids_in_rect(&self, rect: SceneRect) -> Vec<String> {
        self.canvases
            .iter()
            .filter(|canvas| canvas.rect.intersects(rect))
            .map(|canvas| canvas.id.clone())
            .collect()
    }

    pub fn toggle_canvas_selection(&mut self, canvas_id: &str) -> bool {
        if !self.canvases.iter().any(|canvas| canvas.id == canvas_id) {
            return false;
        }

        if self
            .selected_canvas_ids
            .iter()
            .any(|selected_id| selected_id == canvas_id)
        {
            self.selected_canvas_ids
                .retain(|selected_id| selected_id != canvas_id);
        } else {
            self.selected_canvas_ids.push(canvas_id.to_string());
        }
        true
    }

    pub fn select_canvases(&mut self, canvas_ids: Vec<String>) -> bool {
        if !canvas_ids
            .iter()
            .all(|canvas_id| self.canvases.iter().any(|canvas| canvas.id == *canvas_id))
        {
            return false;
        }

        let mut next_selected = Vec::new();
        for canvas_id in canvas_ids {
            if !next_selected
                .iter()
                .any(|selected_id| selected_id == &canvas_id)
            {
                next_selected.push(canvas_id);
            }
        }
        self.selected_canvas_ids = next_selected;
        true
    }

    pub fn select_canvases_in_rect(&mut self, rect: SceneRect, additive: bool) -> Vec<String> {
        let mut canvas_ids = if additive {
            self.selected_canvas_ids.clone()
        } else {
            Vec::new()
        };
        let additions = self.canvas_ids_in_rect(rect);
        if additive {
            for canvas_id in additions {
                if !canvas_ids
                    .iter()
                    .any(|selected_id| selected_id == &canvas_id)
                {
                    canvas_ids.push(canvas_id);
                }
            }
        } else {
            canvas_ids = additions;
        }
        self.selected_canvas_ids = canvas_ids.clone();
        canvas_ids
    }

    pub fn move_canvas(&mut self, canvas_id: &str, dx: i32, dy: i32, move_members: bool) -> bool {
        let Some(canvas) = self
            .canvases
            .iter_mut()
            .find(|canvas| canvas.id == canvas_id)
        else {
            return false;
        };
        canvas.rect.x += dx;
        canvas.rect.y += dy;

        if move_members {
            let member_ids = self
                .memberships
                .iter()
                .filter(|membership| membership.canvas_id == canvas_id)
                .map(|membership| membership.object_id.clone())
                .collect::<Vec<_>>();
            for object_id in member_ids {
                let _ = self.scene.translate_object(&object_id, dx, dy);
            }
        }
        true
    }

    pub fn export_plan_for_canvas(
        &self,
        canvas_id: &str,
        include_global_objects: bool,
    ) -> Option<CanvasExportPlan> {
        let canvas = self.canvases.iter().find(|canvas| canvas.id == canvas_id)?;
        let mut object_ids = self
            .memberships
            .iter()
            .filter(|membership| membership.canvas_id == canvas_id)
            .map(|membership| membership.object_id.clone())
            .collect::<Vec<_>>();

        if include_global_objects {
            let member_set = object_ids
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>();
            let intersecting_ids = self
                .scene
                .object_ids_in_rect(canvas.rect)
                .into_iter()
                .collect::<std::collections::BTreeSet<_>>();
            object_ids.extend(
                self.scene
                    .objects
                    .iter()
                    .filter(|object| !member_set.contains(&object.id))
                    .filter(|object| intersecting_ids.contains(&object.id))
                    .map(|object| object.id.clone()),
            );
        }

        object_ids.sort();
        object_ids.dedup();

        Some(CanvasExportPlan {
            canvas_id: canvas_id.to_string(),
            bounds: canvas.rect,
            object_ids,
            includes_global_objects: include_global_objects,
        })
    }

    pub fn export_scene_for_canvas(
        &self,
        canvas_id: &str,
        include_global_objects: bool,
    ) -> Option<SceneDocument> {
        let plan = self.export_plan_for_canvas(canvas_id, include_global_objects)?;
        let mut scene = self.scene.clone();
        scene.objects.retain(|object| {
            plan.object_ids
                .iter()
                .any(|object_id| object_id == &object.id)
        });
        scene.selected_object_ids.retain(|object_id| {
            plan.object_ids
                .iter()
                .any(|candidate| candidate == object_id)
        });
        Some(scene)
    }

    pub fn remove_canvas(&mut self, canvas_id: &str) -> bool {
        let len_before = self.canvases.len();
        self.canvases.retain(|canvas| canvas.id != canvas_id);
        let removed = self.canvases.len() != len_before;
        if !removed {
            return false;
        }

        self.memberships
            .retain(|membership| membership.canvas_id != canvas_id);
        self.selected_canvas_ids
            .retain(|selected_id| selected_id != canvas_id);
        if self.active_canvas_id.as_deref() == Some(canvas_id) {
            self.active_canvas_id = None;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{CanvasPoint, SceneStroke};
    use mesh_graph::GraphPoint;

    fn sample_canvas(id: &str, name: &str, rect: SceneRect) -> CanvasFrame {
        CanvasFrame {
            id: id.to_string(),
            name: name.to_string(),
            rect,
            visible: true,
            locked: false,
        }
    }

    #[test]
    fn default_universe_starts_with_empty_canvas_state() {
        let universe = UniverseDocument::default();

        assert_eq!(universe.version, 1);
        assert_eq!(universe.viewport, ViewportState::default());
        assert_eq!(universe.active_canvas_id, None);
        assert!(universe.selected_canvas_ids.is_empty());
        assert!(universe.canvases.is_empty());
        assert!(universe.memberships.is_empty());
        assert_eq!(universe.scene, SceneDocument::default());
    }

    #[test]
    fn migration_from_scene_preserves_existing_scene_and_marks_origin() {
        let mut scene = SceneDocument::default();
        let object_id =
            scene.add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None);
        let _ = scene.select_objects(vec![object_id.clone()]);

        let migration = UniverseDocument::from_scene(scene.clone());

        assert!(migration.migrated_from_scene);
        assert_eq!(migration.universe.scene, scene);
        assert!(migration.universe.canvases.is_empty());
        assert!(migration.universe.memberships.is_empty());
    }

    #[test]
    fn assigning_membership_requires_existing_object_and_canvas() {
        let mut universe = UniverseDocument::default();
        let object_id = universe.scene.add_text(
            crate::scene::TextObject {
                position: CanvasPoint::new(20, 30),
                content: "node".to_string(),
            },
            SceneColor::rgb(1, 2, 3),
        );
        universe.canvases.push(sample_canvas(
            "canvas-main",
            "Main",
            SceneRect::new(0, 0, 400, 300),
        ));

        assert!(universe.assign_object_to_canvas(&object_id, "canvas-main"));
        assert_eq!(
            universe.memberships,
            vec![CanvasMembership {
                object_id,
                canvas_id: "canvas-main".to_string(),
            }]
        );
        assert!(!universe.assign_object_to_canvas("missing-object", "canvas-main"));
        assert!(!universe.assign_object_to_canvas("obj-x", "missing-canvas"));
    }

    #[test]
    fn removing_canvas_clears_memberships_and_active_canvas() {
        let mut universe = UniverseDocument::default();
        let object_id =
            universe
                .scene
                .add_circle(CanvasPoint::new(100, 120), 30, SceneStroke::default(), None);
        universe.canvases.push(sample_canvas(
            "canvas-main",
            "Main",
            SceneRect::new(0, 0, 400, 300),
        ));
        universe.active_canvas_id = Some("canvas-main".to_string());
        assert!(universe.assign_object_to_canvas(&object_id, "canvas-main"));

        assert!(universe.remove_canvas("canvas-main"));
        assert!(universe.canvases.is_empty());
        assert!(universe.memberships.is_empty());
        assert_eq!(universe.active_canvas_id, None);
    }

    #[test]
    fn viewport_round_trips_between_world_and_screen_space() {
        let viewport = ViewportState {
            pan_x: 320.0,
            pan_y: -180.0,
            zoom: 2.5,
        };
        let world = GraphPoint::new(-42.0, 91.0);

        let screen = viewport.world_to_screen(world);
        let restored = viewport.screen_to_world(screen);

        assert!((restored.x - world.x).abs() < f64::EPSILON);
        assert!((restored.y - world.y).abs() < f64::EPSILON);
    }

    #[test]
    fn zooming_around_a_screen_point_keeps_the_same_world_focus() {
        let mut viewport = ViewportState {
            pan_x: 120.0,
            pan_y: 80.0,
            zoom: 1.0,
        };
        let focus_screen = GraphPoint::new(400.0, 240.0);
        let world_before = viewport.screen_to_world(focus_screen);

        viewport.zoom_around_screen_point(focus_screen, 1.5);
        let world_after = viewport.screen_to_world(focus_screen);

        assert!((world_after.x - world_before.x).abs() < 0.000_001);
        assert!((world_after.y - world_before.y).abs() < 0.000_001);
        assert_eq!(viewport.zoom, 1.5);
    }

    #[test]
    fn panning_by_screen_delta_shifts_world_points_by_the_same_screen_amount() {
        let mut viewport = ViewportState {
            pan_x: 20.0,
            pan_y: -10.0,
            zoom: 1.25,
        };
        let world = GraphPoint::new(64.0, 48.0);
        let before = viewport.world_to_screen(world);

        viewport.pan_by_screen_delta(30.0, -15.0);
        let after = viewport.world_to_screen(world);

        assert_eq!(after.x - before.x, 30.0);
        assert_eq!(after.y - before.y, -15.0);
    }

    #[test]
    fn fitting_world_bounds_centers_content_inside_the_viewport() {
        let viewport =
            ViewportState::fit_world_bounds(SceneRect::new(100, 80, 400, 200), 1000, 800, 40.0)
                .expect("fit viewport");

        let top_left = viewport.world_to_screen(GraphPoint::new(100.0, 80.0));
        let bottom_right = viewport.world_to_screen(GraphPoint::new(500.0, 280.0));

        assert!(top_left.x >= 39.0);
        assert!(top_left.y >= 39.0);
        assert!(bottom_right.x <= 961.0);
        assert!(bottom_right.y <= 761.0);
    }

    #[test]
    fn create_activate_and_rename_canvas_updates_canvas_state() {
        let mut universe = UniverseDocument::default();

        let canvas_id = universe.create_canvas("Ideas", SceneRect::new(10, 20, 300, 180));
        assert_eq!(
            universe.active_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );
        assert_eq!(universe.canvases.len(), 1);
        assert_eq!(universe.canvases[0].name, "Ideas");

        assert!(universe.rename_canvas(&canvas_id, "Architecture"));
        assert_eq!(universe.canvases[0].name, "Architecture");

        assert!(universe.set_active_canvas(None));
        assert_eq!(universe.active_canvas_id, None);
        assert!(universe.set_active_canvas(Some(&canvas_id)));
        assert_eq!(
            universe.active_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );
    }

    #[test]
    fn moving_canvas_can_move_member_objects_with_it() {
        let mut universe = UniverseDocument::default();
        let object_id = universe.scene.add_rectangle(
            SceneRect::new(20, 30, 40, 20),
            SceneStroke::default(),
            None,
        );
        let canvas_id = universe.create_canvas("Subsystem", SceneRect::new(0, 0, 200, 120));
        assert!(universe.assign_object_to_canvas(&object_id, &canvas_id));

        assert!(universe.move_canvas(&canvas_id, 15, 25, true));

        let canvas = universe
            .canvases
            .iter()
            .find(|canvas| canvas.id == canvas_id)
            .expect("canvas");
        assert_eq!(canvas.rect, SceneRect::new(15, 25, 200, 120));
        let object = universe
            .scene
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .expect("object");
        assert_eq!(object.bounds(), SceneRect::new(35, 55, 40, 20));
    }

    #[test]
    fn export_plan_for_canvas_distinguishes_canvas_and_global_objects() {
        let mut universe = UniverseDocument::default();
        let canvas_id = universe.create_canvas("Review", SceneRect::new(0, 0, 200, 120));
        let canvas_object = universe.scene.add_rectangle(
            SceneRect::new(20, 20, 40, 30),
            SceneStroke::default(),
            None,
        );
        let global_inside =
            universe
                .scene
                .add_circle(CanvasPoint::new(80, 60), 20, SceneStroke::default(), None);
        let _global_outside = universe.scene.add_text(
            crate::scene::TextObject {
                position: CanvasPoint::new(300, 300),
                content: "outside".to_string(),
            },
            SceneColor::rgb(10, 10, 10),
        );
        assert!(universe.assign_object_to_canvas(&canvas_object, &canvas_id));

        let plan_without_globals = universe
            .export_plan_for_canvas(&canvas_id, false)
            .expect("canvas plan");
        assert_eq!(plan_without_globals.bounds, SceneRect::new(0, 0, 200, 120));
        assert_eq!(plan_without_globals.object_ids, vec![canvas_object.clone()]);
        assert!(!plan_without_globals.includes_global_objects);

        let plan_with_globals = universe
            .export_plan_for_canvas(&canvas_id, true)
            .expect("canvas plan");
        assert_eq!(plan_with_globals.bounds, SceneRect::new(0, 0, 200, 120));
        assert!(plan_with_globals.object_ids.contains(&canvas_object));
        assert!(plan_with_globals.object_ids.contains(&global_inside));
        assert_eq!(plan_with_globals.object_ids.len(), 2);
        assert!(plan_with_globals.includes_global_objects);
    }

    #[test]
    fn export_scene_for_canvas_filters_objects_and_selection() {
        let mut universe = UniverseDocument::default();
        let canvas_id = universe.create_canvas("Review", SceneRect::new(0, 0, 200, 120));
        let canvas_object = universe.scene.add_rectangle(
            SceneRect::new(20, 20, 40, 30),
            SceneStroke::default(),
            None,
        );
        let global_inside =
            universe
                .scene
                .add_circle(CanvasPoint::new(80, 60), 20, SceneStroke::default(), None);
        let global_outside = universe.scene.add_text(
            crate::scene::TextObject {
                position: CanvasPoint::new(300, 300),
                content: "outside".to_string(),
            },
            SceneColor::rgb(10, 10, 10),
        );
        assert!(universe.assign_object_to_canvas(&canvas_object, &canvas_id));
        assert!(
            universe
                .scene
                .select_objects(vec![canvas_object.clone(), global_outside.clone()])
        );

        let exported = universe
            .export_scene_for_canvas(&canvas_id, true)
            .expect("canvas scene");

        assert_eq!(exported.object_count(), 2);
        assert!(
            exported
                .objects
                .iter()
                .any(|object| object.id == canvas_object)
        );
        assert!(
            exported
                .objects
                .iter()
                .any(|object| object.id == global_inside)
        );
        assert!(
            !exported
                .objects
                .iter()
                .any(|object| object.id == global_outside)
        );
        assert_eq!(exported.selected_object_ids, vec![canvas_object]);
    }

    #[test]
    fn universe_canvas_state_round_trips_through_json() {
        let mut universe = UniverseDocument::default();
        let canvas_id = universe.create_canvas("Main", SceneRect::new(10, 10, 320, 180));
        let object_id = universe.scene.add_rectangle(
            SceneRect::new(20, 30, 40, 50),
            SceneStroke::default(),
            None,
        );
        assert!(universe.assign_object_to_canvas(&object_id, &canvas_id));

        let json = serde_json::to_string(&universe).expect("serialize universe");
        let restored =
            serde_json::from_str::<UniverseDocument>(&json).expect("deserialize universe");

        assert_eq!(
            restored.active_canvas_id.as_deref(),
            Some(canvas_id.as_str())
        );
        assert_eq!(restored.canvases.len(), 1);
        assert_eq!(restored.memberships.len(), 1);
        assert_eq!(restored.memberships[0].object_id, object_id);
    }

    #[test]
    fn canvas_selection_supports_toggle_and_box_select() {
        let mut universe = UniverseDocument::default();
        let first_id = universe.create_canvas("One", SceneRect::new(0, 0, 120, 80));
        let second_id = universe.create_canvas("Two", SceneRect::new(200, 0, 120, 80));

        assert!(universe.toggle_canvas_selection(&first_id));
        assert_eq!(universe.selected_canvas_ids, vec![first_id.clone()]);

        let replaced = universe.select_canvases_in_rect(SceneRect::new(180, 0, 200, 120), false);
        assert_eq!(replaced, vec![second_id.clone()]);
        assert_eq!(universe.selected_canvas_ids, replaced);

        let additive = universe.select_canvases_in_rect(SceneRect::new(0, 0, 400, 120), true);
        assert_eq!(additive, vec![second_id.clone(), first_id.clone()]);
        assert_eq!(universe.selected_canvas_ids, additive);

        assert!(universe.toggle_canvas_selection(&second_id));
        assert_eq!(universe.selected_canvas_ids, vec![first_id]);
    }

    #[test]
    fn canvas_selection_can_be_replaced_explicitly() {
        let mut universe = UniverseDocument::default();
        let first_id = universe.create_canvas("One", SceneRect::new(0, 0, 120, 80));
        let second_id = universe.create_canvas("Two", SceneRect::new(200, 0, 120, 80));

        assert!(universe.select_canvases(vec![first_id.clone(), second_id.clone()]));
        assert_eq!(
            universe.selected_canvas_ids,
            vec![first_id.clone(), second_id.clone()]
        );

        assert!(!universe.select_canvases(vec!["missing".to_string()]));
        assert_eq!(universe.selected_canvas_ids, vec![first_id, second_id]);
    }

    #[test]
    fn object_canvas_assignment_can_be_replaced_and_cleared() {
        let mut universe = UniverseDocument::default();
        let first_canvas = universe.create_canvas("One", SceneRect::new(0, 0, 120, 80));
        let second_canvas = universe.create_canvas("Two", SceneRect::new(200, 0, 120, 80));
        let object_id = universe.scene.add_rectangle(
            SceneRect::new(10, 10, 40, 30),
            SceneStroke::default(),
            None,
        );

        assert!(universe.assign_object_to_canvas(&object_id, &first_canvas));
        assert_eq!(
            universe.object_canvas_id(&object_id),
            Some(first_canvas.as_str())
        );

        assert!(universe.assign_object_to_canvas(&object_id, &second_canvas));
        assert_eq!(
            universe.object_canvas_id(&object_id),
            Some(second_canvas.as_str())
        );

        assert!(universe.clear_object_canvas_assignment(&object_id));
        assert_eq!(universe.object_canvas_id(&object_id), None);
    }
}
