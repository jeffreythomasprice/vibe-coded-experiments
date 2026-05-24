use crate::character::{BackgroundKind, Character};
use crate::error::{ValidationError, ValidationReport};

pub fn validate_hearthstones(c: &Character) -> ValidationReport {
    let mut report = ValidationReport::new();
    let max = c.background(BackgroundKind::Manse);
    let got = c.hearthstones.len();
    if got > max as usize {
        report.push(ValidationError::TooManyHearthstones { got, max });
    }
    report
}

pub fn validate_artifacts(c: &Character) -> ValidationReport {
    let mut report = ValidationReport::new();

    let max = c.background(BackgroundKind::Artifact);
    let got = c.equipment.artifacts.len();
    if got > max as usize {
        report.push(ValidationError::TooManyArtifacts { got, max });
    }

    let hearthstone_names: std::collections::BTreeSet<&str> =
        c.hearthstones.iter().map(|h| h.name.as_str()).collect();

    for art in &c.equipment.artifacts {
        if art.socketed_hearthstones.len() > art.hearthstone_sockets as usize {
            report.push(ValidationError::OversocketedArtifact {
                artifact: art.name.clone(),
                sockets: art.hearthstone_sockets,
                socketed: art.socketed_hearthstones.len(),
            });
        }
        for hs in &art.socketed_hearthstones {
            if !hearthstone_names.contains(hs.as_str()) {
                report.push(ValidationError::UnknownSocketedHearthstone {
                    artifact: art.name.clone(),
                    hearthstone: hs.clone(),
                });
            }
        }
    }

    let artifact_names: std::collections::BTreeSet<&str> = c
        .equipment
        .artifacts
        .iter()
        .map(|a| a.name.as_str())
        .collect();

    for w in &c.equipment.weapons {
        if let Some(name) = &w.artifact_name {
            if !artifact_names.contains(name.as_str()) {
                report.push(ValidationError::MissingLinkedArtifact {
                    item: format!("Weapon::{}", w.name),
                    artifact_name: name.clone(),
                });
            }
        }
    }
    if let Some(armor) = &c.equipment.armor {
        if let Some(name) = &armor.artifact_name {
            if !artifact_names.contains(name.as_str()) {
                report.push(ValidationError::MissingLinkedArtifact {
                    item: format!("Armor::{}", armor.name),
                    artifact_name: name.clone(),
                });
            }
        }
    }

    report
}
