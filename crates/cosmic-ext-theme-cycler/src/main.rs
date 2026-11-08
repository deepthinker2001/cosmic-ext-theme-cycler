use std::process::ExitCode;

fn main() -> ExitCode {
    // Only these are taken as options: cosmic-panel starts applets with none.
    let option = std::env::args().nth(1);
    let restoring = option.as_deref() == Some("--restore");
    let informational = matches!(
        option.as_deref(),
        Some("--help" | "-h" | "--version" | "-V")
    );
    if !informational && let Some(refusal) = cosmic_ext_theme_cycler::refusal_as_root(restoring) {
        eprintln!("{refusal}");
        return ExitCode::FAILURE;
    }
    match option.as_deref() {
        Some("--restore") => return cosmic_ext_theme_cycler::restore_from_terminal(),
        Some("--help" | "-h") => {
            println!("{}", cosmic_ext_theme_cycler::usage());
            return ExitCode::SUCCESS;
        }
        Some("--version" | "-V") => {
            println!("cosmic-ext-theme-cycler {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    // Logging only for the applet: the options above talk to a person.
    tracing_subscriber::fmt::init();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        app_id = cosmic_ext_theme_cycler::APP_ID,
        "starting theme cycler applet",
    );
    match cosmic_ext_theme_cycler::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cosmic-ext-theme-cycler: {error}");
            ExitCode::FAILURE
        }
    }
}
