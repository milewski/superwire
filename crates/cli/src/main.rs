mod app;
mod commands;
mod diagnostics;

use app::Application;

fn main() {
    let application = Application::from_environment();
    let exit_status = application.run();

    std::process::exit(exit_status.code());
}
