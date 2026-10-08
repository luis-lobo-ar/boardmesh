use crate::ports::{
    FileSystemWorkspaceStorage, NoopWorkspaceEventPublisher, WorkspaceEvent, WorkspaceEventKind,
    WorkspaceEventPublisher, WorkspaceStoragePort,
};
use crate::runtime::RuntimeSceneSnapshot;
use crate::scene::{
    CanvasPoint, ConnectorSpec, LinkObject, SceneColor, SceneDocument, SceneRect, SceneStroke,
    TextObject, TriangleObject,
};
use crate::session::SessionManifest;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct BoardWorkspace {
    session_root: PathBuf,
    active_session_id: Option<String>,
    scene_path: PathBuf,
    scene: SceneDocument,
    storage: Arc<dyn WorkspaceStoragePort>,
    events: Arc<dyn WorkspaceEventPublisher>,
}

impl BoardWorkspace {
    pub fn new(
        session_root: PathBuf,
        active_session_id: Option<String>,
        scene_path: PathBuf,
        scene: SceneDocument,
    ) -> Self {
        Self::with_ports(
            session_root,
            active_session_id,
            scene_path,
            scene,
            Arc::new(FileSystemWorkspaceStorage),
            Arc::new(NoopWorkspaceEventPublisher),
        )
    }

    pub fn with_ports(
        session_root: PathBuf,
        active_session_id: Option<String>,
        scene_path: PathBuf,
        scene: SceneDocument,
        storage: Arc<dyn WorkspaceStoragePort>,
        events: Arc<dyn WorkspaceEventPublisher>,
    ) -> Self {
        Self {
            session_root,
            active_session_id,
            scene_path,
            scene,
            storage,
            events,
        }
    }

    pub fn session_root(&self) -> &Path {
        &self.session_root
    }

    pub fn active_session_id(&self) -> Option<&str> {
        self.active_session_id.as_deref()
    }

    pub fn active_session_id_owned(&self) -> Option<String> {
        self.active_session_id.clone()
    }

    pub fn scene_path(&self) -> &Path {
        &self.scene_path
    }

    pub fn scene_path_string(&self) -> String {
        self.scene_path.display().to_string()
    }

    pub fn scene(&self) -> &SceneDocument {
        &self.scene
    }

    pub fn scene_clone(&self) -> SceneDocument {
        self.scene.clone()
    }

    pub fn refresh_from_disk(&mut self) -> Result<()> {
        self.scene = self.storage.load_scene(&self.scene_path)?;
        Ok(())
    }

    pub fn persist(&self) -> Result<()> {
        self.storage.save_scene(&self.scene_path, &self.scene)
    }

    pub fn ensure_named_session(&self, session_id: &str) -> Result<(PathBuf, SessionManifest)> {
        let scene_path = self
            .storage
            .resolve_session_path(&self.session_root, session_id)?;
        let _ = self.storage.ensure_scene_file(&scene_path)?;

        let manifest_path = self.manifest_path_for(session_id);
        let mut manifest = if manifest_path.exists() {
            self.storage.load_session_manifest(&manifest_path)?
        } else {
            SessionManifest::new(session_id.to_string(), scene_path.clone())
        };
        manifest.scene_path = scene_path.display().to_string();
        manifest.touch();
        self.storage
            .save_session_manifest(&manifest_path, &manifest)?;
        Ok((scene_path, manifest))
    }

    pub fn activate_session(
        &mut self,
        session_id: Option<String>,
        scene_path: PathBuf,
    ) -> Result<()> {
        let scene = self.storage.ensure_scene_file(&scene_path)?;
        self.scene = scene;
        self.scene_path = scene_path.clone();

        let inferred_session_id = session_id
            .or_else(|| {
                self.storage
                    .session_id_from_path(&self.session_root, &scene_path)
            })
            .filter(|candidate| {
                self.manifest_path_for(candidate).exists()
                    || self
                        .storage
                        .resolve_session_path(&self.session_root, candidate)
                        .map(|resolved| resolved == scene_path)
                        .unwrap_or(false)
            });

        if let Some(session_id) = inferred_session_id.as_deref() {
            let _ = self.ensure_named_session(session_id)?;
        }
        self.active_session_id = inferred_session_id;
        self.publish(WorkspaceEventKind::SessionActivated)?;
        Ok(())
    }

    pub fn runtime_snapshot_for_scene(&self, scene: SceneDocument) -> RuntimeSceneSnapshot {
        self.event_for(WorkspaceEventKind::SceneChanged, scene)
            .runtime_snapshot()
    }

    pub fn current_runtime_snapshot(&self) -> RuntimeSceneSnapshot {
        self.runtime_snapshot_for_scene(self.scene_clone())
    }

    pub fn runtime_session_list(&self) -> Result<crate::runtime::SessionListResult> {
        let sessions = self
            .storage
            .list_session_ids(&self.session_root)?
            .into_iter()
            .map(|session_id| {
                let scene_path = self
                    .storage
                    .resolve_session_path(&self.session_root, &session_id)?;
                Ok(crate::runtime::SessionListEntry {
                    session_id,
                    scene_path: scene_path.display().to_string(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(crate::runtime::SessionListResult {
            active_session_id: self.active_session_id.clone(),
            scene_path: self.scene_path_string(),
            sessions_dir: self.session_root.display().to_string(),
            sessions,
        })
    }

    pub fn create_named_session_from_current_scene(
        &self,
        session_id: &str,
    ) -> Result<(PathBuf, SessionManifest)> {
        self.storage
            .create_named_session_from_scene(&self.session_root, session_id, &self.scene)
    }

    pub fn load_scene_json(
        &mut self,
        scene: SceneDocument,
        target_path: PathBuf,
        make_active: bool,
    ) -> Result<()> {
        self.storage.save_scene(&target_path, &scene)?;
        self.scene = scene;
        if make_active || target_path == self.scene_path {
            self.activate_session(None, target_path)?;
        } else {
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(())
    }

    pub fn save_scene_as(&mut self, target_path: PathBuf, make_active: bool) -> Result<()> {
        self.storage.save_scene(&target_path, &self.scene)?;
        if make_active {
            self.activate_session(None, target_path)?;
        }
        Ok(())
    }

    pub fn switch_scene(&mut self, target_path: PathBuf, create_if_missing: bool) -> Result<()> {
        let scene = if create_if_missing {
            self.storage.ensure_scene_file(&target_path)?
        } else {
            self.storage.load_scene(&target_path)?
        };
        self.scene = scene;
        self.activate_session(None, target_path)?;
        Ok(())
    }

    pub fn replace_scene(&mut self, scene: SceneDocument) -> Result<RuntimeSceneSnapshot> {
        self.scene = scene;
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(self.current_runtime_snapshot())
    }

    pub fn clear_scene(&mut self) -> Result<()> {
        self.scene.clear();
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)
    }

    pub fn add_line(
        &mut self,
        start: CanvasPoint,
        end: CanvasPoint,
        stroke: SceneStroke,
    ) -> Result<String> {
        let object_id = self.scene.add_line(start, end, stroke);
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn add_connector(
        &mut self,
        connector: ConnectorSpec,
        stroke: SceneStroke,
    ) -> Result<String> {
        let object_id = self.scene.add_connector(connector, stroke);
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn add_link(&mut self, link: LinkObject, stroke: SceneStroke) -> Result<String> {
        let object_id = self.scene.add_link(link, stroke);
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn add_rectangle(
        &mut self,
        rect: SceneRect,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> Result<String> {
        let object_id = self.scene.add_rectangle(rect, stroke, fill);
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn add_circle(
        &mut self,
        center: CanvasPoint,
        radius: i32,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> Result<String> {
        let object_id = self.scene.add_circle(center, radius.max(1), stroke, fill);
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn add_triangle(
        &mut self,
        triangle: TriangleObject,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> Result<String> {
        let object_id = self.scene.add_triangle(triangle, stroke, fill);
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn add_polygon(
        &mut self,
        points: Vec<CanvasPoint>,
        stroke: SceneStroke,
        fill: Option<SceneColor>,
    ) -> Result<Option<String>> {
        let object_id = self.scene.add_polygon(points, stroke, fill);
        if let Some(object_id) = object_id.as_ref() {
            let _ = self.scene.select_objects(vec![object_id.clone()]);
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(object_id)
    }

    pub fn add_stroke(
        &mut self,
        points: Vec<CanvasPoint>,
        stroke: SceneStroke,
    ) -> Result<Option<String>> {
        let object_id = self.scene.add_stroke(points, stroke);
        if let Some(object_id) = object_id.as_ref() {
            let _ = self.scene.select_objects(vec![object_id.clone()]);
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(object_id)
    }

    pub fn add_text(
        &mut self,
        position: CanvasPoint,
        text: String,
        color: SceneColor,
    ) -> Result<String> {
        let object_id = self.scene.add_text(
            TextObject {
                position,
                content: text,
            },
            color,
        );
        let _ = self.scene.select_objects(vec![object_id.clone()]);
        self.persist()?;
        self.publish(WorkspaceEventKind::SceneChanged)?;
        Ok(object_id)
    }

    pub fn fill_object(&mut self, object_id: &str, fill: SceneColor) -> Result<bool> {
        let updated = self.scene.fill_object(object_id, fill);
        if updated {
            let _ = self.scene.select_objects(vec![object_id.to_string()]);
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn duplicate_object(
        &mut self,
        object_id: &str,
        dx: i32,
        dy: i32,
    ) -> Result<Option<String>> {
        let duplicate_id = self.scene.duplicate_object(object_id, dx, dy);
        if let Some(duplicate_id) = duplicate_id.as_ref() {
            let _ = self.scene.select_objects(vec![duplicate_id.clone()]);
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(duplicate_id)
    }

    pub fn set_object_visibility(&mut self, object_id: &str, visible: bool) -> Result<bool> {
        let updated = self.scene.set_object_visibility(object_id, visible);
        if updated {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn set_object_locked(&mut self, object_id: &str, locked: bool) -> Result<bool> {
        let updated = self.scene.set_object_locked(object_id, locked);
        if updated {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn move_object_to_front(&mut self, object_id: &str) -> Result<bool> {
        let updated = self.scene.move_object_to_front(object_id);
        if updated {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn move_object_to_back(&mut self, object_id: &str) -> Result<bool> {
        let updated = self.scene.move_object_to_back(object_id);
        if updated {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn move_object_forward(&mut self, object_id: &str) -> Result<bool> {
        let updated = self.scene.move_object_forward(object_id);
        if updated {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn move_object_backward(&mut self, object_id: &str) -> Result<bool> {
        let updated = self.scene.move_object_backward(object_id);
        if updated {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(updated)
    }

    pub fn delete_object(&mut self, object_id: &str) -> Result<bool> {
        let deleted = self.scene.delete_object(object_id);
        if deleted {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(deleted)
    }

    pub fn delete_objects(&mut self, object_ids: &[String]) -> Result<usize> {
        let deleted = self.scene.delete_objects(object_ids);
        if deleted > 0 {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(deleted)
    }

    pub fn select_objects(&mut self, object_ids: Vec<String>) -> Result<bool> {
        let selected = self.scene.select_objects(object_ids);
        if selected {
            self.persist()?;
            self.publish(WorkspaceEventKind::SceneChanged)?;
        }
        Ok(selected)
    }

    fn manifest_path_for(&self, session_id: &str) -> PathBuf {
        self.storage
            .session_manifest_path(&self.session_root, session_id)
    }

    fn event_for(&self, kind: WorkspaceEventKind, scene: SceneDocument) -> WorkspaceEvent {
        let manifest_path = self
            .active_session_id
            .as_ref()
            .map(|session_id| self.manifest_path_for(session_id));
        WorkspaceEvent {
            kind,
            session_id: self.active_session_id.clone(),
            scene_path: self.scene_path.clone(),
            manifest_path,
            scene,
        }
    }

    fn publish(&self, kind: WorkspaceEventKind) -> Result<()> {
        self.events
            .publish(self.event_for(kind, self.scene_clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::WorkspaceEventPublisher;
    use crate::scene::{
        ConnectorAnchor, ConnectorEndpoint, LinkEnd, LinkRoutingPreference, SceneConnectorKind,
    };
    use crate::session_manifest_path;
    use crate::storage::{ensure_scene_file, load_scene, resolve_session_path};
    use std::sync::{Arc, Mutex};
    use uuid::Uuid;

    #[derive(Default)]
    struct RecordingPublisher {
        events: Mutex<Vec<WorkspaceEvent>>,
    }

    impl WorkspaceEventPublisher for RecordingPublisher {
        fn publish(&self, event: WorkspaceEvent) -> Result<()> {
            self.events.lock().expect("events lock").push(event);
            Ok(())
        }
    }

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("boardmesh-app-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("temp root");
        root
    }

    fn sample_workspace() -> BoardWorkspace {
        let root = temp_root();
        let scene_path = root.join("default.json");
        let scene = ensure_scene_file(&scene_path).expect("default scene");
        BoardWorkspace::new(root, None, scene_path, scene)
    }

    #[test]
    fn add_text_persists_scene_and_selects_new_object() {
        let mut workspace = sample_workspace();

        let object_id = workspace
            .add_text(
                CanvasPoint::new(120, 80),
                "hello".to_string(),
                SceneColor::rgb(1, 2, 3),
            )
            .expect("add text");

        assert_eq!(workspace.scene().object_count(), 1);
        assert_eq!(
            workspace.scene().selected_object_ids,
            vec![object_id.clone()]
        );

        let persisted = load_scene(workspace.scene_path()).expect("persisted scene");
        assert_eq!(persisted.object_count(), 1);
        assert_eq!(persisted.selected_object_ids, vec![object_id]);
    }

    #[test]
    fn add_text_publishes_scene_changed_event() {
        let root = temp_root();
        let scene_path = root.join("default.json");
        let scene = ensure_scene_file(&scene_path).expect("default scene");
        let publisher = Arc::new(RecordingPublisher::default());
        let mut workspace = BoardWorkspace::with_ports(
            root,
            None,
            scene_path,
            scene,
            Arc::new(FileSystemWorkspaceStorage),
            publisher.clone(),
        );

        let _ = workspace
            .add_text(
                CanvasPoint::new(12, 18),
                "event".to_string(),
                SceneColor::rgb(5, 6, 7),
            )
            .expect("add text");

        let events = publisher.events.lock().expect("events lock");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, WorkspaceEventKind::SceneChanged);
        assert_eq!(events[0].scene.object_count(), 1);
    }

    #[test]
    fn clear_scene_removes_objects_from_memory_and_disk() {
        let mut workspace = sample_workspace();
        let _ = workspace
            .add_text(
                CanvasPoint::new(24, 32),
                "before".to_string(),
                SceneColor::rgb(9, 8, 7),
            )
            .expect("seed scene");

        workspace.clear_scene().expect("clear scene");

        assert_eq!(workspace.scene().object_count(), 0);
        let persisted = load_scene(workspace.scene_path()).expect("persisted scene");
        assert_eq!(persisted.object_count(), 0);
    }

    #[test]
    fn add_connector_persists_scene_and_selects_new_object() {
        let mut workspace = sample_workspace();
        let source_id = workspace
            .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
            .expect("source rectangle");
        let target_id = workspace
            .add_rectangle(
                SceneRect::new(160, 40, 60, 50),
                SceneStroke::default(),
                None,
            )
            .expect("target rectangle");

        let object_id = workspace
            .add_connector(
                ConnectorSpec {
                    start: ConnectorEndpoint::Bound {
                        object_id: source_id,
                        anchor: ConnectorAnchor::East,
                        offset: CanvasPoint::new(0, 0),
                    },
                    end: ConnectorEndpoint::Bound {
                        object_id: target_id,
                        anchor: ConnectorAnchor::West,
                        offset: CanvasPoint::new(0, 0),
                    },
                    kind: SceneConnectorKind::Straight,
                },
                SceneStroke::default(),
            )
            .expect("add connector");

        assert_eq!(
            workspace.scene().selected_object_ids,
            vec![object_id.clone()]
        );

        let persisted = load_scene(workspace.scene_path()).expect("persisted scene");
        assert_eq!(persisted.object_count(), 3);
        assert_eq!(persisted.selected_object_ids, vec![object_id.clone()]);
        assert!(
            persisted
                .objects
                .iter()
                .any(|object| object.id == object_id)
        );
    }

    #[test]
    fn add_link_persists_scene_and_selects_new_object() {
        let mut workspace = sample_workspace();
        let source_id = workspace
            .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
            .expect("source rectangle");
        let target_id = workspace
            .add_rectangle(
                SceneRect::new(160, 40, 60, 50),
                SceneStroke::default(),
                None,
            )
            .expect("target rectangle");

        let object_id = workspace
            .add_link(
                LinkObject {
                    source: LinkEnd {
                        endpoint: ConnectorEndpoint::Bound {
                            object_id: source_id,
                            anchor: ConnectorAnchor::East,
                            offset: CanvasPoint::new(0, 0),
                        },
                        label: Some("source".to_string()),
                    },
                    target: LinkEnd {
                        endpoint: ConnectorEndpoint::Bound {
                            object_id: target_id,
                            anchor: ConnectorAnchor::West,
                            offset: CanvasPoint::new(0, 0),
                        },
                        label: Some("target".to_string()),
                    },
                    routing: LinkRoutingPreference::Spline,
                    label: Some("references".to_string()),
                },
                SceneStroke::default(),
            )
            .expect("add link");

        assert_eq!(
            workspace.scene().selected_object_ids,
            vec![object_id.clone()]
        );

        let persisted = load_scene(workspace.scene_path()).expect("persisted scene");
        assert_eq!(persisted.object_count(), 3);
        assert_eq!(persisted.selected_object_ids, vec![object_id.clone()]);
        assert!(
            persisted
                .objects
                .iter()
                .any(|object| object.id == object_id)
        );
    }

    #[test]
    fn activate_session_infers_named_session_and_creates_manifest() {
        let mut workspace = sample_workspace();
        let session_id = "review-board";
        let session_path =
            resolve_session_path(workspace.session_root(), session_id).expect("session path");

        workspace
            .activate_session(Some(session_id.to_string()), session_path.clone())
            .expect("activate session");

        assert_eq!(workspace.active_session_id(), Some(session_id));
        assert_eq!(workspace.scene_path(), session_path.as_path());
        assert!(session_manifest_path(workspace.session_root(), session_id).exists());
    }

    #[test]
    fn duplicate_object_persists_scene_and_selects_duplicate() {
        let mut workspace = sample_workspace();
        let object_id = workspace
            .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
            .expect("seed rectangle");

        let duplicate_id = workspace
            .duplicate_object(&object_id, 24, 16)
            .expect("duplicate result")
            .expect("duplicate id");

        assert_eq!(
            workspace.scene().selected_object_ids,
            vec![duplicate_id.clone()]
        );
        let duplicate = workspace
            .scene()
            .objects
            .iter()
            .find(|object| object.id == duplicate_id)
            .expect("duplicate object");
        assert_eq!(duplicate.bounds(), SceneRect::new(34, 36, 40, 30));
    }

    #[test]
    fn set_object_visibility_persists_updated_visibility() {
        let mut workspace = sample_workspace();
        let object_id = workspace
            .add_rectangle(SceneRect::new(10, 20, 40, 30), SceneStroke::default(), None)
            .expect("seed rectangle");

        assert!(
            workspace
                .set_object_visibility(&object_id, false)
                .expect("visibility update")
        );

        let persisted = load_scene(workspace.scene_path()).expect("persisted scene");
        let object = persisted
            .objects
            .iter()
            .find(|item| item.id == object_id)
            .expect("persisted object");
        assert!(!object.visible);
    }
}
