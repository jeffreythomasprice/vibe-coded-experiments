//! End-to-end proof that `lib::sandbox`'s `bwrap` backend actually produces
//! the virtual filesystem `lib::vfs::MountTable` describes — not just that
//! the argv looks right (see `lib::sandbox::bwrap`'s unit tests for that).
//!
//! Unlike `live_anthropic`/`live_openai`/`live_ollama`, this file is **not**
//! gated on `AI_HARNESS_LIVE` — `bwrap` is a local binary that runs in
//! milliseconds and costs nothing, so gating it the same way would mean the
//! sandbox mechanism is never exercised by a plain `cargo test`. It follows
//! the same *spirit* as those tests instead: every test below calls
//! [`sandbox_backend`] first and returns immediately (printing why) if
//! `bwrap` isn't actually usable here, so a machine without a working
//! unprivileged sandbox reads as "skipped", not "broken build".
//!
//! Each test reproduces one empirically-verified claim from
//! `lib::sandbox`'s module doc against a real spawned `bwrap` process.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lib::config::SandboxConfig;
use lib::sandbox::{detect, SandboxBackend, SandboxSpec};
use lib::vfs::{Mount, MountTable};
use shared::project::AccessMode;

/// Prints why, and returns `None`, unless `bwrap` passes a real functional
/// probe (see `lib::sandbox::bwrap::probe`) — mirroring
/// `lib/tests/common::skip_unless_live`'s "skip, don't fail" convention.
fn sandbox_backend(test_name: &str) -> Option<Arc<dyn SandboxBackend>> {
    let (backend, availability) = detect(&SandboxConfig::default());
    if !availability.is_available() {
        eprintln!("skipping {test_name}: no working sandbox backend ({availability:?})");
        return None;
    }
    Some(backend)
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Unlike the `{tag}-{pid}-{:?}ThreadId` pattern used elsewhere in this
    /// workspace (e.g. `lib::db::testing::TempDb`), this doesn't fold in
    /// `ThreadId`'s `Debug` output: it renders as `ThreadId(1)`, and every
    /// test here interpolates this path — unquoted, deliberately, to catch
    /// exactly this kind of thing — straight into a `/bin/sh -c` script. A
    /// literal `(` there is a shell syntax error, not a path character. An
    /// atomic counter gives the same per-thread uniqueness without it.
    fn new(tag: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ai-harness-sandbox-bwrap-test-{tag}-{}-{n}",
            std::process::id(),
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn join(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn run(backend: &dyn SandboxBackend, table: MountTable, script: &str) -> std::process::Output {
    let spec = SandboxSpec {
        table,
        program: "/bin/sh".into(),
        args: vec!["-c".into(), script.into()],
        cwd: None,
        network: false,
    };
    backend
        .command(&spec)
        .expect("command() only fails when the backend itself is unavailable")
        .output()
        .expect("spawning bwrap")
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// The scenario from `lib::vfs`'s module doc: real `/a/b/c.txt`, `/d`,
/// `/e/f.txt`; only `/a/b` and `/d` are project directories. The virtual
/// filesystem must contain exactly `/a/b/c.txt` and `/d` (still empty); `/a`
/// is a synthetic scaffold listing only `b`, and `/e` doesn't exist at all.
#[test]
fn scaffold_directories_and_missing_paths_match_the_virtual_filesystem_exactly() {
    let Some(backend) = sandbox_backend("scaffold_directories_and_missing_paths_match_the_virtual_filesystem_exactly")
    else {
        return;
    };
    let tmp = TempDir::new("scaffold");
    std::fs::create_dir_all(tmp.join("a/b")).unwrap();
    std::fs::create_dir_all(tmp.join("d")).unwrap();
    std::fs::create_dir_all(tmp.join("e")).unwrap();
    std::fs::write(tmp.join("a/b/c.txt"), b"hello sandbox").unwrap();
    std::fs::write(tmp.join("e/f.txt"), b"top secret").unwrap();

    let table = MountTable::from_mounts(vec![
        Mount {
            root: tmp.join("a/b"),
            mode: AccessMode::ReadWrite,
        },
        Mount {
            root: tmp.join("d"),
            mode: AccessMode::ReadWrite,
        },
    ]);

    let real = tmp.path.display().to_string();
    let script = format!(
        "ls {real}/a; echo ---; ls {real}; echo ---; ls {real}/e >/dev/null 2>&1 && echo VISIBLE || echo ABSENT; echo ---; cat {real}/a/b/c.txt"
    );
    let output = run(backend.as_ref(), table, &script);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    let mut sections = out.split("---\n");
    assert_eq!(sections.next().unwrap().trim(), "b", "the scaffold at /a must list only its mounted child");
    let root_listing: Vec<&str> = sections.next().unwrap().split_whitespace().collect();
    assert_eq!(root_listing, vec!["a", "d"], "the root scaffold must list only its mounted children");
    assert_eq!(sections.next().unwrap().trim(), "ABSENT", "/e has no path to a mount and must not exist");
    assert_eq!(sections.next().unwrap().trim(), "hello sandbox");
}

#[test]
fn an_empty_mounted_directory_is_preserved() {
    let Some(backend) = sandbox_backend("an_empty_mounted_directory_is_preserved") else {
        return;
    };
    let tmp = TempDir::new("empty-dir");
    std::fs::create_dir_all(tmp.join("d")).unwrap();
    let table = MountTable::from_mounts(vec![Mount {
        root: tmp.join("d"),
        mode: AccessMode::ReadWrite,
    }]);
    let script = format!("ls -A {}", tmp.join("d").display());
    let output = run(backend.as_ref(), table, &script);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert_eq!(stdout(&output).trim(), "", "an empty mounted directory must list as empty, not absent");
}

#[test]
fn a_symlink_inside_a_mount_pointing_outside_it_is_structurally_dead() {
    let Some(backend) = sandbox_backend("a_symlink_inside_a_mount_pointing_outside_it_is_structurally_dead") else {
        return;
    };
    let tmp = TempDir::new("symlink-escape");
    std::fs::create_dir_all(tmp.join("a/b")).unwrap();
    std::fs::create_dir_all(tmp.join("e")).unwrap();
    std::fs::write(tmp.join("e/f.txt"), b"top secret").unwrap();
    std::os::unix::fs::symlink(tmp.join("e"), tmp.join("a/b/escape")).unwrap();

    let table = MountTable::from_mounts(vec![Mount {
        root: tmp.join("a/b"),
        mode: AccessMode::ReadWrite,
    }]);
    let script = format!("cat {}", tmp.join("a/b/escape/f.txt").display());
    let output = run(backend.as_ref(), table, &script);
    assert!(!output.status.success(), "reading through the escaping symlink must fail");
    assert!(
        stderr(&output).contains("No such file"),
        "expected a plain not-found error, not a permissions error — the target is structurally absent, not merely forbidden: {}",
        stderr(&output)
    );
}

#[test]
fn a_read_only_mount_rejects_writes() {
    let Some(backend) = sandbox_backend("a_read_only_mount_rejects_writes") else {
        return;
    };
    let tmp = TempDir::new("read-only");
    std::fs::create_dir_all(tmp.join("a/b")).unwrap();
    let table = MountTable::from_mounts(vec![Mount {
        root: tmp.join("a/b"),
        mode: AccessMode::ReadOnly,
    }]);
    let script = format!("echo x > {}", tmp.join("a/b/new.txt").display());
    let output = run(backend.as_ref(), table, &script);
    assert!(!output.status.success(), "a write into a read-only mount must fail");
    assert!(stderr(&output).contains("Read-only"), "stderr: {}", stderr(&output));
    assert!(!tmp.join("a/b/new.txt").exists());
}

/// Mirrors the manually-verified `bwrap` layering: `--ro-bind /a` then
/// `--bind /a/b/out` makes `/a` read-only, `/a/b/out` writable — and,
/// because `MountTable` sorts parents before children, the layering comes
/// out right regardless of the order directories were configured in.
#[test]
fn a_read_write_mount_nested_in_a_read_only_one_layers_correctly() {
    let Some(backend) = sandbox_backend("a_read_write_mount_nested_in_a_read_only_one_layers_correctly") else {
        return;
    };
    let tmp = TempDir::new("nested-layering");
    std::fs::create_dir_all(tmp.join("a/b/out")).unwrap();
    let table = MountTable::from_mounts(vec![
        // Deliberately given child-before-parent to prove the table's own
        // sort — not caller discipline — is what makes the layering work.
        Mount {
            root: tmp.join("a/b/out"),
            mode: AccessMode::ReadWrite,
        },
        Mount {
            root: tmp.join("a"),
            mode: AccessMode::ReadOnly,
        },
    ]);
    let real = tmp.path.display().to_string();
    let script = format!(
        "(echo x > {real}/a/nope.txt) 2>&1 | grep -q 'Read-only' && echo PARENT_RO || echo PARENT_WRITABLE; \
         (echo ok > {real}/a/b/out/file.txt && echo CHILD_RW) || echo CHILD_READONLY"
    );
    let output = run(backend.as_ref(), table, &script);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("PARENT_RO"), "the read-only parent must reject writes: {out}");
    assert!(out.contains("CHILD_RW"), "the nested read-write mount must accept writes: {out}");
    assert_eq!(
        std::fs::read_to_string(tmp.join("a/b/out/file.txt")).unwrap().trim(),
        "ok"
    );
}

#[test]
fn a_missing_project_directory_fails_the_sandbox_instead_of_silently_omitting_it() {
    let Some(backend) = sandbox_backend("a_missing_project_directory_fails_the_sandbox_instead_of_silently_omitting_it")
    else {
        return;
    };
    let tmp = TempDir::new("missing-dir");
    // Built directly with `from_mounts`, bypassing `MountTable::build`'s own
    // existence check, so this test exercises what `bwrap` itself does with
    // a bind source that isn't there.
    let table = MountTable::from_mounts(vec![Mount {
        root: tmp.join("does-not-exist"),
        mode: AccessMode::ReadWrite,
    }]);
    let output = run(backend.as_ref(), table, "echo should-not-run");
    assert!(!output.status.success(), "bwrap must refuse to start rather than silently drop the mount");
    assert!(!stdout(&output).contains("should-not-run"));
}

#[test]
fn a_write_into_a_scaffold_directory_never_reaches_the_host() {
    let Some(backend) = sandbox_backend("a_write_into_a_scaffold_directory_never_reaches_the_host") else {
        return;
    };
    let tmp = TempDir::new("scaffold-write");
    std::fs::create_dir_all(tmp.join("a/b")).unwrap();
    let table = MountTable::from_mounts(vec![Mount {
        root: tmp.join("a/b"),
        mode: AccessMode::ReadWrite,
    }]);
    let script = format!("mkdir {}/sneaky && echo CREATED", tmp.join("a").display());
    let output = run(backend.as_ref(), table, &script);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("CREATED"), "the write succeeds inside the sandbox (ephemeral tmpfs)");
    assert!(
        !Path::new(&tmp.join("a/sneaky")).exists(),
        "but must never appear on the host once the sandbox exits"
    );
}
