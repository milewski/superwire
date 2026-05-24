use std::fs;
use std::path::{Path, PathBuf};

use crate::diagnostics::CommandError;

pub(super) struct WorkflowPathTargets<'target> {
    workflow_targets: &'target [PathBuf],
}

impl<'target> WorkflowPathTargets<'target> {
    pub(super) const fn new(workflow_targets: &'target [PathBuf]) -> Self {
        Self { workflow_targets }
    }

    pub(super) fn collect(&self) -> Result<Vec<PathBuf>, CommandError> {
        let mut workflow_paths = Vec::new();

        for workflow_target in self.workflow_targets {
            if workflow_target.is_file() {
                if !Self::is_workflow_file_path(workflow_target) {
                    return Err(CommandError::invalid_input(format!(
                        "expected a .wire workflow file, got {}",
                        workflow_target.display()
                    )));
                }

                workflow_paths.push(workflow_target.clone());

                continue;
            }

            if workflow_target.is_dir() {
                Self::collect_wire_files_recursively(workflow_target, &mut workflow_paths)?;

                continue;
            }

            return Err(CommandError::invalid_input(format!(
                "path does not exist or is not accessible: {}",
                workflow_target.display()
            )));
        }

        workflow_paths.sort();
        workflow_paths.dedup();

        if workflow_paths.is_empty() {
            return Err(CommandError::invalid_input("no workflow files (.wire) found"));
        }

        Ok(workflow_paths)
    }

    fn collect_wire_files_recursively(directory_path: &Path, workflow_paths: &mut Vec<PathBuf>) -> Result<(), CommandError> {
        let directory_entries = fs::read_dir(directory_path)
            .map_err(|read_error| CommandError::internal(format!("failed to read directory {}: {read_error}", directory_path.display())))?;

        for directory_entry_result in directory_entries {
            let directory_entry = directory_entry_result.map_err(|read_error| {
                CommandError::internal(format!(
                    "failed to read entry in directory {}: {read_error}",
                    directory_path.display()
                ))
            })?;
            let entry_path = directory_entry.path();

            if entry_path.is_dir() {
                Self::collect_wire_files_recursively(&entry_path, workflow_paths)?;

                continue;
            }

            if Self::is_workflow_file_path(&entry_path) {
                workflow_paths.push(entry_path);
            }
        }

        Ok(())
    }

    fn is_workflow_file_path(file_path: &Path) -> bool {
        file_path.extension().and_then(|extension| extension.to_str()) == Some("wire")
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::WorkflowPathTargets;

    static NEXT_TEMPORARY_WORKSPACE_IDENTIFIER: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn collects_sorted_unique_workflow_files_from_files_and_directories() {
        let temporary_workspace = TemporaryWorkspace::new();
        let workflow_directory = temporary_workspace.create_directory("workflows");
        let first_workflow_path = temporary_workspace.write_file("workflows/first.wire");
        let nested_workflow_path = temporary_workspace.write_file("workflows/nested/second.wire");
        temporary_workspace.write_file("workflows/nested/notes.txt");
        let workflow_targets = vec![workflow_directory, nested_workflow_path.clone(), first_workflow_path.clone()];

        let workflow_paths = WorkflowPathTargets::new(&workflow_targets)
            .collect()
            .expect("workflow path collection should succeed");

        assert_eq!(workflow_paths, vec![first_workflow_path, nested_workflow_path]);
    }

    #[test]
    fn rejects_non_workflow_file_targets() {
        let temporary_workspace = TemporaryWorkspace::new();
        let notes_path = temporary_workspace.write_file("notes.txt");
        let workflow_targets = vec![notes_path.clone()];

        let command_error = WorkflowPathTargets::new(&workflow_targets)
            .collect()
            .expect_err("workflow path collection should reject non-workflow files");

        assert!(command_error.message().contains("expected a .wire workflow file"));
        assert!(command_error.message().contains(&notes_path.display().to_string()));
    }

    #[test]
    fn rejects_directories_without_workflow_files() {
        let temporary_workspace = TemporaryWorkspace::new();
        let workflow_directory = temporary_workspace.create_directory("workflows");
        temporary_workspace.write_file("workflows/notes.txt");
        let workflow_targets = vec![workflow_directory];

        let command_error = WorkflowPathTargets::new(&workflow_targets)
            .collect()
            .expect_err("workflow path collection should reject empty workflow directories");

        assert_eq!(command_error.message(), "no workflow files (.wire) found");
    }

    #[test]
    fn rejects_missing_targets() {
        let temporary_workspace = TemporaryWorkspace::new();
        let missing_path = temporary_workspace.root_directory.join("missing.wire");
        let workflow_targets = vec![missing_path.clone()];

        let command_error = WorkflowPathTargets::new(&workflow_targets)
            .collect()
            .expect_err("workflow path collection should reject missing paths");

        assert!(command_error.message().contains("path does not exist or is not accessible"));
        assert!(command_error.message().contains(&missing_path.display().to_string()));
    }

    struct TemporaryWorkspace {
        root_directory: PathBuf,
    }

    impl TemporaryWorkspace {
        fn new() -> Self {
            let root_directory = std::env::temp_dir().join(format!("superwire-workflow-path-tests-{}", unique_suffix()));

            fs::create_dir_all(&root_directory).expect("temporary root directory should be created");

            Self { root_directory }
        }

        fn create_directory(&self, relative_path: &str) -> PathBuf {
            let absolute_path = self.root_directory.join(relative_path);

            fs::create_dir_all(&absolute_path).expect("temporary directory should be created");

            absolute_path
        }

        fn write_file(&self, relative_path: &str) -> PathBuf {
            let absolute_path = self.root_directory.join(relative_path);

            if let Some(parent_directory) = absolute_path.parent() {
                fs::create_dir_all(parent_directory).expect("parent directory should be created");
            }

            fs::write(&absolute_path, "").expect("temporary file should be written");

            absolute_path
        }
    }

    impl Drop for TemporaryWorkspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root_directory);
        }
    }

    fn unique_suffix() -> String {
        let timestamp_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_millis();
        let process_identifier = std::process::id();
        let workspace_identifier = NEXT_TEMPORARY_WORKSPACE_IDENTIFIER.fetch_add(1, Ordering::Relaxed);

        format!("{timestamp_millis}-{process_identifier}-{workspace_identifier}")
    }
}
