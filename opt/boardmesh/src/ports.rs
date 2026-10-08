use crate::runtime::RuntimeSceneSnapshot;
use crate::scene::SceneDocument;
use crate::session::{
    RUNTIME_API_VERSION, SESSION_PROTOCOL_VERSION, SessionManifest,
    create_named_session_from_scene, load_session_manifest, save_session_manifest,
    session_manifest_path,
};
use crate::storage::{
    ensure_scene_file, list_session_ids, load_scene, resolve_session_path, save_scene,
    session_id_from_path,
};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub trait WorkspaceStoragePort: Send + Sync {
    fn ensure_scene_file(&self, path: &Path) -> Result<SceneDocument>;
    fn load_scene(&self, path: &Path) -> Result<SceneDocument>;
    fn save_scene(&self, path: &Path, scene: &SceneDocument) -> Result<()>;
    fn resolve_session_path(&self, session_root: &Path, session_id: &str) -> Result<PathBuf>;
    fn session_id_from_path(&self, session_root: &Path, scene_path: &Path) -> Option<String>;
    fn list_session_ids(&self, session_root: &Path) -> Result<Vec<String>>;
    fn load_session_manifest(&self, path: &Path) -> Result<SessionManifest>;
    fn save_session_manifest(&self, path: &Path, manifest: &SessionManifest) -> Result<()>;
    fn session_manifest_path(&self, session_root: &Path, session_id: &str) -> PathBuf;
    fn create_named_session_from_scene(
        &self,
        session_root: &Path,
        session_id: &str,
        scene: &SceneDocument,
    ) -> Result<(PathBuf, SessionManifest)>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceEventKind {
    SceneChanged,
    SessionActivated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEvent {
    pub kind: WorkspaceEventKind,
    pub session_id: Option<String>,
    pub scene_path: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub scene: SceneDocument,
}

impl WorkspaceEvent {
    pub fn runtime_snapshot(&self) -> RuntimeSceneSnapshot {
        RuntimeSceneSnapshot {
            session_id: self.session_id.clone(),
            scene_path: self.scene_path.display().to_string(),
            manifest_path: self
                .manifest_path
                .as_ref()
                .map(|path| path.display().to_string()),
            scene: self.scene.clone(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            runtime_api_version: RUNTIME_API_VERSION,
        }
    }
}

pub trait WorkspaceEventPublisher: Send + Sync {
    fn publish(&self, event: WorkspaceEvent) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FileSystemWorkspaceStorage;

impl WorkspaceStoragePort for FileSystemWorkspaceStorage {
    fn ensure_scene_file(&self, path: &Path) -> Result<SceneDocument> {
        ensure_scene_file(path)
    }

    fn load_scene(&self, path: &Path) -> Result<SceneDocument> {
        load_scene(path)
    }

    fn save_scene(&self, path: &Path, scene: &SceneDocument) -> Result<()> {
        save_scene(path, scene)
    }

    fn resolve_session_path(&self, session_root: &Path, session_id: &str) -> Result<PathBuf> {
        resolve_session_path(session_root, session_id)
    }

    fn session_id_from_path(&self, session_root: &Path, scene_path: &Path) -> Option<String> {
        session_id_from_path(session_root, scene_path)
    }

    fn list_session_ids(&self, session_root: &Path) -> Result<Vec<String>> {
        list_session_ids(session_root)
    }

    fn load_session_manifest(&self, path: &Path) -> Result<SessionManifest> {
        load_session_manifest(path)
    }

    fn save_session_manifest(&self, path: &Path, manifest: &SessionManifest) -> Result<()> {
        save_session_manifest(path, manifest)
    }

    fn session_manifest_path(&self, session_root: &Path, session_id: &str) -> PathBuf {
        session_manifest_path(session_root, session_id)
    }

    fn create_named_session_from_scene(
        &self,
        session_root: &Path,
        session_id: &str,
        scene: &SceneDocument,
    ) -> Result<(PathBuf, SessionManifest)> {
        create_named_session_from_scene(session_root, session_id, scene)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopWorkspaceEventPublisher;

impl WorkspaceEventPublisher for NoopWorkspaceEventPublisher {
    fn publish(&self, _event: WorkspaceEvent) -> Result<()> {
        Ok(())
    }
}
