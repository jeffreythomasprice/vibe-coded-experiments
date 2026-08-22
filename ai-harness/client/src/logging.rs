//! Frontend logging.
//!
//! wasm cannot write files, so events go two places: the browser devtools
//! console (immediate, for development) and — via [`ForwardLayer`] — the Tauri
//! backend, which re-emits them into the same rotating log files the server
//! writes. See `lib::logging::log_client_record`.

use std::cell::Cell;
use std::fmt;

use serde::Serialize;
use shared::ClientLogRecord;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::registry;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_web::MakeWebConsoleWriter;
use wasm_bindgen_futures::spawn_local;

use crate::ipc::invoke;

/// Argument envelope for the backend's `log_from_client` command. The Rust
/// parameter is named `record`, so the JS-side key must be `record` too.
#[derive(Serialize)]
struct LogArgs {
    record: ClientLogRecord,
}

thread_local! {
    /// Set while we are forwarding, so a log emitted on the forwarding path
    /// cannot recurse back into it.
    static FORWARDING: Cell<bool> = const { Cell::new(false) };
}

/// Install the frontend subscriber. Call once, before mounting.
pub fn init() {
    console_error_panic_hook::set_once();

    // `Targets`, not `EnvFilter`: wasm has no environment to read, and
    // `EnvFilter` would pull regex into the bundle for nothing. Mirrors the
    // backend default — our crates at TRACE, everything else at WARN.
    let filter = Targets::new()
        .with_default(LevelFilter::WARN)
        .with_target("client", LevelFilter::TRACE)
        .with_target("shared", LevelFilter::TRACE);

    let console_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        // The console adds its own timestamps.
        .without_time()
        .with_writer(MakeWebConsoleWriter::new());

    let _ = registry()
        .with(filter)
        .with(console_layer)
        .with(ForwardLayer)
        .try_init();
}

/// Ships each event to the backend over IPC.
struct ForwardLayer;

impl<S: Subscriber> Layer<S> for ForwardLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if FORWARDING.with(Cell::get) {
            return;
        }

        let meta = event.metadata();
        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        let record = ClientLogRecord {
            // JS `Date.toISOString()` is already exactly ISO8601 with millis.
            timestamp: String::from(js_sys::Date::new_0().to_iso_string()),
            level: meta.level().into(),
            target: meta.target().to_string(),
            file: meta.file().map(str::to_string),
            line: meta.line(),
            message: visitor.finish(),
        };

        // If serialization fails there is nothing safe to do about it — logging
        // the failure here would recurse. The event is not lost: the console
        // layer above has already rendered it.
        let Ok(args) = serde_wasm_bindgen::to_value(&LogArgs { record }) else {
            return;
        };

        FORWARDING.with(|f| f.set(true));
        spawn_local(async move {
            let _ = invoke("log_from_client", args).await;
        });
        FORWARDING.with(|f| f.set(false));
    }
}

/// Flattens an event's fields into one string: the `message` field verbatim,
/// with any other fields appended as `key=value`.
#[derive(Default)]
struct MessageVisitor {
    message: String,
    fields: String,
}

impl MessageVisitor {
    fn finish(self) -> String {
        format!("{}{}", self.message, self.fields)
    }

    fn push(&mut self, field: &Field, value: impl fmt::Display) {
        use fmt::Write;
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            let _ = write!(self.fields, " {}={}", field.name(), value);
        }
    }
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.push(field, format_args!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.push(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.push(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.push(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.push(field, value);
    }
}
