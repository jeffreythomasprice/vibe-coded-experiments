//! The `bubblewrap` (`bwrap`) backend: an unprivileged Linux sandbox built on
//! user, mount, and (optionally) network namespaces. See `super`'s module
//! doc for the empirical basis of the argv this builds.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use shared::project::AccessMode;

use super::{Availability, SandboxBackend, SandboxError, SandboxSpec};

/// The `bwrap`-backed [`SandboxBackend`].
pub struct Bubblewrap {
    bwrap_path: PathBuf,
    system_paths: Vec<PathBuf>,
}

impl Bubblewrap {
    pub fn new(bwrap_path: PathBuf, system_paths: Vec<PathBuf>) -> Self {
        Self {
            bwrap_path,
            system_paths,
        }
    }
}

impl SandboxBackend for Bubblewrap {
    fn name(&self) -> &'static str {
        "bubblewrap"
    }

    fn command(&self, spec: &SandboxSpec) -> Result<std::process::Command, SandboxError> {
        let argv = bwrap_argv(&self.bwrap_path, &self.system_paths, spec);
        let mut command = std::process::Command::new(&argv[0]);
        command.args(&argv[1..]);
        Ok(command)
    }
}

/// Build the full `bwrap` argv for `spec`. Pure — touches neither the
/// filesystem nor a process — so this is the whole confinement policy as a
/// value a test can assert on.
///
/// Order, each point verified against a real `bwrap` run (see `super`'s
/// module doc): `--unshare-all` (plus `--share-net` only when
/// `spec.network`) isolates everything up front; `--clearenv` then strips
/// the inherited environment — mandatory, since `bwrap` does not clear it on
/// its own, and this server process's `ANTHROPIC_API_KEY`/`OPENAI_API_KEY`
/// must never reach a sandboxed child; `system_paths` are bound read-only
/// (`--ro-bind-try`, so a missing one is skipped rather than failing);
/// finally the project's own mounts, in [`crate::vfs::MountTable`]'s
/// already-parent-before-child order, `--bind` for read-write and
/// `--ro-bind` for read-only.
pub fn bwrap_argv(bwrap_path: &Path, system_paths: &[PathBuf], spec: &SandboxSpec) -> Vec<OsString> {
    fn s(text: &str) -> OsString {
        OsString::from(text)
    }

    let mut argv: Vec<OsString> = vec![bwrap_path.as_os_str().to_owned()];

    argv.push(s("--die-with-parent"));
    argv.push(s("--new-session"));
    argv.push(s("--unshare-all"));
    if spec.network {
        argv.push(s("--share-net"));
    }
    argv.push(s("--clearenv"));
    argv.push(s("--proc"));
    argv.push(s("/proc"));
    argv.push(s("--dev"));
    argv.push(s("/dev"));
    argv.push(s("--tmpfs"));
    argv.push(s("/tmp"));

    for path in system_paths {
        argv.push(s("--ro-bind-try"));
        argv.push(path.as_os_str().to_owned());
        argv.push(path.as_os_str().to_owned());
    }

    for mount in spec.table.mounts() {
        argv.push(s(if mount.mode == AccessMode::ReadOnly {
            "--ro-bind"
        } else {
            "--bind"
        }));
        argv.push(mount.root.as_os_str().to_owned());
        argv.push(mount.root.as_os_str().to_owned());
    }

    argv.push(s("--setenv"));
    argv.push(s("PATH"));
    argv.push(s("/usr/bin:/bin"));
    argv.push(s("--setenv"));
    argv.push(s("HOME"));
    argv.push(s("/tmp"));

    if let Some(cwd) = &spec.cwd {
        argv.push(s("--chdir"));
        argv.push(cwd.as_os_str().to_owned());
    }

    argv.push(s("--"));
    argv.push(spec.program.clone());
    argv.extend(spec.args.iter().cloned());
    argv
}

/// Whether `bwrap` at `bwrap_path` actually works here: a real, trivial
/// sandboxed exec, not `bwrap --version` — see `super`'s module doc for why
/// the version string alone is not proof (Ubuntu 24.04's
/// `apparmor_restrict_unprivileged_userns` can let `bwrap` run at all while
/// still rejecting `--unshare-user` at exec time).
pub fn probe(bwrap_path: &Path, system_paths: &[PathBuf]) -> Availability {
    let mut command = std::process::Command::new(bwrap_path);
    command
        .args(["--die-with-parent", "--unshare-all", "--proc", "/proc", "--dev", "/dev"]);
    for path in system_paths {
        if path.exists() {
            command.arg("--ro-bind-try").arg(path).arg(path);
        }
    }
    command.args(["--", "/bin/true"]);

    match command.output() {
        Ok(output) if output.status.success() => Availability::Available,
        Ok(output) => Availability::Unavailable {
            reason: first_line(&output.stderr).unwrap_or_else(|| format!("bwrap exited with {}", output.status)),
        },
        Err(source) => Availability::Unavailable {
            reason: format!("failed to execute {}: {source}", bwrap_path.display()),
        },
    }
}

fn first_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().next()?.trim();
    if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::{Mount, MountTable};
    use std::ffi::OsStr;

    fn os(s: &str) -> OsString {
        OsString::from(s)
    }

    fn mount(path: &str, mode: AccessMode) -> Mount {
        Mount {
            root: PathBuf::from(path),
            mode,
        }
    }

    fn spec(table: MountTable, network: bool) -> SandboxSpec {
        SandboxSpec {
            table,
            program: os("/bin/sh"),
            args: vec![os("-c"), os("echo hi")],
            cwd: None,
            network,
        }
    }

    fn find(argv: &[OsString], flag: &str) -> Vec<usize> {
        argv.iter()
            .enumerate()
            .filter(|(_, a)| a.as_os_str() == OsStr::new(flag))
            .map(|(i, _)| i)
            .collect()
    }

    #[test]
    fn argv_starts_with_the_configured_bwrap_binary() {
        let table = MountTable::default();
        let argv = bwrap_argv(Path::new("/usr/bin/bwrap"), &[], &spec(table, true));
        assert_eq!(argv[0], os("/usr/bin/bwrap"));
    }

    #[test]
    fn network_flag_is_share_net_only_when_requested() {
        let table = MountTable::default();
        let with_net = bwrap_argv(Path::new("bwrap"), &[], &spec(table.clone(), true));
        assert!(find(&with_net, "--share-net").len() == 1);

        let without_net = bwrap_argv(Path::new("bwrap"), &[], &spec(table, false));
        assert!(find(&without_net, "--share-net").is_empty());
    }

    #[test]
    fn clearenv_is_always_present() {
        let argv = bwrap_argv(Path::new("bwrap"), &[], &spec(MountTable::default(), true));
        assert_eq!(find(&argv, "--clearenv").len(), 1);
        // The point of --clearenv: nothing named after a provider API key
        // ever appears in the argv itself.
        assert!(!argv.iter().any(|a| a.to_string_lossy().contains("API_KEY")));
    }

    #[test]
    fn system_paths_are_ro_bind_try_so_a_missing_one_does_not_fail_the_sandbox() {
        let system_paths = vec![PathBuf::from("/usr"), PathBuf::from("/bin")];
        let argv = bwrap_argv(Path::new("bwrap"), &system_paths, &spec(MountTable::default(), true));
        let ro_bind_try_positions = find(&argv, "--ro-bind-try");
        assert_eq!(ro_bind_try_positions.len(), 2);
        assert_eq!(argv[ro_bind_try_positions[0] + 1], os("/usr"));
        assert_eq!(argv[ro_bind_try_positions[0] + 2], os("/usr"));
    }

    #[test]
    fn read_only_mounts_use_ro_bind_and_read_write_mounts_use_bind() {
        let table = MountTable::from_mounts(vec![
            mount("/d", AccessMode::ReadWrite),
            mount("/a/b", AccessMode::ReadOnly),
        ]);
        let argv = bwrap_argv(Path::new("bwrap"), &[], &spec(table, true));
        let bind = find(&argv, "--bind");
        let ro_bind = find(&argv, "--ro-bind");
        assert_eq!(argv[bind[0] + 1], os("/d"));
        assert_eq!(argv[ro_bind[0] + 1], os("/a/b"));
    }

    #[test]
    fn mounts_appear_in_the_mount_tables_parent_before_child_order() {
        // MountTable already guarantees this ordering (see its own tests);
        // this asserts bwrap_argv doesn't re-shuffle it.
        let table = MountTable::from_mounts(vec![
            mount("/a/b", AccessMode::ReadWrite),
            mount("/a", AccessMode::ReadOnly),
        ]);
        let argv = bwrap_argv(Path::new("bwrap"), &[], &spec(table, true));
        let a_pos = argv.iter().position(|a| a == &os("/a")).unwrap();
        let a_b_pos = argv.iter().position(|a| a == &os("/a/b")).unwrap();
        assert!(a_pos < a_b_pos, "argv: {argv:?}");
    }

    #[test]
    fn an_empty_mount_table_adds_no_project_binds() {
        let argv = bwrap_argv(Path::new("bwrap"), &[], &spec(MountTable::default(), true));
        assert!(find(&argv, "--bind").is_empty());
        assert!(find(&argv, "--ro-bind").is_empty());
    }

    #[test]
    fn program_and_args_follow_a_double_dash() {
        let argv = bwrap_argv(Path::new("bwrap"), &[], &spec(MountTable::default(), true));
        let dash_dash = argv.iter().position(|a| a == &os("--")).unwrap();
        assert_eq!(argv[dash_dash + 1], os("/bin/sh"));
        assert_eq!(argv[dash_dash + 2], os("-c"));
        assert_eq!(argv[dash_dash + 3], os("echo hi"));
    }

    #[test]
    fn cwd_is_only_set_when_given() {
        let mut s = spec(MountTable::default(), true);
        assert!(find(&bwrap_argv(Path::new("bwrap"), &[], &s), "--chdir").is_empty());
        s.cwd = Some(PathBuf::from("/a/b"));
        let argv = bwrap_argv(Path::new("bwrap"), &[], &s);
        let chdir = find(&argv, "--chdir");
        assert_eq!(chdir.len(), 1);
        assert_eq!(argv[chdir[0] + 1], os("/a/b"));
    }
}
