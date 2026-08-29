//! The filesystem tool suite: read/list/stat (no approval) plus
//! write/edit/mkdir/delete (approval required), all backed by
//! [`crate::vfs::Vfs`] against the calling conversation's project mounts.
//!
//! Every [`crate::vfs::VfsError`] surfaces as an ordinary `Err`, which the
//! agent loop turns into an `is_error` tool result the model can read and
//! react to (a bad path, a read-only mount, a missing file) — never a turn
//! failure.

use std::path::Path;

use anyhow::bail;
use serde::Deserialize;

use crate::agent::tool::{ctx_tool, JsonSchema, ToolContext, ToolOutput};
use crate::agent::ToolRegistry;

/// Guidance shared by every filesystem tool: paths are real absolute paths
/// (never relative to some hidden working directory), and only the paths
/// under the conversation's project are visible at all — anything else
/// reads as "outside every project directory", not "not found".
const SHARED_GUIDANCE: &str =
    "Paths are real absolute paths (e.g. `/home/user/project/src/main.rs`), not relative to \
     any hidden working directory. Only the directories granted to this conversation's project \
     are visible; a path outside all of them fails with \"outside every project directory\", \
     not \"not found\" — that distinction tells you whether the path is wrong or the project \
     doesn't grant it.";

pub fn register(registry: &mut ToolRegistry) {
    registry.register(read_file_tool());
    registry.register(list_directory_tool());
    registry.register(file_info_tool());
    registry.register(write_file_tool());
    registry.register(edit_file_tool());
    registry.register(create_directory_tool());
    registry.register(delete_file_tool());
}

// --- read_file ---

#[derive(Debug, Deserialize, JsonSchema)]
struct ReadFileArgs {
    /// Absolute path to the file to read.
    path: String,
    /// 1-based line number to start reading from. Omit to start at the
    /// beginning of the file.
    #[serde(default)]
    offset: Option<u32>,
    /// Maximum number of lines to return, starting at `offset`. Omit to read
    /// through the end of the file.
    #[serde(default)]
    limit: Option<u32>,
}

fn read_file_tool() -> crate::agent::FnTool {
    ctx_tool(
        "read_file",
        "Read a text file from the project, optionally a line range of it.",
        read_file,
    )
    .with_system_prompt(format!(
        "Reads a UTF-8 text file. {SHARED_GUIDANCE} Use `offset`/`limit` to read a slice of a \
         large file instead of the whole thing. A file larger than this build's configured \
         limit is refused outright — even with `offset`/`limit` — since nothing here can \
         partially read a file without first checking its size."
    ))
}

async fn read_file(ctx: ToolContext, args: ReadFileArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        let path = Path::new(&args.path);
        // Checked before `read_to_string`, which loads the whole file into
        // memory first — a size cap applied only after that read would
        // already have paid the cost it exists to avoid.
        let meta = ctx.vfs().metadata(path)?;
        let limit_bytes = ctx.limits().max_read_bytes as u64;
        if meta.len > limit_bytes {
            bail!(
                "{} is {} bytes, over the {limit_bytes}-byte limit for read_file",
                args.path,
                meta.len
            );
        }
        let contents = ctx.vfs().read_to_string(path)?;
        Ok(ToolOutput::text(slice_lines(&contents, args.offset, args.limit)))
    })
    .await
}

fn slice_lines(text: &str, offset: Option<u32>, limit: Option<u32>) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = offset.unwrap_or(1).max(1) as usize - 1;
    if start >= lines.len() {
        return String::new();
    }
    let end = match limit {
        Some(n) => (start + n as usize).min(lines.len()),
        None => lines.len(),
    };
    lines[start..end].join("\n")
}

// --- list_directory ---

#[derive(Debug, Deserialize, JsonSchema)]
struct ListDirectoryArgs {
    /// Absolute path to the directory to list.
    path: String,
}

fn list_directory_tool() -> crate::agent::FnTool {
    ctx_tool(
        "list_directory",
        "List the immediate contents of a directory in the project.",
        list_directory,
    )
    .with_system_prompt(format!(
        "Lists one directory's immediate children — not recursive. Each entry ends with `/` \
         for a directory, `@` for a symlink, or nothing for a plain file. {SHARED_GUIDANCE} \
         An ancestor of the project's own directories (say, `/home/user` when only \
         `/home/user/project` is granted) lists only the path down to what's granted — never \
         its real siblings — so a sparse listing there means the project is scoped narrowly, \
         not that the project is empty or broken."
    ))
}

async fn list_directory(ctx: ToolContext, args: ListDirectoryArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        let entries = ctx.vfs().read_dir(Path::new(&args.path))?;
        if entries.is_empty() {
            return Ok(ToolOutput::text("(empty directory)"));
        }
        let listing = entries.iter().map(format_entry).collect::<Vec<_>>().join("\n");
        Ok(ToolOutput::text(listing))
    })
    .await
}

fn format_entry(entry: &crate::vfs::Entry) -> String {
    use crate::vfs::Entry;
    match entry {
        Entry::Dir(name) => format!("{name}/"),
        Entry::File(name) => name.clone(),
        Entry::Symlink(name) => format!("{name}@"),
        Entry::Other(name) => name.clone(),
    }
}

// --- file_info ---

#[derive(Debug, Deserialize, JsonSchema)]
struct FileInfoArgs {
    /// Absolute path to inspect.
    path: String,
}

fn file_info_tool() -> crate::agent::FnTool {
    ctx_tool(
        "file_info",
        "Check whether a path exists in the project, and whether it's a file or a directory.",
        file_info,
    )
    .with_system_prompt(SHARED_GUIDANCE.to_string())
}

async fn file_info(ctx: ToolContext, args: FileInfoArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        let meta = ctx.vfs().metadata(Path::new(&args.path))?;
        let kind = if meta.is_dir {
            "directory"
        } else if meta.is_file {
            "file"
        } else {
            "other"
        };
        Ok(ToolOutput::text(format!("kind: {kind}\nsize: {} bytes", meta.len)))
    })
    .await
}

// --- write_file ---

#[derive(Debug, Deserialize, JsonSchema)]
struct WriteFileArgs {
    /// Absolute path to create or fully overwrite.
    path: String,
    /// The complete file contents. Replaces the file entirely — this is not
    /// an append or a patch.
    contents: String,
}

fn write_file_tool() -> crate::agent::FnTool {
    ctx_tool(
        "write_file",
        "Create a file, or replace an existing one's contents entirely.",
        write_file,
    )
    .with_system_prompt(format!(
        "Writes the given contents to a file, creating it if absent and completely replacing \
         it if present — there is no append and no partial write. {SHARED_GUIDANCE} Prefer \
         `edit_file` for a small change to an existing file; reach for `write_file` when you \
         mean to replace the whole thing. Requires the user's approval before it runs."
    ))
    .requiring_approval()
}

async fn write_file(ctx: ToolContext, args: WriteFileArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        ctx.vfs().write(Path::new(&args.path), args.contents.as_bytes())?;
        Ok(ToolOutput::text(format!(
            "wrote {} bytes to {}",
            args.contents.len(),
            args.path
        )))
    })
    .await
}

// --- edit_file ---

#[derive(Debug, Deserialize, JsonSchema)]
struct EditFileArgs {
    /// Absolute path to the file to edit.
    path: String,
    /// The exact text to find. Must occur at least once in the file.
    old_string: String,
    /// The text to replace it with.
    new_string: String,
    /// Replace every occurrence instead of requiring there be exactly one.
    /// Defaults to false.
    #[serde(default)]
    replace_all: bool,
}

fn edit_file_tool() -> crate::agent::FnTool {
    ctx_tool(
        "edit_file",
        "Replace one exact string with another inside an existing file.",
        edit_file,
    )
    .with_system_prompt(format!(
        "Finds `old_string` in the file and replaces it with `new_string`. Fails if \
         `old_string` doesn't occur at all — nothing is silently skipped. Fails if it occurs \
         more than once and `replace_all` isn't set, rather than guessing which occurrence you \
         meant; either pass `replace_all: true` or make `old_string` more specific (include \
         more surrounding context) so it matches exactly once. {SHARED_GUIDANCE} Requires the \
         user's approval before it runs."
    ))
    .requiring_approval()
}

async fn edit_file(ctx: ToolContext, args: EditFileArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        let path = Path::new(&args.path);
        let contents = ctx.vfs().read_to_string(path)?;
        let occurrences = contents.matches(args.old_string.as_str()).count();
        if occurrences == 0 {
            bail!("old_string was not found in {}", args.path);
        }
        if occurrences > 1 && !args.replace_all {
            bail!(
                "old_string matches {occurrences} times in {} — pass replace_all: true, or give \
                 a more specific old_string that matches exactly once",
                args.path
            );
        }
        let updated = if args.replace_all {
            contents.replace(&args.old_string, &args.new_string)
        } else {
            contents.replacen(&args.old_string, &args.new_string, 1)
        };
        ctx.vfs().write(path, updated.as_bytes())?;
        let plural = if occurrences == 1 { "" } else { "s" };
        Ok(ToolOutput::text(format!(
            "replaced {occurrences} occurrence{plural} in {}",
            args.path
        )))
    })
    .await
}

// --- create_directory ---

#[derive(Debug, Deserialize, JsonSchema)]
struct CreateDirectoryArgs {
    /// Absolute path of the directory to create. Parent directories are
    /// created as needed, like `mkdir -p`.
    path: String,
}

fn create_directory_tool() -> crate::agent::FnTool {
    ctx_tool(
        "create_directory",
        "Create a directory in the project, including any missing parent directories.",
        create_directory,
    )
    .with_system_prompt(format!(
        "Creates a directory, and any missing parents, like `mkdir -p`. Already existing is not \
         an error. {SHARED_GUIDANCE} There is no matching tool to remove a directory — \
         `delete_file` only removes files. Requires the user's approval before it runs."
    ))
    .requiring_approval()
}

async fn create_directory(ctx: ToolContext, args: CreateDirectoryArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        ctx.vfs().create_dir_all(Path::new(&args.path))?;
        Ok(ToolOutput::text(format!("created {}", args.path)))
    })
    .await
}

// --- delete_file ---

#[derive(Debug, Deserialize, JsonSchema)]
struct DeleteFileArgs {
    /// Absolute path of the file to delete.
    path: String,
}

fn delete_file_tool() -> crate::agent::FnTool {
    ctx_tool("delete_file", "Delete a single file from the project.", delete_file)
        .with_system_prompt(format!(
            "Deletes exactly one file — not a directory, and not recursive. {SHARED_GUIDANCE} \
             There is no tool to remove a directory. Requires the user's approval before it runs."
        ))
        .requiring_approval()
}

async fn delete_file(ctx: ToolContext, args: DeleteFileArgs) -> anyhow::Result<ToolOutput> {
    super::blocking(move || {
        ctx.vfs().remove_file(Path::new(&args.path))?;
        Ok(ToolOutput::text(format!("deleted {}", args.path)))
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sandbox::Disabled;
    use crate::vfs::{Mount, MountTable, Vfs};
    use shared::agent::Approval;
    use shared::project::AccessMode;
    use std::path::PathBuf;
    use std::sync::Arc;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ai-harness-builtin-fs-test-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
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

    fn ctx_over(root: &Path, mode: AccessMode) -> ToolContext {
        let table = MountTable::from_mounts(vec![Mount {
            root: root.to_path_buf(),
            mode,
        }]);
        ToolContext::new(
            Vfs::new(table),
            Arc::new(Disabled::new("not needed for fs tests")),
            false,
            Default::default(),
        )
    }

    #[test]
    fn read_write_edit_and_delete_are_registered_with_the_expected_default_approval() {
        let mut registry = ToolRegistry::new();
        register(&mut registry);
        let catalog = registry.catalog();
        let by_name = |name: &str| catalog.iter().find(|t| t.def.name == name).unwrap();

        for no_approval in ["read_file", "list_directory", "file_info"] {
            assert_eq!(
                by_name(no_approval).approval,
                Approval::Automatic,
                "{no_approval} should not require approval"
            );
        }
        for gated in ["write_file", "edit_file", "create_directory", "delete_file"] {
            assert_eq!(
                by_name(gated).approval,
                Approval::RequiresApproval,
                "{gated} should require approval"
            );
        }
    }

    #[tokio::test]
    async fn write_then_read_round_trips() {
        let tmp = TempDir::new("rw");
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);
        let file = tmp.join("hello.txt");

        write_file(
            ctx.clone(),
            WriteFileArgs {
                path: file.to_string_lossy().into_owned(),
                contents: "hello sandbox".to_string(),
            },
        )
        .await
        .unwrap();

        let output = read_file(
            ctx,
            ReadFileArgs {
                path: file.to_string_lossy().into_owned(),
                offset: None,
                limit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(text_of(&output), "hello sandbox");
    }

    #[tokio::test]
    async fn write_into_a_read_only_mount_is_refused() {
        let tmp = TempDir::new("ro");
        let ctx = ctx_over(&tmp.path, AccessMode::ReadOnly);
        let err = write_file(
            ctx,
            WriteFileArgs {
                path: tmp.join("nope.txt").to_string_lossy().into_owned(),
                contents: "x".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("read-only"), "error was: {err}");
    }

    #[tokio::test]
    async fn read_file_on_a_path_outside_every_mount_names_it_as_such() {
        let tmp = TempDir::new("outside");
        std::fs::create_dir_all(tmp.join("mounted")).unwrap();
        std::fs::create_dir_all(tmp.join("other")).unwrap();
        std::fs::write(tmp.join("other/secret.txt"), b"nope").unwrap();
        let ctx = ctx_over(&tmp.join("mounted"), AccessMode::ReadWrite);

        let err = read_file(
            ctx,
            ReadFileArgs {
                path: tmp.join("other/secret.txt").to_string_lossy().into_owned(),
                offset: None,
                limit: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("outside every project directory"), "error was: {err}");
    }

    #[tokio::test]
    async fn read_file_rejects_a_file_over_the_configured_limit_before_reading_it() {
        let tmp = TempDir::new("oversized");
        let file = tmp.join("big.txt");
        std::fs::write(&file, vec![b'a'; 100]).unwrap();
        let table = MountTable::from_mounts(vec![Mount {
            root: tmp.path.clone(),
            mode: AccessMode::ReadWrite,
        }]);
        let ctx = ToolContext::new(
            Vfs::new(table),
            Arc::new(Disabled::new("not needed")),
            false,
            crate::agent::ToolLimits {
                max_read_bytes: 10,
                ..Default::default()
            },
        );

        let err = read_file(
            ctx,
            ReadFileArgs {
                path: file.to_string_lossy().into_owned(),
                offset: None,
                limit: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("100 bytes"), "error was: {err}");
        assert!(err.to_string().contains("10-byte limit"), "error was: {err}");
    }

    #[tokio::test]
    async fn read_file_offset_and_limit_slice_by_line() {
        let tmp = TempDir::new("lines");
        let file = tmp.join("lines.txt");
        std::fs::write(&file, "one\ntwo\nthree\nfour\n").unwrap();
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);

        let output = read_file(
            ctx,
            ReadFileArgs {
                path: file.to_string_lossy().into_owned(),
                offset: Some(2),
                limit: Some(2),
            },
        )
        .await
        .unwrap();
        assert_eq!(text_of(&output), "two\nthree");
    }

    #[tokio::test]
    async fn list_directory_on_a_scaffold_shows_only_the_mounted_child() {
        let tmp = TempDir::new("scaffold");
        std::fs::create_dir_all(tmp.join("a/b")).unwrap();
        std::fs::write(tmp.join("a/sibling.txt"), b"invisible").unwrap();
        let ctx = ctx_over(&tmp.join("a/b"), AccessMode::ReadWrite);

        let output = list_directory(
            ctx,
            ListDirectoryArgs {
                path: tmp.join("a").to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap();
        assert_eq!(text_of(&output), "b/");
    }

    #[tokio::test]
    async fn edit_file_errors_when_old_string_is_absent() {
        let tmp = TempDir::new("edit-absent");
        let file = tmp.join("f.txt");
        std::fs::write(&file, "hello world").unwrap();
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);

        let err = edit_file(
            ctx,
            EditFileArgs {
                path: file.to_string_lossy().into_owned(),
                old_string: "goodbye".to_string(),
                new_string: "hi".to_string(),
                replace_all: false,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("was not found"), "error was: {err}");
    }

    #[tokio::test]
    async fn edit_file_errors_when_old_string_is_ambiguous_without_replace_all() {
        let tmp = TempDir::new("edit-ambiguous");
        let file = tmp.join("f.txt");
        std::fs::write(&file, "a a a").unwrap();
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);

        let err = edit_file(
            ctx,
            EditFileArgs {
                path: file.to_string_lossy().into_owned(),
                old_string: "a".to_string(),
                new_string: "b".to_string(),
                replace_all: false,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("matches 3 times"), "error was: {err}");
    }

    #[tokio::test]
    async fn edit_file_replace_all_replaces_every_occurrence() {
        let tmp = TempDir::new("edit-all");
        let file = tmp.join("f.txt");
        std::fs::write(&file, "a a a").unwrap();
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);

        edit_file(
            ctx.clone(),
            EditFileArgs {
                path: file.to_string_lossy().into_owned(),
                old_string: "a".to_string(),
                new_string: "b".to_string(),
                replace_all: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "b b b");
    }

    #[tokio::test]
    async fn create_directory_then_file_info_reports_a_directory() {
        let tmp = TempDir::new("mkdir");
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);
        let nested = tmp.join("a/b/c");

        create_directory(
            ctx.clone(),
            CreateDirectoryArgs {
                path: nested.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap();

        let output = file_info(
            ctx,
            FileInfoArgs {
                path: nested.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap();
        assert!(text_of(&output).starts_with("kind: directory"));
    }

    #[tokio::test]
    async fn delete_file_removes_it() {
        let tmp = TempDir::new("delete");
        let file = tmp.join("gone.txt");
        std::fs::write(&file, "x").unwrap();
        let ctx = ctx_over(&tmp.path, AccessMode::ReadWrite);

        delete_file(
            ctx,
            DeleteFileArgs {
                path: file.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap();
        assert!(!file.exists());
    }

    fn text_of(output: &ToolOutput) -> String {
        match &output.content[0] {
            shared::llm::ToolResultContent::Text { text } => text.clone(),
            other => panic!("expected Text, got {other:?}"),
        }
    }
}
