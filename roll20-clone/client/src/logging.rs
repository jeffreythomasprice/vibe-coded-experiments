//! Browser-console logging via `tracing`.
//!
//! Our crates log at TRACE, everything else at WARN — matching the server.
//! wasm has no environment, so the filter is fixed at compile time.

use tracing_subscriber::fmt::format::Pretty;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;
use tracing_web::MakeWebConsoleWriter;

pub fn init() {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false) // the browser console doesn't render ANSI colors
        .without_time() // timestamps come from the console itself
        .with_writer(MakeWebConsoleWriter::new())
        .with_level(true)
        .fmt_fields(Pretty::default());

    let filter = EnvFilter::new("warn,client=trace,shared=trace");

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}
