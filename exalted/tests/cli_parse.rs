//! Smoke tests for the clap surface in `exalted::cli`. Confirms the
//! restructured CLI keeps the existing subcommand semantics while making the
//! bare `exalted` and `exalted FILE` invocations land in the GUI launcher
//! path.

use clap::Parser;

use exalted::cli::{Cli, Cmd, OutputFormat};

#[test]
fn bare_invocation_has_no_command_and_no_file() {
    let cli = Cli::try_parse_from(["exalted"]).expect("bare invocation must parse");
    assert!(cli.command.is_none(), "no subcommand expected");
    assert!(cli.file.is_none(), "no positional file expected");
    assert_eq!(cli.output_format, OutputFormat::Text);
}

#[test]
fn positional_file_lands_on_file_for_ui_launch() {
    let cli = Cli::try_parse_from(["exalted", "assets/sample-character.toml"])
        .expect("positional must parse");
    assert!(cli.command.is_none(), "positional file is not a subcommand");
    assert_eq!(
        cli.file.as_deref(),
        Some(std::path::Path::new("assets/sample-character.toml"))
    );
}

#[test]
fn validate_subcommand_still_parses() {
    let cli = Cli::try_parse_from(["exalted", "validate", "assets/sample-character.toml"])
        .expect("validate must parse");
    match cli.command.expect("expected a subcommand") {
        Cmd::Validate { file } => {
            assert_eq!(file, std::path::PathBuf::from("assets/sample-character.toml"));
        }
        other => panic!("expected Validate, got {:?}", std::mem::discriminant(&other)),
    }
}

#[test]
fn global_output_format_flag_carries_through_subcommands() {
    let cli =
        Cli::try_parse_from(["exalted", "--output-format", "json", "validate", "x.toml"])
            .expect("global flag with subcommand must parse");
    assert_eq!(cli.output_format, OutputFormat::Json);
    assert!(matches!(cli.command, Some(Cmd::Validate { .. })));
}

#[test]
fn render_subcommand_with_pdf_format_parses() {
    let cli = Cli::try_parse_from([
        "exalted", "render", "--format", "pdf", "-o", "out.pdf", "x.toml",
    ])
    .expect("render with pdf format must parse");
    match cli.command.expect("subcommand") {
        Cmd::Render { file, output, format } => {
            assert_eq!(file, std::path::PathBuf::from("x.toml"));
            assert_eq!(output, Some(std::path::PathBuf::from("out.pdf")));
            assert_eq!(format, exalted::cli::RenderFormat::Pdf);
        }
        _ => panic!("expected Render"),
    }
}
