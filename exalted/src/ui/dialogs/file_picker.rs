//! Thin wrappers over `rfd::FileDialog`. The sync API is fine for our
//! use case: the OS dialog is modal anyway, and a brief blocking call is
//! simpler than wiring async into the egui main loop.

use std::path::{Path, PathBuf};

pub fn open_character(start_dir: Option<&Path>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .add_filter("Exalted character (TOML)", &["toml"])
        .add_filter("All files", &["*"]);
    if let Some(p) = start_dir {
        d = d.set_directory(p);
    }
    d.pick_file()
}

pub fn save_character(start_dir: Option<&Path>, suggested_name: Option<&str>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .add_filter("Exalted character (TOML)", &["toml"])
        .add_filter("All files", &["*"]);
    if let Some(p) = start_dir {
        d = d.set_directory(p);
    }
    if let Some(name) = suggested_name {
        d = d.set_file_name(name);
    }
    d.save_file()
}

pub fn export_pdf(start_dir: Option<&Path>, suggested_name: Option<&str>) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new()
        .add_filter("PDF", &["pdf"])
        .add_filter("All files", &["*"]);
    if let Some(p) = start_dir {
        d = d.set_directory(p);
    }
    if let Some(name) = suggested_name {
        d = d.set_file_name(name);
    }
    d.save_file()
}
