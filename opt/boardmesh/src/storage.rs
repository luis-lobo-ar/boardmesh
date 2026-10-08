use crate::scene::SceneDocument;
use crate::universe::UniverseDocument;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub fn ensure_scene_file(path: &Path) -> Result<SceneDocument> {
    match load_scene(path) {
        Ok(scene) => Ok(scene),
        Err(error) if !path.exists() => {
            let scene = SceneDocument::default();
            save_scene(path, &scene)?;
            Ok(scene)
        }
        Err(error) => Err(error),
    }
}

pub fn load_scene(path: &Path) -> Result<SceneDocument> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read scene file {}", path.display()))?;
    if let Ok(universe) = serde_json::from_str::<UniverseDocument>(&contents) {
        return Ok(universe.scene);
    }
    let scene = serde_json::from_str::<SceneDocument>(&contents)
        .with_context(|| format!("failed to parse scene file {}", path.display()))?;
    Ok(scene)
}

pub fn save_scene(path: &Path, scene: &SceneDocument) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create scene directory {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("tmp");
    let json = match fs::read_to_string(path) {
        Ok(contents) => {
            if let Ok(mut universe) = serde_json::from_str::<UniverseDocument>(&contents) {
                universe.scene = scene.clone();
                serde_json::to_vec_pretty(&universe)
                    .context("failed to serialize universe-backed scene file")?
            } else {
                serde_json::to_vec_pretty(scene).context("failed to serialize scene file")?
            }
        }
        Err(_) => serde_json::to_vec_pretty(scene).context("failed to serialize scene file")?,
    };
    fs::write(&tmp_path, json).with_context(|| {
        format!(
            "failed to write temporary scene file {}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to replace scene file {}", path.display()))?;
    Ok(())
}

pub fn ensure_universe_file(path: &Path) -> Result<UniverseDocument> {
    match load_universe(path) {
        Ok(universe) => Ok(universe),
        Err(error) if !path.exists() => {
            let universe = UniverseDocument::default();
            save_universe(path, &universe)?;
            Ok(universe)
        }
        Err(error) => Err(error),
    }
}

pub fn load_universe(path: &Path) -> Result<UniverseDocument> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read universe file {}", path.display()))?;
    if let Ok(universe) = serde_json::from_str::<UniverseDocument>(&contents) {
        return Ok(universe);
    }

    let scene = serde_json::from_str::<SceneDocument>(&contents)
        .with_context(|| format!("failed to parse universe file {}", path.display()))?;
    Ok(UniverseDocument::from_scene(scene).universe)
}

pub fn save_universe(path: &Path, universe: &UniverseDocument) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create universe directory {}", parent.display()))?;
    }

    let tmp_path = path.with_extension("tmp");
    let json = serde_json::to_vec_pretty(universe).context("failed to serialize universe file")?;
    fs::write(&tmp_path, json).with_context(|| {
        format!(
            "failed to write temporary universe file {}",
            tmp_path.display()
        )
    })?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to replace universe file {}", path.display()))?;
    Ok(())
}

pub fn last_modified(path: &Path) -> Result<SystemTime> {
    let modified = fs::metadata(path)
        .with_context(|| format!("failed to stat scene file {}", path.display()))?
        .modified()
        .with_context(|| format!("failed to read modified time for {}", path.display()))?;
    Ok(modified)
}

pub fn load_scene_if_changed(
    path: &Path,
    last_seen: Option<SystemTime>,
) -> Result<Option<(SceneDocument, SystemTime)>> {
    let modified = match last_modified(path) {
        Ok(modified) => modified,
        Err(error) if !path.exists() => return Ok(None),
        Err(error) => return Err(error),
    };

    if let Some(previous) = last_seen
        && modified <= previous
    {
        return Ok(None);
    }

    let scene = load_scene(path)?;
    Ok(Some((scene, modified)))
}

pub fn resolve_session_path(session_root: &Path, session_id: &str) -> Result<PathBuf> {
    validate_session_id(session_id)?;
    Ok(session_root.join(format!("{session_id}.json")))
}

pub fn validate_session_id(session_id: &str) -> Result<()> {
    let trimmed = session_id.trim();
    anyhow::ensure!(!trimmed.is_empty(), "session id cannot be empty");
    anyhow::ensure!(
        trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')),
        "session id may only contain ASCII letters, digits, '-', '_' or '.'"
    );
    anyhow::ensure!(
        trimmed != "." && trimmed != "..",
        "session id cannot be '.' or '..'"
    );
    Ok(())
}

pub fn session_id_from_path(session_root: &Path, scene_path: &Path) -> Option<String> {
    let parent = scene_path.parent()?;
    if parent != session_root {
        return None;
    }

    let stem = scene_path.file_stem()?.to_str()?;
    validate_session_id(stem).ok()?;
    Some(stem.to_string())
}

pub fn list_session_ids(session_root: &Path) -> Result<Vec<String>> {
    if !session_root.exists() {
        return Ok(Vec::new());
    }

    let mut session_ids = fs::read_dir(session_root)
        .with_context(|| {
            format!(
                "failed to read sessions directory {}",
                session_root.display()
            )
        })?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                return None;
            }
            let stem = path.file_stem().and_then(|stem| stem.to_str())?;
            if stem.ends_with(".manifest") {
                return None;
            }
            Some(stem.to_string())
        })
        .filter(|session_id| validate_session_id(session_id).is_ok())
        .collect::<Vec<_>>();
    session_ids.sort();
    Ok(session_ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::SceneRect;
    use crate::universe::ViewportState;

    #[test]
    fn resolve_session_path_uses_json_file_in_session_root() {
        let root = Path::new("/tmp/boardmesh-sessions");
        let path = resolve_session_path(root, "agent-a").unwrap();
        assert_eq!(path, root.join("agent-a.json"));
    }

    #[test]
    fn validate_session_id_rejects_invalid_names() {
        assert!(validate_session_id("agent-a").is_ok());
        assert!(validate_session_id("agent/a").is_err());
        assert!(validate_session_id("../oops").is_err());
        assert!(validate_session_id("").is_err());
    }

    #[test]
    fn session_id_from_path_matches_direct_children_only() {
        let root = Path::new("/tmp/boardmesh-sessions");
        assert_eq!(
            session_id_from_path(root, &root.join("shared.json")).as_deref(),
            Some("shared")
        );
        assert_eq!(
            session_id_from_path(root, Path::new("/tmp/other/shared.json")),
            None
        );
    }

    #[test]
    fn list_session_ids_ignores_manifest_json_files() {
        let root = std::env::temp_dir().join(format!(
            "boardmesh-storage-tests-{}-{}",
            std::process::id(),
            crate::session::unix_time_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("alpha.json"), "{}").unwrap();
        fs::write(root.join("alpha.manifest.json"), "{}").unwrap();
        fs::write(root.join("beta.json"), "{}").unwrap();

        let session_ids = list_session_ids(&root).unwrap();

        assert_eq!(session_ids, vec!["alpha".to_string(), "beta".to_string()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn universe_file_round_trips_with_canvas_state() {
        let root = std::env::temp_dir().join(format!(
            "boardmesh-universe-tests-{}-{}",
            std::process::id(),
            crate::session::unix_time_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("universe.json");

        let mut universe = UniverseDocument {
            viewport: ViewportState {
                pan_x: 12.5,
                pan_y: -8.0,
                zoom: 1.5,
            },
            ..UniverseDocument::default()
        };
        let canvas_id = universe.create_canvas("Main", SceneRect::new(10, 20, 300, 180));
        let object_id = universe.scene.add_rectangle(
            SceneRect::new(20, 30, 40, 50),
            crate::scene::SceneStroke::default(),
            None,
        );
        assert!(universe.assign_object_to_canvas(&object_id, &canvas_id));

        save_universe(&path, &universe).unwrap();
        let restored = load_universe(&path).unwrap();

        assert_eq!(restored, universe);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_universe_migrates_legacy_scene_json() {
        let root = std::env::temp_dir().join(format!(
            "boardmesh-universe-migrate-{}-{}",
            std::process::id(),
            crate::session::unix_time_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("legacy.json");

        let mut scene = SceneDocument::default();
        let object_id = scene.add_rectangle(
            SceneRect::new(10, 20, 40, 50),
            crate::scene::SceneStroke::default(),
            None,
        );
        let _ = scene.select_objects(vec![object_id.clone()]);
        save_scene(&path, &scene).unwrap();

        let universe = load_universe(&path).unwrap();

        assert_eq!(universe.scene, scene);
        assert!(universe.canvases.is_empty());
        assert!(universe.memberships.is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_scene_reads_the_scene_from_a_universe_document() {
        let root = std::env::temp_dir().join(format!(
            "boardmesh-scene-from-universe-{}-{}",
            std::process::id(),
            crate::session::unix_time_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("universe.json");

        let mut universe = UniverseDocument::default();
        let object_id = universe.scene.add_text(
            crate::scene::TextObject {
                position: crate::scene::CanvasPoint::new(20, 30),
                content: "hello".to_string(),
            },
            crate::scene::SceneColor::default(),
        );
        let canvas_id = universe.create_canvas("Main", SceneRect::new(0, 0, 200, 120));
        assert!(universe.assign_object_to_canvas(&object_id, &canvas_id));
        save_universe(&path, &universe).unwrap();

        let scene = load_scene(&path).unwrap();

        assert_eq!(scene, universe.scene);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn save_scene_preserves_existing_universe_metadata() {
        let root = std::env::temp_dir().join(format!(
            "boardmesh-scene-save-universe-{}-{}",
            std::process::id(),
            crate::session::unix_time_ms()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("universe.json");

        let mut universe = UniverseDocument::default();
        let canvas_id = universe.create_canvas("Main", SceneRect::new(0, 0, 200, 120));
        let object_id = universe.scene.add_rectangle(
            SceneRect::new(10, 20, 40, 50),
            crate::scene::SceneStroke::default(),
            None,
        );
        assert!(universe.assign_object_to_canvas(&object_id, &canvas_id));
        save_universe(&path, &universe).unwrap();

        let mut next_scene = universe.scene.clone();
        next_scene.add_circle(
            crate::scene::CanvasPoint::new(120, 70),
            18,
            crate::scene::SceneStroke::default(),
            None,
        );
        save_scene(&path, &next_scene).unwrap();

        let restored = load_universe(&path).unwrap();
        assert_eq!(restored.scene, next_scene);
        assert_eq!(restored.canvases, universe.canvases);
        assert_eq!(restored.memberships, universe.memberships);
        let _ = fs::remove_dir_all(root);
    }
}
