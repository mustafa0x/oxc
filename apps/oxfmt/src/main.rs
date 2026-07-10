use oxfmt::cli::{CliRunResult, WalkRunner, format_command, init_rayon, init_tracing};

// Pure Rust CLI entry point.
// This CLI supports native Rust formatting modes.
// For full featured JS CLI entry point, see `run_cli()` exported by `main_napi.rs`.

#[tokio::main]
async fn main() -> CliRunResult {
    // Parse command line arguments from std::env::args()
    let command = format_command().run();

    init_tracing();
    init_rayon(command.runtime_options.threads);
    match command.mode {
        #[cfg(feature = "napi")]
        Mode::Stdin(_) => StdinRunner::new_without_js(command).run(),
        _ => WalkRunner::new(command).run(),
    }
}
