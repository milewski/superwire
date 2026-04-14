wit_bindgen::generate!({
    path: "wit",
    world: "engine",
});

use exports::superwire::engine::runner::{Guest, RunResult};

pub struct Engine;

impl Guest for Engine {
    fn run_workflow(workflow_path: String, input_json: String) -> RunResult {
        RunResult {
            ok: None,
            err: Some(format!("workflow {} input {}", workflow_path, input_json)),
        }
    }
}
