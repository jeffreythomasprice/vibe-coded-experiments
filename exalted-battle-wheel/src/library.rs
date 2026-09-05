//! A user-saved action library, persisted via `prefs::Prefs::library` exactly like any other
//! preference: JSON in localStorage, kept in sync across tabs by the `storage` event.

use exalted_battle_wheel::battle::{
    template, ActionKind, Declaration, DeclaredAction, DeclaredEffect, MarkerId, Sequence, SequenceStep,
};
use serde::{Deserialize, Serialize};

pub type SavedId = u32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LibraryError {
    #[error("no saved action with id {0}")]
    Unknown(SavedId),
    #[error("a saved action needs a name")]
    Unnamed,
    #[error("an effect must span at least one tick")]
    ZeroDuration,
}

/// The saved, editable form of an effect — unlike `DeclaredEffect`, it carries no `MarkerId`: one
/// is allocated fresh (via `BattleLog::alloc_marker_id`) each time the saved action is declared.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedEffect {
    pub label: String,
    pub delay: u32,
    pub ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SavedShape {
    Single { kind: ActionKind, speed: u32, dv_penalty: i32 },
    Sequence { steps: Vec<SequenceStep> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedAction {
    pub id: SavedId,
    pub name: String,
    pub note: String,
    pub shape: SavedShape,
    pub effects: Vec<SavedEffect>,
}

pub enum SavedDeclaration {
    Action(DeclaredAction),
    Sequence(Sequence),
}

impl SavedAction {
    /// `ids` must carry one `MarkerId` per entry in `effects`, allocated by the caller so replay
    /// stays deterministic.
    pub fn build(&self, ids: &[MarkerId]) -> SavedDeclaration {
        let effects: Vec<DeclaredEffect> = self
            .effects
            .iter()
            .zip(ids)
            .map(|(effect, id)| DeclaredEffect { id: *id, label: effect.label.clone(), delay: effect.delay, ticks: effect.ticks })
            .collect();

        match &self.shape {
            SavedShape::Single { kind, speed, dv_penalty } => {
                let declaration = Declaration {
                    name: Some(self.name.clone()),
                    speed: Some(*speed),
                    dv_penalty: Some(*dv_penalty),
                    note: self.note.clone(),
                    effects,
                    ..Default::default()
                };
                SavedDeclaration::Action(template(*kind).declare(declaration))
            }
            SavedShape::Sequence { steps } => {
                SavedDeclaration::Sequence(Sequence { name: self.name.clone(), steps: steps.clone(), current: 0, effects })
            }
        }
    }
}

fn validate(name: &str, effects: &[SavedEffect]) -> Result<(), LibraryError> {
    if name.trim().is_empty() {
        return Err(LibraryError::Unnamed);
    }
    if effects.iter().any(|effect| effect.ticks == 0) {
        return Err(LibraryError::ZeroDuration);
    }
    Ok(())
}

/// `next_id` resets to 0 once `actions` is empty, so a library with nothing saved always encodes
/// identically to `Library::default()` — the same trick `Pref::store` relies on for every other
/// preference to clean up its localStorage key once nothing non-default is left. While anything
/// is saved, ids never get reassigned by a delete elsewhere in the list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    next_id: SavedId,
    actions: Vec<SavedAction>,
}

impl Library {
    pub fn actions(&self) -> &[SavedAction] {
        &self.actions
    }

    pub fn find(&self, id: SavedId) -> Option<&SavedAction> {
        self.actions.iter().find(|action| action.id == id)
    }

    pub fn add(&mut self, name: String, note: String, shape: SavedShape, effects: Vec<SavedEffect>) -> Result<SavedId, LibraryError> {
        validate(&name, &effects)?;
        let id = self.next_id;
        self.next_id += 1;
        self.actions.push(SavedAction { id, name, note, shape, effects });
        Ok(id)
    }

    pub fn replace(&mut self, action: SavedAction) -> Result<(), LibraryError> {
        validate(&action.name, &action.effects)?;
        let existing = self.actions.iter_mut().find(|a| a.id == action.id).ok_or(LibraryError::Unknown(action.id))?;
        *existing = action;
        Ok(())
    }

    pub fn remove(&mut self, id: SavedId) -> Result<(), LibraryError> {
        let index = self.actions.iter().position(|a| a.id == id).ok_or(LibraryError::Unknown(id))?;
        self.actions.remove(index);
        if self.actions.is_empty() {
            self.next_id = 0;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exalted_battle_wheel::battle::SpeedSpec;

    fn single(name: &str) -> (String, String, SavedShape, Vec<SavedEffect>) {
        (name.to_string(), String::new(), SavedShape::Single { kind: ActionKind::Attack, speed: 4, dv_penalty: -1 }, Vec::new())
    }

    #[test]
    fn add_assigns_increasing_ids() {
        let mut library = Library::default();
        let (name, note, shape, effects) = single("Sweeping Blow");
        let first = library.add(name, note, shape, effects).unwrap();
        let (name, note, shape, effects) = single("Cascade of Cutting Terror");
        let second = library.add(name, note, shape, effects).unwrap();
        assert_ne!(first, second);
        assert_eq!(library.actions().len(), 2);
    }

    #[test]
    fn add_rejects_a_blank_name() {
        let mut library = Library::default();
        let (_, note, shape, effects) = single("");
        let err = library.add("   ".to_string(), note, shape, effects).unwrap_err();
        assert_eq!(err, LibraryError::Unnamed);
    }

    #[test]
    fn add_rejects_a_zero_duration_effect() {
        let mut library = Library::default();
        let (name, note, shape, _) = single("Butterflies");
        let effects = vec![SavedEffect { label: "Mark".to_string(), delay: 0, ticks: 0 }];
        let err = library.add(name, note, shape, effects).unwrap_err();
        assert_eq!(err, LibraryError::ZeroDuration);
    }

    #[test]
    fn replace_updates_in_place() {
        let mut library = Library::default();
        let (name, note, shape, effects) = single("Sweeping Blow");
        let id = library.add(name, note, shape, effects).unwrap();
        let mut updated = library.find(id).unwrap().clone();
        updated.name = "Renamed".to_string();
        library.replace(updated).unwrap();
        assert_eq!(library.find(id).unwrap().name, "Renamed");
    }

    #[test]
    fn replace_rejects_an_unknown_id() {
        let mut library = Library::default();
        let (name, note, shape, effects) = single("Sweeping Blow");
        let action = SavedAction { id: 99, name, note, shape, effects };
        let err = library.replace(action).unwrap_err();
        assert_eq!(err, LibraryError::Unknown(99));
    }

    #[test]
    fn remove_drops_the_action_and_resets_next_id_when_empty() {
        let mut library = Library::default();
        let (name, note, shape, effects) = single("Sweeping Blow");
        let id = library.add(name, note, shape, effects).unwrap();
        library.remove(id).unwrap();
        assert!(library.actions().is_empty());
        assert_eq!(library, Library::default());
    }

    #[test]
    fn remove_rejects_an_unknown_id() {
        let mut library = Library::default();
        assert_eq!(library.remove(0).unwrap_err(), LibraryError::Unknown(0));
    }

    #[test]
    fn empty_library_round_trips_and_matches_default() {
        let json = serde_json::to_string(&Library::default()).unwrap();
        let decoded: Library = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, Library::default());
    }

    #[test]
    fn single_action_round_trips_through_json() {
        let mut library = Library::default();
        let effects = vec![SavedEffect { label: "Butterflies".to_string(), delay: 1, ticks: 3 }];
        let id = library.add("Death of Obsidian Butterflies".to_string(), "note".to_string(), SavedShape::Sequence {
            steps: vec![SequenceStep { label: "Shape".to_string(), speed: SpeedSpec::Fixed(5), dv_penalty: -3 }],
        }, effects).unwrap();

        let json = serde_json::to_string(&library).unwrap();
        let decoded: Library = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.find(id), library.find(id));
    }

    #[test]
    fn build_single_uses_the_saved_speed_and_dv_and_name() {
        let (name, note, shape, effects) = single("Sweeping Blow");
        let action = SavedAction { id: 0, name, note, shape, effects };
        let SavedDeclaration::Action(declared) = action.build(&[]) else { panic!("expected a single action") };
        assert_eq!(declared.label, "Sweeping Blow");
        assert_eq!(declared.speed, 4);
        assert_eq!(declared.dv_penalty, -1);
    }

    #[test]
    fn build_zips_saved_effects_with_the_given_marker_ids() {
        let action = SavedAction {
            id: 0,
            name: "Death of Obsidian Butterflies".to_string(),
            note: String::new(),
            shape: SavedShape::Sequence { steps: vec![SequenceStep { label: "Cast".to_string(), speed: SpeedSpec::Variable { default: 5 }, dv_penalty: 0 }] },
            effects: vec![SavedEffect { label: "Butterflies".to_string(), delay: 0, ticks: 3 }],
        };
        let SavedDeclaration::Sequence(sequence) = action.build(&[MarkerId(7)]) else { panic!("expected a sequence") };
        assert_eq!(sequence.effects.len(), 1);
        assert_eq!(sequence.effects[0].id, MarkerId(7));
        assert_eq!(sequence.effects[0].ticks, 3);
    }
}
