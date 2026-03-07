use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use tracing::{info, warn};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

use crate::config::arena_config::ArenaConfig;
use crate::config::cli_args::MatchArgs;
use crate::config::constants::SimConstants;
use crate::config::loadout::RobotConfig;
use crate::config::project::ProjectConfig;
use crate::sim::arena::{Arena, ArenaBody};
use crate::sim::match_runner::{MatchConfig, MatchFormat, RobotEntry};
use crate::sim::robot::Loadout;
use crate::vm::assembler::assemble;

fn has_glob_chars(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

fn expand_group_globs(
    groups: &HashMap<String, Vec<PathBuf>>,
    config_dir: &Path,
) -> Result<HashMap<String, Vec<PathBuf>>> {
    let mut expanded = HashMap::new();
    for (name, entries) in groups {
        let mut result = Vec::new();
        for entry in entries {
            let entry_str = entry.to_string_lossy();
            if has_glob_chars(&entry_str) {
                let pattern = config_dir.join(&*entry_str);
                let pattern_str = pattern.to_string_lossy();
                let matches: Vec<PathBuf> = glob::glob(&pattern_str)
                    .with_context(|| {
                        format!("invalid glob pattern '{}' in group '{}'", entry_str, name)
                    })?
                    .filter_map(|r| r.ok())
                    .map(|p| {
                        p.strip_prefix(config_dir)
                            .map(|rel| rel.to_path_buf())
                            .unwrap_or(p)
                    })
                    .collect();
                if matches.is_empty() {
                    warn!(
                        "glob pattern '{}' in group '{}' matched no files",
                        entry_str, name
                    );
                }
                result.extend(matches);
            } else {
                result.push(entry.clone());
            }
        }
        expanded.insert(name.clone(), result);
    }
    Ok(expanded)
}

fn load_robot(path: &Path) -> Result<(RobotConfig, String)> {
    let config = RobotConfig::load(path)?;

    let asm_path = path
        .parent()
        .unwrap_or(Path::new("."))
        .join(&config.program);
    let asm_source = std::fs::read_to_string(&asm_path)
        .with_context(|| format!("failed to read program '{}'", asm_path.display()))?;

    Ok((config, asm_source))
}

fn resolve_robot_path(
    entry: &Path,
    groups: &HashMap<String, Vec<PathBuf>>,
    config_dir: &Path,
    rng: &mut StdRng,
    from_cli: bool,
) -> Result<PathBuf> {
    let entry_str = entry.to_string_lossy();
    if let Some(group_name) = entry_str.strip_prefix('@') {
        let members = groups
            .get(group_name)
            .with_context(|| format!("unknown robot group '@{}'", group_name))?;
        if members.is_empty() {
            bail!("robot group '{}' is empty", group_name);
        }
        let chosen = members.choose(rng).unwrap();
        let resolved = if from_cli {
            chosen.clone()
        } else {
            config_dir.join(chosen)
        };
        info!("group '{}': selected '{}'", group_name, chosen.display());
        Ok(resolved)
    } else if from_cli {
        Ok(entry.to_path_buf())
    } else {
        Ok(config_dir.join(entry))
    }
}

pub fn resolve_match_config(args: &MatchArgs, project: &ProjectConfig) -> Result<MatchConfig> {
    let config_dir = args.config.parent().unwrap_or(Path::new(".")).to_path_buf();

    let seed = args.seed.or(project.seed).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    });

    let raw_groups = project.groups.clone().unwrap_or_default();
    let groups = expand_group_globs(&raw_groups, &config_dir)?;
    let mut rng = StdRng::seed_from_u64(seed);

    let robot_paths: Vec<PathBuf> = if !args.robots.is_empty() {
        info!("robots: from CLI arguments");
        args.robots
            .iter()
            .map(|p| resolve_robot_path(p, &groups, &config_dir, &mut rng, true))
            .collect::<Result<Vec<_>>>()?
    } else if let Some(ref robots) = project.robots {
        info!("robots: from project config");
        robots
            .iter()
            .map(|p| resolve_robot_path(p, &groups, &config_dir, &mut rng, false))
            .collect::<Result<Vec<_>>>()?
    } else {
        bail!("no robots specified (provide robot files as arguments or in the config file)");
    };

    let arena_path: Option<PathBuf> = if args.arena.is_some() {
        info!("arena: from CLI argument");
        args.arena.clone()
    } else {
        if project.arena.is_some() {
            info!("arena: from project config");
        }
        project.arena.as_ref().map(|p| config_dir.join(p))
    };

    let max_ticks = args.ticks.or(project.max_ticks).unwrap_or(10_000);

    info!(
        "max_ticks: {} (source: {}), seed: {} (source: {})",
        max_ticks,
        if args.ticks.is_some() {
            "CLI"
        } else if project.max_ticks.is_some() {
            "config"
        } else {
            "default"
        },
        seed,
        if args.seed.is_some() {
            "CLI"
        } else if project.seed.is_some() {
            "config"
        } else {
            "default"
        },
    );

    let constants = SimConstants::default();

    let mut entries = Vec::new();
    for path in &robot_paths {
        let (config, asm_source) = load_robot(path)?;
        config
            .validate(&constants)
            .with_context(|| format!("invalid robot config '{}'", path.display()))?;

        let program = assemble(&asm_source)
            .with_context(|| format!("failed to assemble program for robot '{}'", config.name))?;

        let loadout = Loadout::from(&config.loadout);
        entries.push(RobotEntry {
            name: config.name,
            program,
            loadout,
        });
    }

    // Number duplicate robot names
    let mut name_counts: HashMap<String, usize> = HashMap::new();
    for entry in &entries {
        *name_counts.entry(entry.name.clone()).or_default() += 1;
    }
    let mut name_indices: HashMap<String, usize> = HashMap::new();
    for entry in &mut entries {
        if name_counts[&entry.name] > 1 {
            let idx = name_indices.entry(entry.name.clone()).or_insert(0);
            *idx += 1;
            entry.name = format!("{} {}", entry.name, idx);
        }
    }

    let arena: Arena = if let Some(arena_path) = &arena_path {
        let arena_config = ArenaConfig::load(arena_path)
            .with_context(|| format!("failed to load arena config '{}'", arena_path.display()))?;
        (&arena_config).into()
    } else {
        bail!(
            "no arena specified (provide an arena file with --arena or set 'arena' in the config file)"
        );
    };

    if entries.len() > arena.spawn_points.len() {
        bail!(
            "arena has {} spawn points but {} robots were specified",
            arena.spawn_points.len(),
            entries.len()
        );
    }

    let format = if entries.len() == 2 {
        MatchFormat::Duel
    } else {
        MatchFormat::FreeForAll
    };

    log_resolved_config(&arena, &entries, &constants, max_ticks, seed, &format);

    Ok(MatchConfig {
        format,
        max_ticks,
        arena,
        robot_configs: entries,
        constants,
        seed,
    })
}

fn log_resolved_config(
    arena: &Arena,
    entries: &[RobotEntry],
    constants: &SimConstants,
    max_ticks: u32,
    seed: u64,
    format: &MatchFormat,
) {
    info!("=== Resolved Match Config ===");
    info!(
        "format: {:?}, max_ticks: {}, seed: {}",
        format, max_ticks, seed
    );

    info!(
        "arena: {}x{}, {} bodies (excl. walls), {} spawn points",
        arena.bounds.0,
        arena.bounds.1,
        arena.bodies.len(),
        arena.spawn_points.len(),
    );
    for (i, body) in arena.bodies.iter().enumerate() {
        match body {
            ArenaBody::Rectangle {
                x,
                y,
                width,
                height,
                rotation,
            } => {
                info!(
                    "  body[{}]: rectangle at ({}, {}), {}x{}, rot={:.1}°",
                    i,
                    x,
                    y,
                    width,
                    height,
                    rotation.to_degrees()
                );
            }
            ArenaBody::Circle { x, y, radius } => {
                info!("  body[{}]: circle at ({}, {}), r={}", i, x, y, radius);
            }
        }
    }
    for (i, sp) in arena.spawn_points.iter().enumerate() {
        info!(
            "  spawn[{}]: ({}, {}), facing {:.1}°",
            i, sp.position.0, sp.position.1, sp.facing
        );
    }

    info!("robots: {} entries", entries.len());
    for (i, entry) in entries.iter().enumerate() {
        let lo = &entry.loadout;
        let max_hp = constants.base_hp + constants.hp_per_point * lo.hp_points as i32;
        let max_speed = constants.base_speed + constants.speed_per_point * lo.speed_points as f32;
        let armor = constants.base_armor + lo.armor_points as i32;
        let gun_power =
            constants.base_gun_power + constants.gun_power_per_point * lo.gun_points as i32;

        info!(
            "  [{}] {} — loadout: hp={} spd={} arm={} gun={} ({}/{} pts) → HP:{} Speed:{:.1} Armor:{} Gun:{}",
            i,
            entry.name,
            lo.hp_points,
            lo.speed_points,
            lo.armor_points,
            lo.gun_points,
            lo.hp_points + lo.speed_points + lo.armor_points + lo.gun_points,
            constants.point_budget,
            max_hp,
            max_speed,
            armor,
            gun_power,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_group_picks_from_members() {
        let mut groups = HashMap::new();
        groups.insert(
            "movers".to_string(),
            vec![PathBuf::from("patrol.toml"), PathBuf::from("dumb.toml")],
        );
        let config_dir = Path::new("examples");
        let mut rng = StdRng::seed_from_u64(42);

        let result =
            resolve_robot_path(Path::new("@movers"), &groups, config_dir, &mut rng, true).unwrap();
        assert!(result == PathBuf::from("patrol.toml") || result == PathBuf::from("dumb.toml"));
    }

    #[test]
    fn resolve_group_from_config_prepends_config_dir() {
        let mut groups = HashMap::new();
        groups.insert("campers".to_string(), vec![PathBuf::from("spinner.toml")]);
        let config_dir = Path::new("examples");
        let mut rng = StdRng::seed_from_u64(42);

        let result =
            resolve_robot_path(Path::new("@campers"), &groups, config_dir, &mut rng, false)
                .unwrap();
        assert_eq!(result, PathBuf::from("examples/spinner.toml"));
    }

    #[test]
    fn resolve_unknown_group_errors() {
        let groups = HashMap::new();
        let config_dir = Path::new(".");
        let mut rng = StdRng::seed_from_u64(42);

        let result = resolve_robot_path(
            Path::new("@nonexistent"),
            &groups,
            config_dir,
            &mut rng,
            true,
        );
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("unknown robot group")
        );
    }

    #[test]
    fn resolve_plain_path_unchanged_from_cli() {
        let groups = HashMap::new();
        let config_dir = Path::new("examples");
        let mut rng = StdRng::seed_from_u64(42);

        let result = resolve_robot_path(
            Path::new("dumb.toml"),
            &groups,
            config_dir,
            &mut rng,
            true,
        )
        .unwrap();
        assert_eq!(result, PathBuf::from("dumb.toml"));
    }

    #[test]
    fn resolve_group_is_deterministic_with_same_seed() {
        let mut groups = HashMap::new();
        groups.insert(
            "movers".to_string(),
            vec![PathBuf::from("patrol.toml"), PathBuf::from("dumb.toml")],
        );
        let config_dir = Path::new(".");

        let mut rng1 = StdRng::seed_from_u64(99);
        let result1 =
            resolve_robot_path(Path::new("@movers"), &groups, config_dir, &mut rng1, true)
                .unwrap();

        let mut rng2 = StdRng::seed_from_u64(99);
        let result2 =
            resolve_robot_path(Path::new("@movers"), &groups, config_dir, &mut rng2, true)
                .unwrap();

        assert_eq!(result1, result2);
    }

    #[test]
    fn expand_globs_expands_matching_patterns() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.toml"), "{}").unwrap();
        std::fs::write(dir.path().join("b.toml"), "{}").unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();

        let mut groups = HashMap::new();
        groups.insert("all_toml".to_string(), vec![PathBuf::from("*.toml")]);

        let result = expand_group_globs(&groups, dir.path()).unwrap();
        let mut paths = result["all_toml"].clone();
        paths.sort();
        assert_eq!(
            paths,
            vec![PathBuf::from("a.toml"), PathBuf::from("b.toml")]
        );
    }

    #[test]
    fn expand_globs_passes_non_glob_entries_through() {
        let dir = tempfile::tempdir().unwrap();

        let mut groups = HashMap::new();
        groups.insert(
            "static".to_string(),
            vec![PathBuf::from("spinner.toml"), PathBuf::from("patrol.toml")],
        );

        let result = expand_group_globs(&groups, dir.path()).unwrap();
        assert_eq!(
            result["static"],
            vec![PathBuf::from("spinner.toml"), PathBuf::from("patrol.toml")]
        );
    }

    fn make_entry(name: &str) -> RobotEntry {
        use crate::vm::instruction::Program;
        RobotEntry {
            name: name.to_string(),
            program: Program {
                instructions: vec![],
            },
            loadout: Loadout {
                hp_points: 0,
                speed_points: 0,
                armor_points: 0,
                gun_points: 0,
            },
        }
    }

    fn deduplicate_names(entries: &mut Vec<RobotEntry>) {
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        for entry in entries.iter() {
            *name_counts.entry(entry.name.clone()).or_default() += 1;
        }
        let mut name_indices: HashMap<String, usize> = HashMap::new();
        for entry in entries.iter_mut() {
            if name_counts[&entry.name] > 1 {
                let idx = name_indices.entry(entry.name.clone()).or_insert(0);
                *idx += 1;
                entry.name = format!("{} {}", entry.name, idx);
            }
        }
    }

    #[test]
    fn deduplicate_names_numbers_duplicates() {
        let mut entries = vec![make_entry("Spinner"), make_entry("Spinner")];
        deduplicate_names(&mut entries);
        assert_eq!(entries[0].name, "Spinner 1");
        assert_eq!(entries[1].name, "Spinner 2");
    }

    #[test]
    fn deduplicate_names_leaves_unique_unchanged() {
        let mut entries = vec![make_entry("Spinner"), make_entry("Patrol")];
        deduplicate_names(&mut entries);
        assert_eq!(entries[0].name, "Spinner");
        assert_eq!(entries[1].name, "Patrol");
    }

    #[test]
    fn deduplicate_names_handles_multiple_groups() {
        let mut entries = vec![
            make_entry("Foo"),
            make_entry("Bar"),
            make_entry("Foo"),
            make_entry("Bar"),
        ];
        deduplicate_names(&mut entries);
        assert_eq!(entries[0].name, "Foo 1");
        assert_eq!(entries[1].name, "Bar 1");
        assert_eq!(entries[2].name, "Foo 2");
        assert_eq!(entries[3].name, "Bar 2");
    }

    #[test]
    fn expand_globs_no_match_warns_but_no_error() {
        let dir = tempfile::tempdir().unwrap();

        let mut groups = HashMap::new();
        groups.insert("empty".to_string(), vec![PathBuf::from("*.xyz")]);

        let result = expand_group_globs(&groups, dir.path()).unwrap();
        assert!(result["empty"].is_empty());
    }
}
