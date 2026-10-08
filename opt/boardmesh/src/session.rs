use crate::scene::SceneDocument;
use crate::storage::{resolve_session_path, save_scene};
use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SESSION_PROTOCOL_VERSION: u32 = 1;
pub const RUNTIME_API_VERSION: u32 = 1;
pub const SERVER_HEARTBEAT_STALE_MS: u64 = 5_000;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default)]
    pub iconized: bool,
    #[serde(default)]
    pub desktop_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AttachedWindow {
    pub window_id: String,
    pub app_version: String,
    pub protocol_version: u32,
    #[serde(default)]
    pub pid: Option<u32>,
    pub last_seen_ms: u64,
    #[serde(default)]
    pub window_state: Option<WindowState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AttachedServer {
    pub server_id: String,
    pub app_version: String,
    pub protocol_version: u32,
    pub runtime_api_version: u32,
    #[serde(default)]
    pub pid: Option<u32>,
    pub last_seen_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct SessionManifest {
    pub session_id: String,
    pub protocol_version: u32,
    pub app_version: String,
    pub scene_path: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    #[serde(default)]
    pub attached_windows: Vec<AttachedWindow>,
    #[serde(default)]
    pub attached_servers: Vec<AttachedServer>,
}

impl SessionManifest {
    pub fn new(session_id: String, scene_path: PathBuf) -> Self {
        let now = unix_time_ms();
        Self {
            session_id,
            protocol_version: SESSION_PROTOCOL_VERSION,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            scene_path: scene_path.display().to_string(),
            created_at_ms: now,
            updated_at_ms: now,
            attached_windows: Vec::new(),
            attached_servers: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at_ms = unix_time_ms();
    }

    pub fn ensure_compatible(&self) -> Result<()> {
        if self.protocol_version != SESSION_PROTOCOL_VERSION {
            bail!(
                "session protocol mismatch: manifest has {}, current binary expects {}",
                self.protocol_version,
                SESSION_PROTOCOL_VERSION
            );
        }
        Ok(())
    }

    pub fn update_window(&mut self, window: AttachedWindow) {
        if let Some(existing) = self
            .attached_windows
            .iter_mut()
            .find(|existing| existing.window_id == window.window_id)
        {
            *existing = window;
        } else {
            self.attached_windows.push(window);
        }
        self.touch();
    }

    pub fn remove_window(&mut self, window_id: &str) {
        let original_len = self.attached_windows.len();
        self.attached_windows
            .retain(|window| window.window_id != window_id);
        if self.attached_windows.len() != original_len {
            self.touch();
        }
    }

    pub fn update_server(&mut self, server: AttachedServer) {
        if let Some(existing) = self
            .attached_servers
            .iter_mut()
            .find(|existing| existing.server_id == server.server_id)
        {
            *existing = server;
        } else {
            self.attached_servers.push(server);
        }
        self.touch();
    }

    pub fn latest_server(&self) -> Option<&AttachedServer> {
        self.attached_servers
            .iter()
            .max_by_key(|server| server.last_seen_ms)
    }
}

pub fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn session_manifest_path(session_root: &Path, session_id: &str) -> PathBuf {
    session_root.join(format!("{session_id}.manifest.json"))
}

pub fn load_session_manifest(path: &Path) -> Result<SessionManifest> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session manifest {}", path.display()))?;
    let manifest = serde_json::from_str::<SessionManifest>(&contents)
        .with_context(|| format!("failed to parse session manifest {}", path.display()))?;
    manifest.ensure_compatible()?;
    Ok(manifest)
}

pub fn save_session_manifest(path: &Path, manifest: &SessionManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create session directory {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("tmp");
    let json =
        serde_json::to_vec_pretty(manifest).context("failed to serialize session manifest")?;
    std::fs::write(&tmp_path, json)
        .with_context(|| format!("failed to write temporary manifest {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to replace manifest {}", path.display()))?;
    Ok(())
}

pub fn create_named_session_from_scene(
    session_root: &Path,
    session_id: &str,
    scene: &SceneDocument,
) -> Result<(PathBuf, SessionManifest)> {
    let scene_path = resolve_session_path(session_root, session_id)?;
    let manifest_path = session_manifest_path(session_root, session_id);
    anyhow::ensure!(
        !manifest_path.exists(),
        "session '{session_id}' already exists"
    );

    save_scene(&scene_path, scene)?;
    let manifest = SessionManifest::new(session_id.to_string(), scene_path.clone());
    save_session_manifest(&manifest_path, &manifest)?;
    Ok((scene_path, manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{SceneColor, SceneDocument, SceneRect};
    use crate::storage::load_scene;
    use std::fs;

    fn temp_session_root(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "boardmesh-session-tests-{name}-{}-{}",
            std::process::id(),
            unix_time_ms()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn session_manifest_tracks_window_updates_by_id() {
        let mut manifest =
            SessionManifest::new("alpha".to_string(), PathBuf::from("/tmp/alpha.json"));
        manifest.update_window(AttachedWindow {
            window_id: "win-1".to_string(),
            app_version: "0.2.0".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            pid: Some(10),
            last_seen_ms: 1,
            window_state: Some(WindowState {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
                maximized: false,
                iconized: false,
                desktop_id: None,
            }),
        });
        manifest.update_window(AttachedWindow {
            window_id: "win-1".to_string(),
            app_version: "0.2.0".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            pid: Some(11),
            last_seen_ms: 2,
            window_state: None,
        });

        assert_eq!(manifest.attached_windows.len(), 1);
        assert_eq!(manifest.attached_windows[0].pid, Some(11));
    }

    #[test]
    fn incompatible_protocol_is_rejected() {
        let manifest = SessionManifest {
            session_id: "alpha".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION + 1,
            app_version: "0.2.0".to_string(),
            scene_path: "/tmp/alpha.json".to_string(),
            created_at_ms: 0,
            updated_at_ms: 0,
            attached_windows: Vec::new(),
            attached_servers: Vec::new(),
        };
        assert!(manifest.ensure_compatible().is_err());
    }

    #[test]
    fn remove_window_drops_matching_entry() {
        let mut manifest =
            SessionManifest::new("alpha".to_string(), PathBuf::from("/tmp/alpha.json"));
        manifest.update_window(AttachedWindow {
            window_id: "win-1".to_string(),
            app_version: "0.2.0".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            pid: Some(10),
            last_seen_ms: 1,
            window_state: None,
        });
        manifest.update_window(AttachedWindow {
            window_id: "win-2".to_string(),
            app_version: "0.2.0".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            pid: Some(20),
            last_seen_ms: 2,
            window_state: None,
        });

        manifest.remove_window("win-1");

        assert_eq!(manifest.attached_windows.len(), 1);
        assert_eq!(manifest.attached_windows[0].window_id, "win-2");
    }

    #[test]
    fn session_manifest_tracks_server_updates_by_id() {
        let mut manifest =
            SessionManifest::new("alpha".to_string(), PathBuf::from("/tmp/alpha.json"));
        manifest.update_server(AttachedServer {
            server_id: "mcp-1".to_string(),
            app_version: "0.2.0".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            runtime_api_version: RUNTIME_API_VERSION,
            pid: Some(44),
            last_seen_ms: 1,
        });
        manifest.update_server(AttachedServer {
            server_id: "mcp-1".to_string(),
            app_version: "0.2.1".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            runtime_api_version: RUNTIME_API_VERSION,
            pid: Some(55),
            last_seen_ms: 2,
        });

        assert_eq!(manifest.attached_servers.len(), 1);
        assert_eq!(manifest.attached_servers[0].pid, Some(55));
        assert_eq!(manifest.latest_server().unwrap().app_version, "0.2.1");
    }

    #[test]
    fn create_named_session_from_scene_writes_scene_and_manifest() {
        let session_root = temp_session_root("create");
        let mut scene = SceneDocument::default();
        scene.add_rectangle(
            SceneRect::new(10, 20, 30, 40),
            Default::default(),
            Some(SceneColor::rgb(255, 200, 0)),
        );

        let (scene_path, manifest) =
            create_named_session_from_scene(&session_root, "agent-a", &scene).unwrap();

        let loaded_scene = load_scene(&scene_path).unwrap();
        let loaded_manifest =
            load_session_manifest(&session_manifest_path(&session_root, "agent-a")).unwrap();

        assert_eq!(scene_path, session_root.join("agent-a.json"));
        assert_eq!(loaded_scene.object_count(), 1);
        assert_eq!(manifest.session_id, "agent-a");
        assert_eq!(loaded_manifest.session_id, "agent-a");
        assert_eq!(loaded_manifest.scene_path, scene_path.display().to_string());

        let _ = fs::remove_dir_all(session_root);
    }

    #[test]
    fn create_named_session_from_scene_rejects_existing_manifest() {
        let session_root = temp_session_root("existing");
        let scene = SceneDocument::default();

        create_named_session_from_scene(&session_root, "agent-a", &scene).unwrap();
        let error = create_named_session_from_scene(&session_root, "agent-a", &scene)
            .expect_err("expected existing session to be rejected");

        assert!(error.to_string().contains("already exists"));

        let _ = fs::remove_dir_all(session_root);
    }
}
