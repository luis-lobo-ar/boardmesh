use crate::scene::SceneDocument;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub label: String,
    pub scene: SceneDocument,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneHistory {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    limit: usize,
}

impl SceneHistory {
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            limit,
        }
    }

    pub fn push(&mut self, label: impl Into<String>, scene: SceneDocument) {
        self.undo_stack.push(HistoryEntry {
            label: label.into(),
            scene,
        });
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self, current_scene: SceneDocument) -> Option<SceneDocument> {
        let previous_entry = self.undo_stack.pop()?;
        self.redo_stack.push(HistoryEntry {
            label: previous_entry.label,
            scene: current_scene,
        });
        Some(previous_entry.scene)
    }

    pub fn redo(&mut self, current_scene: SceneDocument) -> Option<SceneDocument> {
        let next_entry = self.redo_stack.pop()?;
        self.undo_stack.push(HistoryEntry {
            label: next_entry.label,
            scene: current_scene,
        });
        Some(next_entry.scene)
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn undo_entries(&self) -> &[HistoryEntry] {
        &self.undo_stack
    }

    pub fn redo_entries(&self) -> &[HistoryEntry] {
        &self.redo_stack
    }

    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    pub fn pop_undo(&mut self) -> Option<HistoryEntry> {
        self.undo_stack.pop()
    }

    pub fn push_redo(&mut self, label: impl Into<String>, scene: SceneDocument) {
        self.redo_stack.push(HistoryEntry {
            label: label.into(),
            scene,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_clears_redo_and_enforces_limit() {
        let mut history = SceneHistory::new(2);
        history.push("A", SceneDocument::default());
        history.redo_stack.push(HistoryEntry {
            label: "stale".to_string(),
            scene: SceneDocument::default(),
        });

        history.push("B", SceneDocument::default());
        history.push("C", SceneDocument::default());

        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo_entries()[0].label, "B");
        assert_eq!(history.undo_entries()[1].label, "C");
    }

    #[test]
    fn undo_and_redo_move_between_stacks() {
        let mut history = SceneHistory::new(10);
        let mut scene_a = SceneDocument::default();
        scene_a.selected_object_ids.push("a".to_string());
        let mut scene_b = SceneDocument::default();
        scene_b.selected_object_ids.push("b".to_string());
        let mut scene_c = SceneDocument::default();
        scene_c.selected_object_ids.push("c".to_string());

        history.push("A", scene_a.clone());
        history.push("B", scene_b.clone());

        let undone = history.undo(scene_c.clone()).expect("undo result");
        assert_eq!(undone, scene_b);
        assert_eq!(history.undo_len(), 1);
        assert_eq!(history.redo_len(), 1);

        let redone = history.redo(scene_b.clone()).expect("redo result");
        assert_eq!(redone, scene_c);
        assert_eq!(history.undo_len(), 2);
        assert_eq!(history.redo_len(), 0);
    }
}
