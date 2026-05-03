use std::path::Path;

use chrono::{SecondsFormat, Utc};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{self, FmtContext, FormatEvent, FormatFields};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::config::LoggingConfig;
use crate::error::Error;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Role {
    Client,
    Server,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Role::Client => "client",
            Role::Server => "server",
        }
    }
}

struct LineFormat {
    role: Role,
    pid: u32,
}

impl<S, N> FormatEvent<S, N> for LineFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let ts = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let meta = event.metadata();
        write!(
            writer,
            "{ts} pid={} {} {} {}: ",
            self.pid,
            self.role.label(),
            meta.level(),
            meta.target(),
        )?;
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

pub fn init(role: Role, cfg: &LoggingConfig) -> Result<(), Error> {
    let crate_target = env!("CARGO_PKG_NAME").replace('-', "_");
    let default_filter = format!("warn,{crate_target}=trace");
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let pid = std::process::id();

    // Make sure the log file's parent dir exists. /tmp always does, but a
    // user-configured path under a missing dir shouldn't crash inside the
    // appender.
    if let Some(parent) = cfg.file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::Logging(format!("creating log dir {}: {e}", parent.display()))
            })?;
        }
    }

    let parent = cfg
        .file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let filename = cfg
        .file
        .file_name()
        .ok_or_else(|| Error::Logging(format!("logging.file has no filename: {}", cfg.file.display())))?;
    // rolling::never opens with O_APPEND, which makes write(2) calls below
    // PIPE_BUF (4096 bytes on Linux) atomic across processes — good enough
    // for a CLI tool sharing one log file between client and server.
    let file_appender = tracing_appender::rolling::never(parent, filename);

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .event_format(LineFormat { role, pid });

    // Stderr layer is server-only: clients render an indicatif spinner on
    // stderr and would interleave with log lines. The file layer captures
    // everything from both roles.
    let stderr_layer = (role == Role::Server).then(|| {
        fmt::layer()
            .with_writer(std::io::stderr)
            .event_format(LineFormat { role, pid })
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer.map(|l| l.boxed()))
        .try_init()
        .map_err(|e| Error::Logging(e.to_string()))?;
    Ok(())
}
