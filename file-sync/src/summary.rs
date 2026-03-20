use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CopyRecord {
    pub src: PathBuf,
    pub dst: PathBuf,
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct ConflictRecord {
    pub entity: String,
    pub winner: PathBuf,
    pub losers: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct BackupRecord {
    pub original: PathBuf,
    pub backup: PathBuf,
    pub dry_run: bool,
}

#[derive(Debug, Default)]
pub struct Summary {
    pub copies: Vec<CopyRecord>,
    pub backups: Vec<BackupRecord>,
    pub conflicts: Vec<ConflictRecord>,
    pub errors: Vec<String>,
    pub skipped: Vec<String>,
    pub already_in_sync: u32,
}

impl Summary {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_copy(&mut self, src: &Path, dst: &Path, dry_run: bool) {
        self.copies.push(CopyRecord {
            src: src.to_path_buf(),
            dst: dst.to_path_buf(),
            dry_run,
        });
    }

    pub fn record_backup(&mut self, original: &Path, backup: &Path, dry_run: bool) {
        self.backups.push(BackupRecord {
            original: original.to_path_buf(),
            backup: backup.to_path_buf(),
            dry_run,
        });
    }

    pub fn record_conflict(&mut self, entity: &str, winner: &Path, losers: Vec<PathBuf>) {
        self.conflicts.push(ConflictRecord {
            entity: entity.to_string(),
            winner: winner.to_path_buf(),
            losers,
        });
    }

    pub fn record_error(&mut self, msg: String) {
        self.errors.push(msg);
    }

    pub fn record_skip(&mut self, reason: String) {
        self.skipped.push(reason);
    }

    pub fn record_in_sync(&mut self) {
        self.already_in_sync += 1;
    }

    pub fn print(&self) {
        println!("\n--- file-sync summary ---");

        if !self.backups.is_empty() {
            let dry_backups: Vec<_> = self.backups.iter().filter(|b| b.dry_run).collect();
            let real_backups: Vec<_> = self.backups.iter().filter(|b| !b.dry_run).collect();

            if !real_backups.is_empty() {
                println!("Backups: {}", real_backups.len());
                for b in &real_backups {
                    println!("  {} -> {}", b.original.display(), b.backup.display());
                }
            }
            if !dry_backups.is_empty() {
                println!("Would back up (dry-run): {}", dry_backups.len());
                for b in &dry_backups {
                    println!("  {} -> {}", b.original.display(), b.backup.display());
                }
            }
        }

        if !self.copies.is_empty() {
            let dry_copies: Vec<_> = self.copies.iter().filter(|c| c.dry_run).collect();
            let real_copies: Vec<_> = self.copies.iter().filter(|c| !c.dry_run).collect();

            if !real_copies.is_empty() {
                println!("Copies: {}", real_copies.len());
                for c in &real_copies {
                    println!("  {} -> {}", c.src.display(), c.dst.display());
                }
            }
            if !dry_copies.is_empty() {
                println!("Would copy (dry-run): {}", dry_copies.len());
                for c in &dry_copies {
                    println!("  {} -> {}", c.src.display(), c.dst.display());
                }
            }
        }

        if !self.conflicts.is_empty() {
            println!("Conflicts resolved: {}", self.conflicts.len());
            for c in &self.conflicts {
                println!(
                    "  [{}] Kept: {} (overwrote {} other(s))",
                    c.entity,
                    c.winner.display(),
                    c.losers.len()
                );
            }
        }

        if self.already_in_sync > 0 {
            println!("Already in sync: {}", self.already_in_sync);
        }

        if !self.skipped.is_empty() {
            println!("Skipped: {}", self.skipped.len());
            for s in &self.skipped {
                println!("  {}", s);
            }
        }

        if !self.errors.is_empty() {
            println!("Errors: {}", self.errors.len());
            for e in &self.errors {
                println!("  {}", e);
            }
        }

        if self.copies.is_empty()
            && self.backups.is_empty()
            && self.conflicts.is_empty()
            && self.skipped.is_empty()
            && self.errors.is_empty()
            && self.already_in_sync == 0
        {
            println!("Everything in sync, nothing to do.");
        }

        println!("-------------------------");
    }
}
