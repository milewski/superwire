fn main() {
    if let Err(command_error) = engine_ai_cli::run_from_environment() {
        eprintln!("{command_error}");
        std::process::exit(1);
    }
}
