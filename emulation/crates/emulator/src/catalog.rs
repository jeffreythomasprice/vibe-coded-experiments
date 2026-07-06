//! In-memory catalog of the ROMs available in the configured `roms_dir`.
//!
//! Built once at startup by scanning the [`RomLibrary`] and parsing each ROM's
//! header (the cheap [`CartridgeHeader::parse`] path — no full [`Cartridge`]).
//! Files that fail to read or parse are logged and skipped rather than aborting
//! startup. The result feeds `run <name>` lookups today and will feed the
//! not-yet-built menu UI later, which is why the whole per-ROM metadata is kept
//! together in one struct.

use common::{RomId, RomLibrary};
use gameboy::CartridgeHeader;

use crate::storage::FileStore;

/// Everything we know about one ROM in the library without keeping its bytes:
/// its store identity, the display-friendly file stem, and the parsed header.
#[derive(Debug)]
pub struct RomInfo {
    /// Full file name (e.g. `game.gb`) — the [`RomId`] used to re-read the ROM.
    pub id: RomId,
    /// File name without extension (e.g. `game`) — for display and name matching.
    pub file_stem: String,
    /// Parsed cartridge header (title, type, sizes, CGB flag, ...).
    pub header: CartridgeHeader,
}

impl RomInfo {
    /// The header title, trimmed. Empty for many homebrew/test ROMs.
    pub fn title(&self) -> &str {
        self.header.title.trim()
    }
}

/// The set of parsed ROMs discovered in `roms_dir`.
pub struct RomCatalog {
    roms: Vec<RomInfo>,
}

impl RomCatalog {
    /// Scan the library, reading and parsing every ROM's header. Reads and parse
    /// failures are warned about and skipped; the ROM bytes are dropped once the
    /// header is parsed so only the lightweight headers stay resident.
    pub fn load(store: &FileStore) -> RomCatalog {
        let entries = match store.list_roms() {
            Ok(entries) => entries,
            Err(err) => {
                tracing::warn!(%err, "failed to list roms; catalog is empty");
                return RomCatalog { roms: Vec::new() };
            }
        };

        let mut roms = Vec::new();
        for entry in entries {
            let bytes = match store.read_rom(&entry.id) {
                Ok(bytes) => bytes,
                Err(err) => {
                    tracing::warn!(rom = %entry.id.0, %err, "failed to read rom; skipping");
                    continue;
                }
            };
            match CartridgeHeader::parse(&bytes) {
                Ok(header) => roms.push(RomInfo {
                    id: entry.id,
                    file_stem: entry.name,
                    header,
                }),
                Err(err) => {
                    tracing::warn!(rom = %entry.id.0, %err, "failed to parse rom header; skipping")
                }
            }
        }

        roms.sort_by(|a, b| a.file_stem.cmp(&b.file_stem));
        tracing::info!(count = roms.len(), "loaded rom catalog");
        RomCatalog { roms }
    }

    pub fn roms(&self) -> &[RomInfo] {
        &self.roms
    }

    /// Find a ROM by name, matching `query` case-insensitively against the file
    /// name (with or without extension) or the header title.
    ///
    /// Returns `Ok(None)` when nothing matches and `Err(candidates)` when more
    /// than one ROM matches, so the caller can report an ambiguous query.
    pub fn find(&self, query: &str) -> Result<Option<&RomInfo>, Vec<&RomInfo>> {
        let query = query.trim();
        let matches: Vec<&RomInfo> = self
            .roms
            .iter()
            .filter(|info| {
                query.eq_ignore_ascii_case(&info.id.0)
                    || query.eq_ignore_ascii_case(&info.file_stem)
                    || query.eq_ignore_ascii_case(info.title())
            })
            .collect();

        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0])),
            _ => Err(matches),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid ROM whose header carries `title`. The header parser
    /// only needs the first `0x0150` bytes; everything else can be zero.
    fn rom_with_title(title: &str) -> Vec<u8> {
        let mut rom = vec![0u8; 0x0150];
        let bytes = title.as_bytes();
        rom[0x0134..0x0134 + bytes.len()].copy_from_slice(bytes);
        rom
    }

    fn info(file_name: &str, title: &str) -> RomInfo {
        let header = CartridgeHeader::parse(&rom_with_title(title)).expect("valid header");
        let stem = file_name
            .rsplit_once('.')
            .map_or(file_name, |(stem, _)| stem);
        RomInfo {
            id: RomId(file_name.to_string()),
            file_stem: stem.to_string(),
            header,
        }
    }

    fn catalog(roms: Vec<RomInfo>) -> RomCatalog {
        RomCatalog { roms }
    }

    #[test]
    fn matches_file_stem_case_insensitively() {
        let cat = catalog(vec![info("Tetris.gb", "TETRIS")]);
        let found = cat.find("tetris").expect("not ambiguous").expect("found");
        assert_eq!(found.id.0, "Tetris.gb");
    }

    #[test]
    fn matches_full_file_name_and_header_title() {
        let cat = catalog(vec![info("zelda.gb", "LEGEND OF ZELDA")]);
        assert!(cat.find("zelda.gb").unwrap().is_some());
        assert!(cat.find("legend of zelda").unwrap().is_some());
    }

    #[test]
    fn no_match_returns_none() {
        let cat = catalog(vec![info("tetris.gb", "TETRIS")]);
        assert!(cat.find("mario").expect("not ambiguous").is_none());
    }

    #[test]
    fn multiple_matches_are_ambiguous() {
        // Two different files that share the same header title.
        let cat = catalog(vec![
            info("dump-a.gb", "SAME TITLE"),
            info("dump-b.gb", "SAME TITLE"),
        ]);
        let err = cat.find("same title").expect_err("ambiguous");
        assert_eq!(err.len(), 2);
    }
}
