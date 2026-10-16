//! Managed installs of the Python OCR engines used for Japanese pages
//! (NDLOCR-Lite and YomiToku). The server checks them out and installs them
//! with `git` and `uv` under `[ocr_tools].dir`, the first time an ingest needs
//! them or when `ocr-tools install` asks.
//!
//! Layout under the install root:
//! - `ndlocr-lite/`: git checkout at `ndlocr_rev`, with its venv in `.venv/`
//! - `yomitoku/.venv/`: venv with `yomitoku==<yomitoku_version>` from PyPI
//! - `hf-cache/`: `HF_HOME` for YomiToku's model weights
//! - `install.log`: everything every install step printed, appended
//!
//! Each tool dir gets an `.installed.json` stamp as the *last* install step. A
//! dir without a stamp, or whose stamp names a different version or Python,
//! is deleted and rebuilt, so an interrupted or failed install never looks
//! installed. Venvs aren't relocatable, so building elsewhere and renaming
//! into place isn't an option. Installs only run inside the server's single
//! worker, so two never race.

use std::ffi::OsStr;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::sync::watch;

use crate::config::{OcrToolsConfig, YomitokuDevice};
use crate::proc::{self, ProcError};

const NDLOCR_DIR: &str = "ndlocr-lite";
const YOMITOKU_DIR: &str = "yomitoku";
const HF_CACHE_DIR: &str = "hf-cache";
const STAMP_FILE: &str = ".installed.json";
const LOG_FILE: &str = "install.log";

/// Lines of a failed step's output quoted in the error message. The full
/// output is in `install.log`.
const ERROR_TAIL_LINES: usize = 15;

const UV_HINT: &str = "Arch: pacman -S uv; elsewhere: curl -LsSf https://astral.sh/uv/install.sh | sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Ndlocr,
    Yomitoku,
}

impl Tool {
    pub fn name(self) -> &'static str {
        match self {
            Tool::Ndlocr => "NDLOCR-Lite",
            Tool::Yomitoku => "YomiToku",
        }
    }

    fn dir_name(self) -> &'static str {
        match self {
            Tool::Ndlocr => NDLOCR_DIR,
            Tool::Yomitoku => YOMITOKU_DIR,
        }
    }

    /// Progress stage shown while this tool installs (see
    /// `progress_format::stage_label`).
    pub fn install_stage(self) -> &'static str {
        match self {
            Tool::Ndlocr => "ocr-install-ndlocr",
            Tool::Yomitoku => "ocr-install-yomitoku",
        }
    }

    /// The version a fresh install would produce: the pinned commit for
    /// NDLOCR-Lite, the PyPI release for YomiToku.
    fn wanted_version(self, cfg: &OcrToolsConfig) -> &str {
        match self {
            Tool::Ndlocr => &cfg.ndlocr_rev,
            Tool::Yomitoku => &cfg.yomitoku_version,
        }
    }
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ToolsError {
    #[error(
        "`uv` not found on PATH; it installs the Japanese OCR engines (NDLOCR-Lite, YomiToku). \
         Install it ({UV_HINT}), or set [ocr].japanese_engine = \"tesseract\""
    )]
    UvMissing,

    #[error(
        "`git` not found on PATH; it checks out NDLOCR-Lite. Install git, or set \
         [ocr].japanese_engine to \"yomitoku\" or \"tesseract\""
    )]
    GitMissing,

    #[error(
        "{tools} not installed and [ocr_tools].auto_install = false; run \
         `document-search ocr-tools install`"
    )]
    NotInstalled { tools: String },

    #[error(
        "cloning NDLOCR-Lite from {repo} failed:\n{detail}\n(check the network and \
         [ocr_tools].ndlocr_repo; full output in {log})"
    )]
    Clone {
        repo: String,
        detail: String,
        log: PathBuf,
    },

    #[error(
        "checking out NDLOCR-Lite commit {rev} failed:\n{detail}\n(check that \
         [ocr_tools].ndlocr_rev names a commit in {repo}; full output in {log})"
    )]
    Checkout {
        rev: String,
        repo: String,
        detail: String,
        log: PathBuf,
    },

    #[error(
        "creating the {tool} Python {python} environment with uv failed:\n{detail}\n(uv \
         downloads Python {python} if it isn't installed, so check the network, or set \
         [ocr_tools].python to an installed version; full output in {log})"
    )]
    PythonUnavailable {
        tool: Tool,
        python: String,
        detail: String,
        log: PathBuf,
    },

    #[error(
        "installing {tool}'s Python packages with uv failed:\n{detail}\n({hint}full output \
         in {log})"
    )]
    DependencyInstall {
        tool: Tool,
        hint: &'static str,
        detail: String,
        log: PathBuf,
    },

    #[error(
        "downloading YomiToku's model weights from Hugging Face failed:\n{detail}\n(check \
         the network; full output in {log})"
    )]
    ModelDownload { detail: String, log: PathBuf },

    #[error(
        "{tool} installed but failed to start:\n{detail}\n(`document-search ocr-tools \
         install --reinstall` rebuilds it; full output in {log})"
    )]
    SmokeTest {
        tool: Tool,
        detail: String,
        log: PathBuf,
    },

    #[error(
        "[ocr_tools].yomitoku_device = \"cuda\" but torch in the YomiToku environment found \
         no usable CUDA GPU; set it to \"auto\" or \"cpu\""
    )]
    CudaUnavailable,

    #[error("running {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },

    #[error("{what} {path}: {source}")]
    Io {
        what: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("OCR tool install cancelled")]
    Cancelled,
}

/// Written as each tool's last install step; its presence means "complete".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// Commit for NDLOCR-Lite, release for YomiToku.
    pub version: String,
    pub python: String,
    /// Whether torch saw a CUDA GPU at install time. YomiToku only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda: Option<bool>,
    pub installed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ToolState {
    Missing,
    /// The dir exists but has no stamp: an install was interrupted or failed.
    Partial,
    /// Installed, but not the version or Python the config asks for.
    Outdated { stamp: Stamp, wanted: String },
    Installed { stamp: Stamp },
}

impl ToolState {
    fn installed(&self) -> Option<&Stamp> {
        match self {
            ToolState::Installed { stamp } => Some(stamp),
            _ => None,
        }
    }
}

/// What `ocr-tools status` reports.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub dir: PathBuf,
    /// `uv --version` output, or `None` when uv isn't on PATH.
    pub uv: Option<String>,
    pub git: Option<String>,
    pub ndlocr: ToolState,
    pub yomitoku: ToolState,
}

/// A ready-to-run NDLOCR-Lite. Run `python ocr.py` with `src_dir` as the
/// working directory (it imports its siblings and breaks under `python -I`).
#[derive(Debug, Clone)]
pub struct Ndlocr {
    pub python: PathBuf,
    pub src_dir: PathBuf,
}

/// A ready-to-run YomiToku CLI.
#[derive(Debug, Clone)]
pub struct Yomitoku {
    pub exe: PathBuf,
    pub hf_home: PathBuf,
    /// `"cuda"` or `"cpu"`, resolved from `yomitoku_device` and the stamp.
    pub device: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub ndlocr: Option<Ndlocr>,
    pub yomitoku: Option<Yomitoku>,
}

/// How `ensure` treats tools that aren't installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Install whatever is missing or outdated, if `auto_install` allows.
    /// Used by ingest.
    Ensure,
    /// Install whatever is missing or outdated regardless of `auto_install`.
    /// `ocr-tools install`.
    Install,
    /// Delete and reinstall even if installed. `ocr-tools install --reinstall`.
    Reinstall,
}

/// Result of `ensure`: the runnable tools plus which ones were (re)installed.
#[derive(Debug, Clone, Default)]
pub struct Ensured {
    pub tools: Tools,
    pub installed: Vec<Tool>,
}

pub async fn status(cfg: &OcrToolsConfig) -> Status {
    let root = cfg.dir();
    Status {
        uv: program_version("uv").await,
        git: program_version("git").await,
        ndlocr: tool_state(&root, Tool::Ndlocr, cfg),
        yomitoku: tool_state(&root, Tool::Yomitoku, cfg),
        dir: root,
    }
}

/// Make each tool in `wanted` runnable, installing per `mode`. `on_stage` is
/// called with `Tool::install_stage` before each install starts, so the
/// caller can surface a multi-minute install as its own progress stage.
pub async fn ensure<F>(
    cfg: &OcrToolsConfig,
    wanted: &[Tool],
    mode: Mode,
    cancel: Option<&watch::Receiver<bool>>,
    mut on_stage: F,
) -> Result<Ensured, ToolsError>
where
    F: FnMut(&'static str),
{
    let root = cfg.dir();
    let to_install: Vec<Tool> = wanted
        .iter()
        .copied()
        .filter(|&t| mode == Mode::Reinstall || tool_state(&root, t, cfg).installed().is_none())
        .collect();

    if !to_install.is_empty() {
        if mode == Mode::Ensure && !cfg.auto_install {
            let names: Vec<&str> = to_install.iter().map(|t| t.name()).collect();
            return Err(ToolsError::NotInstalled {
                tools: names.join(" and "),
            });
        }
        if program_version("uv").await.is_none() {
            return Err(ToolsError::UvMissing);
        }
        if to_install.contains(&Tool::Ndlocr) && program_version("git").await.is_none() {
            return Err(ToolsError::GitMissing);
        }
        std::fs::create_dir_all(&root).map_err(|source| ToolsError::Io {
            what: "creating OCR tools dir",
            path: root.clone(),
            source,
        })?;
        let installer = Installer {
            cfg,
            root: &root,
            log: root.join(LOG_FILE),
            cancel,
        };
        for &tool in &to_install {
            on_stage(tool.install_stage());
            tracing::info!(tool = tool.name(), dir = %root.display(), "ocr-tools: installing");
            let started = std::time::Instant::now();
            installer.install(tool).await?;
            tracing::info!(
                tool = tool.name(),
                secs = started.elapsed().as_secs_f64(),
                "ocr-tools: installed",
            );
        }
    }

    let mut tools = Tools::default();
    for &tool in wanted {
        let ToolState::Installed { stamp } = tool_state(&root, tool, cfg) else {
            // install() always ends by writing a matching stamp.
            unreachable!("{tool} not installed after install");
        };
        let dir = root.join(tool.dir_name());
        match tool {
            Tool::Ndlocr => {
                tools.ndlocr = Some(Ndlocr {
                    python: venv_python(&dir),
                    src_dir: dir.join("src"),
                })
            }
            Tool::Yomitoku => {
                let device = match (cfg.yomitoku_device, stamp.cuda.unwrap_or(false)) {
                    (YomitokuDevice::Cpu, _) | (YomitokuDevice::Auto, false) => "cpu",
                    (YomitokuDevice::Cuda, true) | (YomitokuDevice::Auto, true) => "cuda",
                    (YomitokuDevice::Cuda, false) => return Err(ToolsError::CudaUnavailable),
                };
                if device == "cpu" && cfg.yomitoku_device == YomitokuDevice::Auto {
                    tracing::warn!("ocr-tools: no CUDA GPU found at install time; YomiToku runs on CPU (slow)");
                }
                tools.yomitoku = Some(Yomitoku {
                    exe: dir.join(".venv").join("bin").join("yomitoku"),
                    hf_home: root.join(HF_CACHE_DIR),
                    device,
                })
            }
        }
    }
    Ok(Ensured {
        tools,
        installed: to_install,
    })
}

fn tool_state(root: &Path, tool: Tool, cfg: &OcrToolsConfig) -> ToolState {
    let dir = root.join(tool.dir_name());
    if !dir.exists() {
        return ToolState::Missing;
    }
    let Some(stamp) = std::fs::read_to_string(dir.join(STAMP_FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<Stamp>(&s).ok())
    else {
        return ToolState::Partial;
    };
    let wanted = tool.wanted_version(cfg);
    if stamp.version != wanted || stamp.python != cfg.python {
        return ToolState::Outdated {
            stamp,
            wanted: wanted.to_string(),
        };
    }
    ToolState::Installed { stamp }
}

fn venv_python(dir: &Path) -> PathBuf {
    dir.join(".venv").join("bin").join("python")
}

/// First line of `<program> --version`, or `None` when it can't be run.
async fn program_version(program: &str) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.arg("--version");
    let out = proc::run(cmd, None).await.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    Some(text.lines().next().unwrap_or("").trim().to_string())
}

struct Installer<'a> {
    cfg: &'a OcrToolsConfig,
    root: &'a Path,
    log: PathBuf,
    cancel: Option<&'a watch::Receiver<bool>>,
}

/// A failed install step, before it's mapped to the `ToolsError` variant that
/// says which step it was.
enum StepError {
    Failed(String),
    Fatal(ToolsError),
}

impl Installer<'_> {
    async fn install(&self, tool: Tool) -> Result<(), ToolsError> {
        let dir = self.root.join(tool.dir_name());
        if dir.exists() {
            tokio::fs::remove_dir_all(&dir)
                .await
                .map_err(|source| ToolsError::Io {
                    what: "removing old install",
                    path: dir.clone(),
                    source,
                })?;
        }
        let cuda = match tool {
            Tool::Ndlocr => {
                self.install_ndlocr(&dir).await?;
                None
            }
            Tool::Yomitoku => Some(self.install_yomitoku(&dir).await?),
        };
        let stamp = Stamp {
            version: tool.wanted_version(self.cfg).to_string(),
            python: self.cfg.python.clone(),
            cuda,
            installed_at: chrono::Local::now().to_rfc3339(),
        };
        let path = dir.join(STAMP_FILE);
        let json = serde_json::to_string_pretty(&stamp).expect("stamp serializes");
        std::fs::write(&path, json).map_err(|source| ToolsError::Io {
            what: "writing install stamp",
            path,
            source,
        })
    }

    async fn install_ndlocr(&self, dir: &Path) -> Result<(), ToolsError> {
        let cfg = self.cfg;
        let mut clone = Command::new("git");
        clone
            .args(["clone", "--filter=blob:none", "--no-checkout", &cfg.ndlocr_repo])
            .arg(dir);
        self.step(Tool::Ndlocr, clone, None)
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::Clone {
                repo: cfg.ndlocr_repo.clone(),
                detail,
                log,
            }))?;

        // Resolve the rev first: `git checkout` reads an unknown rev as a
        // path and reports something unhelpful about path arguments.
        let checkout_err = |detail: String, log: PathBuf| ToolsError::Checkout {
            rev: cfg.ndlocr_rev.clone(),
            repo: cfg.ndlocr_repo.clone(),
            detail,
            log,
        };
        let mut resolve = Command::new("git");
        resolve
            .arg("-C")
            .arg(dir)
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("{}^{{commit}}", cfg.ndlocr_rev));
        let commit = match self.step(Tool::Ndlocr, resolve, None).await {
            Ok(out) => String::from_utf8_lossy(&out).trim().to_string(),
            Err(StepError::Failed(_)) => {
                return Err(checkout_err(
                    "no such commit in the repository".to_string(),
                    self.log.clone(),
                ));
            }
            Err(StepError::Fatal(e)) => return Err(e),
        };
        let mut checkout = Command::new("git");
        checkout
            .arg("-C")
            .arg(dir)
            .args(["checkout", "--detach", &commit]);
        self.step(Tool::Ndlocr, checkout, None)
            .await
            .map_err(|e| self.fail(e, checkout_err))?;

        self.create_venv(Tool::Ndlocr, dir).await?;

        let mut pip = self.uv_pip_install(dir);
        pip.arg("-r").arg(dir.join("requirements.txt"));
        self.step(Tool::Ndlocr, pip, None)
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::DependencyInstall {
                tool: Tool::Ndlocr,
                hint: "check the network; `uv cache clean` clears a corrupt cache; ",
                detail,
                log,
            }))?;

        // ocr.py imports onnxruntime and its model code before parsing args,
        // so `--version` proves the environment is usable.
        let mut smoke = Command::new(venv_python(dir));
        smoke.args(["ocr.py", "--version"]).current_dir(dir.join("src"));
        self.step(Tool::Ndlocr, smoke, None)
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::SmokeTest {
                tool: Tool::Ndlocr,
                detail,
                log,
            }))?;
        Ok(())
    }

    /// Returns whether torch can see a CUDA GPU.
    async fn install_yomitoku(&self, dir: &Path) -> Result<bool, ToolsError> {
        std::fs::create_dir_all(dir).map_err(|source| ToolsError::Io {
            what: "creating",
            path: dir.to_path_buf(),
            source,
        })?;
        self.create_venv(Tool::Yomitoku, dir).await?;

        let mut pip = self.uv_pip_install(dir);
        pip.arg(format!("yomitoku=={}", self.cfg.yomitoku_version));
        self.step(Tool::Yomitoku, pip, None)
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::DependencyInstall {
                tool: Tool::Yomitoku,
                hint: "check the network and that [ocr_tools].yomitoku_version is a release \
                       on PyPI; `uv cache clean` clears a corrupt cache; ",
                detail,
                log,
            }))?;

        let python = venv_python(dir);
        let mut probe = Command::new(&python);
        probe.args([
            "-c",
            "import torch, yomitoku; print('cuda', torch.cuda.is_available())",
        ]);
        let out = self
            .step(Tool::Yomitoku, probe, None)
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::SmokeTest {
                tool: Tool::Yomitoku,
                detail,
                log,
            }))?;
        let cuda = String::from_utf8_lossy(&out).contains("cuda True");

        // Constructing the analyzer downloads every model it uses, so an
        // ingest never stalls on (or fails at) a first-run download.
        let hf_home = self.root.join(HF_CACHE_DIR);
        let mut fetch = Command::new(&python);
        fetch.args([
            "-c",
            "from yomitoku import DocumentAnalyzer; DocumentAnalyzer(device='cpu')",
        ]);
        self.step(Tool::Yomitoku, fetch, Some(&hf_home))
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::ModelDownload { detail, log }))?;
        Ok(cuda)
    }

    async fn create_venv(&self, tool: Tool, dir: &Path) -> Result<(), ToolsError> {
        let mut venv = Command::new("uv");
        venv.args(["venv", "--python", &self.cfg.python])
            .arg(dir.join(".venv"));
        self.step(tool, venv, None)
            .await
            .map_err(|e| self.fail(e, |detail, log| ToolsError::PythonUnavailable {
                tool,
                python: self.cfg.python.clone(),
                detail,
                log,
            }))?;
        Ok(())
    }

    fn uv_pip_install(&self, dir: &Path) -> Command {
        let mut cmd = Command::new("uv");
        cmd.args(["pip", "install", "--python"]).arg(venv_python(dir));
        cmd
    }

    /// Run one install step, appending its command line and output to
    /// `install.log`. Returns stdout on success.
    async fn step(
        &self,
        tool: Tool,
        mut cmd: Command,
        hf_home: Option<&Path>,
    ) -> Result<Vec<u8>, StepError> {
        if let Some(h) = hf_home {
            cmd.env("HF_HOME", h);
        }
        let std_cmd = cmd.as_std();
        let program = std_cmd.get_program().to_string_lossy().into_owned();
        let line = std::iter::once(std_cmd.get_program())
            .chain(std_cmd.get_args())
            .map(OsStr::to_string_lossy)
            .collect::<Vec<_>>()
            .join(" ");
        tracing::debug!(tool = tool.name(), cmd = %line, "ocr-tools: running");
        self.append_log(&format!(
            "\n=== {} {}: {line}\n",
            chrono::Local::now().to_rfc3339(),
            tool.name()
        ));
        let result = proc::run(cmd, self.cancel).await;
        match &result {
            Ok(out) | Err(ProcError::Failed(out)) => {
                self.append_log(&String::from_utf8_lossy(&out.stdout));
                self.append_log(&String::from_utf8_lossy(&out.stderr));
                self.append_log(&format!("=== exit: {}\n", out.status));
            }
            Err(other) => self.append_log(&format!("=== {other:?}\n")),
        }
        match result {
            Ok(out) => Ok(out.stdout),
            Err(ProcError::Failed(out)) => {
                Err(StepError::Failed(proc::output_tail(&out, ERROR_TAIL_LINES)))
            }
            Err(ProcError::Cancelled) => Err(StepError::Fatal(ToolsError::Cancelled)),
            Err(ProcError::NotFound) if program == "uv" => {
                Err(StepError::Fatal(ToolsError::UvMissing))
            }
            Err(ProcError::NotFound) if program == "git" => {
                Err(StepError::Fatal(ToolsError::GitMissing))
            }
            Err(ProcError::NotFound) => Err(StepError::Fatal(ToolsError::Spawn {
                program,
                source: std::io::ErrorKind::NotFound.into(),
            })),
            Err(ProcError::Spawn(source) | ProcError::Wait(source)) => {
                Err(StepError::Fatal(ToolsError::Spawn { program, source }))
            }
        }
    }

    fn fail(&self, e: StepError, make: impl FnOnce(String, PathBuf) -> ToolsError) -> ToolsError {
        match e {
            StepError::Failed(detail) => make(detail, self.log.clone()),
            StepError::Fatal(err) => err,
        }
    }

    /// Best effort: a log that can't be written shouldn't fail the install.
    fn append_log(&self, text: &str) {
        let res = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .and_then(|mut f| f.write_all(text.as_bytes()));
        if let Err(e) = res {
            tracing::warn!(path = %self.log.display(), error = %e, "ocr-tools: writing install log");
        }
    }
}

/// The tools `ocr-tools install` installs: the ones `[ocr].japanese_engine`
/// uses, or both when it is `tesseract` (installing then is an explicit ask).
pub fn tools_for_install(engine: crate::config::JapaneseEngine) -> &'static [Tool] {
    use crate::config::JapaneseEngine;
    match engine {
        JapaneseEngine::Ndlocr => &[Tool::Ndlocr],
        JapaneseEngine::Yomitoku => &[Tool::Yomitoku],
        JapaneseEngine::Ensemble | JapaneseEngine::Tesseract => &[Tool::Ndlocr, Tool::Yomitoku],
    }
}

fn state_line(state: &ToolState) -> String {
    match state {
        ToolState::Missing => "not installed".to_string(),
        ToolState::Partial => "incomplete install (rebuilt on next install)".to_string(),
        ToolState::Outdated { stamp, wanted } => format!(
            "outdated: {} on Python {}, config wants {wanted} (reinstalled on next install)",
            stamp.version, stamp.python
        ),
        ToolState::Installed { stamp } => {
            let mut s = format!("installed {} (Python {}", stamp.version, stamp.python);
            match stamp.cuda {
                Some(true) => s.push_str(", CUDA available"),
                Some(false) => s.push_str(", no CUDA: runs on CPU"),
                None => {}
            }
            s.push_str(&format!(", {})", stamp.installed_at));
            s
        }
    }
}

pub fn render_status_text(st: &Status) -> String {
    let found = |v: &Option<String>| match v {
        Some(v) if !v.is_empty() => v.clone(),
        Some(_) => "found".to_string(),
        None => "NOT FOUND on PATH".to_string(),
    };
    format!(
        "dir:         {}\nuv:          {}\ngit:         {}\nNDLOCR-Lite: {}\nYomiToku:    {}\n",
        st.dir.display(),
        found(&st.uv),
        found(&st.git),
        state_line(&st.ndlocr),
        state_line(&st.yomitoku),
    )
}

pub fn render_status_json(st: &Status) -> String {
    let mut v = serde_json::to_value(st).expect("status serializes");
    v["ok"] = serde_json::Value::Bool(true);
    v.to_string()
}

pub fn render_install_text(cfg: &OcrToolsConfig, ensured: &Ensured, wanted: &[Tool]) -> String {
    let mut s = String::new();
    for t in wanted {
        let verb = if ensured.installed.contains(t) {
            "installed"
        } else {
            "already installed"
        };
        s.push_str(&format!(
            "{}: {verb} ({})\n",
            t.name(),
            t.wanted_version(cfg)
        ));
    }
    s.push_str(&format!("dir: {}\n", cfg.dir().display()));
    s
}

pub fn render_install_json(cfg: &OcrToolsConfig, ensured: &Ensured, wanted: &[Tool]) -> String {
    serde_json::json!({
        "ok": true,
        "dir": cfg.dir().display().to_string(),
        "tools": wanted,
        "installed": ensured.installed,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path) -> OcrToolsConfig {
        OcrToolsConfig {
            dir: Some(dir.to_path_buf()),
            auto_install: true,
            python: "3.12".into(),
            ndlocr_repo: "https://example.invalid/ndlocr-lite".into(),
            ndlocr_rev: "abc123".into(),
            yomitoku_version: "0.15.0".into(),
            yomitoku_device: YomitokuDevice::Auto,
            batch_pages: 32,
        }
    }

    fn write_stamp(dir: &Path, version: &str, cuda: Option<bool>) {
        std::fs::create_dir_all(dir).unwrap();
        let stamp = Stamp {
            version: version.into(),
            python: "3.12".into(),
            cuda,
            installed_at: "2026-10-16T00:00:00+00:00".into(),
        };
        std::fs::write(dir.join(STAMP_FILE), serde_json::to_string(&stamp).unwrap()).unwrap();
    }

    #[test]
    fn tool_state_reads_stamps() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg(tmp.path());
        assert_eq!(tool_state(tmp.path(), Tool::Ndlocr, &c), ToolState::Missing);

        let nd = tmp.path().join(NDLOCR_DIR);
        std::fs::create_dir_all(&nd).unwrap();
        assert_eq!(tool_state(tmp.path(), Tool::Ndlocr, &c), ToolState::Partial);

        write_stamp(&nd, "old", None);
        assert!(matches!(
            tool_state(tmp.path(), Tool::Ndlocr, &c),
            ToolState::Outdated { ref wanted, .. } if wanted == "abc123"
        ));

        write_stamp(&nd, "abc123", None);
        assert!(tool_state(tmp.path(), Tool::Ndlocr, &c).installed().is_some());

        let c312 = OcrToolsConfig {
            python: "3.11".into(),
            ..c.clone()
        };
        assert!(matches!(
            tool_state(tmp.path(), Tool::Ndlocr, &c312),
            ToolState::Outdated { .. }
        ));
    }

    #[tokio::test]
    async fn ensure_uses_installed_tools_without_running_anything() {
        let tmp = tempfile::tempdir().unwrap();
        let c = cfg(tmp.path());
        write_stamp(&tmp.path().join(NDLOCR_DIR), "abc123", None);
        write_stamp(&tmp.path().join(YOMITOKU_DIR), "0.15.0", Some(true));
        let mut stages = Vec::new();
        let got = ensure(&c, &[Tool::Ndlocr, Tool::Yomitoku], Mode::Ensure, None, |s| {
            stages.push(s)
        })
        .await
        .unwrap();
        assert!(stages.is_empty());
        assert!(got.installed.is_empty());
        assert_eq!(got.tools.yomitoku.unwrap().device, "cuda");
        assert!(got.tools.ndlocr.unwrap().src_dir.ends_with("ndlocr-lite/src"));
    }

    #[tokio::test]
    async fn device_follows_config_and_probe() {
        let tmp = tempfile::tempdir().unwrap();
        write_stamp(&tmp.path().join(YOMITOKU_DIR), "0.15.0", Some(false));
        let auto = cfg(tmp.path());
        let got = ensure(&auto, &[Tool::Yomitoku], Mode::Ensure, None, |_| {}).await;
        assert_eq!(got.unwrap().tools.yomitoku.unwrap().device, "cpu");

        let cuda = OcrToolsConfig {
            yomitoku_device: YomitokuDevice::Cuda,
            ..auto
        };
        let err = ensure(&cuda, &[Tool::Yomitoku], Mode::Ensure, None, |_| {})
            .await
            .unwrap_err();
        assert!(matches!(err, ToolsError::CudaUnavailable));
    }

    #[tokio::test]
    async fn ensure_without_auto_install_names_missing_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let c = OcrToolsConfig {
            auto_install: false,
            ..cfg(tmp.path())
        };
        let err = ensure(&c, &[Tool::Ndlocr, Tool::Yomitoku], Mode::Ensure, None, |_| {})
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("NDLOCR-Lite and YomiToku not installed"), "{msg}");
        assert!(msg.contains("ocr-tools install"), "{msg}");
    }

    #[test]
    fn errors_name_the_step_and_log() {
        let e = ToolsError::Checkout {
            rev: "deadbeef".into(),
            repo: "https://example.invalid/r".into(),
            detail: "fatal: reference is not a tree: deadbeef".into(),
            log: PathBuf::from("/x/install.log"),
        };
        let msg = e.to_string();
        assert!(msg.contains("commit deadbeef"), "{msg}");
        assert!(msg.contains("ndlocr_rev"), "{msg}");
        assert!(msg.contains("/x/install.log"), "{msg}");
        assert!(ToolsError::UvMissing.to_string().contains("pacman -S uv"));
    }
}
