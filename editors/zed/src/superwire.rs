use std::env;
use std::fs;
use std::path::PathBuf;
use zed_extension_api::settings::LspSettings;
use zed_extension_api::{self as zed, LanguageServerId, Result};

const SERVER_NAME: &str = "superwire-lsp";
const SERVER_PATH_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_LSP_PATH";
const WSL_LOCALHOST_PREFIX: &str = "\\\\wsl.localhost\\";
const WSL_DOLLAR_PREFIX: &str = "\\\\wsl$\\";

struct WslWorktree {
    distribution_name: String,
    linux_worktree_root: String,
}

struct SuperwireExtension {
    cached_server_path: Option<String>,
}

impl SuperwireExtension {
    fn command_for_server(&mut self, worktree: &zed::Worktree) -> Result<zed::Command> {
        let binary_settings = LspSettings::for_worktree(SERVER_NAME, worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.binary);

        let command_arguments = binary_settings
            .as_ref()
            .and_then(|binary_configuration| binary_configuration.arguments.clone())
            .unwrap_or_default();

        if let Some(configured_server_path) = binary_settings
            .as_ref()
            .and_then(|binary_configuration| binary_configuration.path.clone())
        {
            return Ok(zed::Command {
                command: configured_server_path,
                args: command_arguments.clone(),
                env: Self::shell_environment_for_worktree(worktree),
            });
        }

        if let Some(environment_server_path) = Self::resolve_environment_server_path() {
            return Ok(zed::Command {
                command: environment_server_path,
                args: command_arguments.clone(),
                env: Self::shell_environment_for_worktree(worktree),
            });
        }

        if let Some(wsl_server_command) = Self::resolve_wsl_server_command(worktree, &command_arguments) {
            return Ok(wsl_server_command);
        }

        let shell_environment = Self::shell_environment_for_worktree(worktree);

        if let Some(worktree_server_path) = Self::resolve_worktree_server_path(worktree) {
            self.cached_server_path = Some(worktree_server_path.clone());

            return Ok(zed::Command {
                command: worktree_server_path,
                args: command_arguments,
                env: shell_environment,
            });
        }

        if let Some(cached_server_path) = self.cached_server_path.clone() {
            if Self::is_executable_server_binary(&cached_server_path) {
                return Ok(zed::Command {
                    command: cached_server_path,
                    args: command_arguments,
                    env: shell_environment,
                });
            }
        }

        if let Some(path_server_path) = worktree.which(SERVER_NAME) {
            self.cached_server_path = Some(path_server_path.clone());

            return Ok(zed::Command {
                command: path_server_path,
                args: command_arguments,
                env: shell_environment,
            });
        }

        if let Some(local_server_path) = Self::resolve_local_server_path() {
            self.cached_server_path = Some(local_server_path.clone());

            return Ok(zed::Command {
                command: local_server_path,
                args: command_arguments,
                env: shell_environment,
            });
        }

        Ok(zed::Command {
            command: SERVER_NAME.to_string(),
            args: command_arguments,
            env: shell_environment,
        })
    }

    fn resolve_environment_server_path() -> Option<String> {
        let configured_server_path = env::var(SERVER_PATH_ENVIRONMENT_VARIABLE).ok()?;

        if Self::is_executable_server_binary(&configured_server_path) {
            return Some(configured_server_path);
        }

        None
    }

    fn resolve_local_server_path() -> Option<String> {
        let current_directory = env::current_dir().ok()?;

        let server_binary_names = Self::server_binary_names();
        let candidate_directories = [
            current_directory.join("../../target/release"),
            current_directory.join("../../target/debug"),
            current_directory.join("../../../target/release"),
            current_directory.join("../../../target/debug"),
        ];

        for candidate_directory in candidate_directories {
            for server_binary_name in &server_binary_names {
                let candidate_server_path = candidate_directory.join(server_binary_name);

                if Self::is_executable_server_binary_path(&candidate_server_path) {
                    return Some(candidate_server_path.to_string_lossy().to_string());
                }
            }
        }

        None
    }

    fn resolve_wsl_server_command(worktree: &zed::Worktree, command_arguments: &[String]) -> Option<zed::Command> {
        let wsl_worktree = Self::parse_wsl_worktree(worktree)?;
        let candidate_server_paths = [
            format!("{}/target/release/superwire-lsp", wsl_worktree.linux_worktree_root),
            format!("{}/target/debug/superwire-lsp", wsl_worktree.linux_worktree_root),
        ];

        for candidate_server_path in &candidate_server_paths {
            if !Self::wsl_path_exists(&wsl_worktree.distribution_name, candidate_server_path, "-x") {
                continue;
            }

            let mut command_arguments_for_wsl = vec![
                "-d".to_string(),
                wsl_worktree.distribution_name.clone(),
                "--".to_string(),
                candidate_server_path.clone(),
            ];

            command_arguments_for_wsl.extend(command_arguments.iter().cloned());

            return Some(zed::Command {
                command: "wsl.exe".to_string(),
                args: command_arguments_for_wsl,
                env: Vec::new(),
            });
        }

        let manifest_path = format!("{}/crates/lsp/Cargo.toml", wsl_worktree.linux_worktree_root);

        if !Self::wsl_path_exists(&wsl_worktree.distribution_name, &manifest_path, "-f") {
            return None;
        }

        let mut command_arguments_for_wsl = vec![
            "-d".to_string(),
            wsl_worktree.distribution_name,
            "--".to_string(),
            "cargo".to_string(),
            "run".to_string(),
            "--quiet".to_string(),
            "--manifest-path".to_string(),
            manifest_path,
            "--bin".to_string(),
            SERVER_NAME.to_string(),
            "--".to_string(),
        ];

        command_arguments_for_wsl.extend(command_arguments.iter().cloned());

        Some(zed::Command {
            command: "wsl.exe".to_string(),
            args: command_arguments_for_wsl,
            env: Vec::new(),
        })
    }

    fn parse_wsl_worktree(worktree: &zed::Worktree) -> Option<WslWorktree> {
        let (operating_system, _) = zed::current_platform();

        if operating_system != zed::Os::Windows {
            return None;
        }

        let worktree_root_path = worktree.root_path();
        let wsl_path_remainder = worktree_root_path
            .strip_prefix(WSL_LOCALHOST_PREFIX)
            .or_else(|| worktree_root_path.strip_prefix(WSL_DOLLAR_PREFIX))?;

        let path_components: Vec<&str> = wsl_path_remainder
            .split('\\')
            .filter(|path_component| !path_component.is_empty())
            .collect();

        if path_components.len() < 2 {
            return None;
        }

        let distribution_name = path_components[0].to_string();
        let linux_worktree_root = format!("/{}", path_components[1..].join("/"));

        Some(WslWorktree {
            distribution_name,
            linux_worktree_root,
        })
    }

    fn wsl_path_exists(distribution_name: &str, candidate_path: &str, test_flag: &str) -> bool {
        let mut test_command = zed::process::Command::new("wsl.exe")
            .arg("-d")
            .arg(distribution_name)
            .arg("--")
            .arg("test")
            .arg(test_flag)
            .arg(candidate_path);

        test_command.output().is_ok_and(|command_output| command_output.status == Some(0))
    }

    fn resolve_worktree_server_path(worktree: &zed::Worktree) -> Option<String> {
        let worktree_root_directory = PathBuf::from(worktree.root_path());
        let server_binary_names = Self::server_binary_names();

        for ancestor_directory in worktree_root_directory.ancestors().take(4) {
            let candidate_directories = [ancestor_directory.join("target/release"), ancestor_directory.join("target/debug")];

            for candidate_directory in candidate_directories {
                for server_binary_name in &server_binary_names {
                    let candidate_server_path = candidate_directory.join(server_binary_name);

                    if Self::is_executable_server_binary_path(&candidate_server_path) {
                        return Some(candidate_server_path.to_string_lossy().to_string());
                    }
                }
            }
        }

        None
    }

    fn server_binary_names() -> Vec<&'static str> {
        let (operating_system, _) = zed::current_platform();

        if operating_system == zed::Os::Windows {
            return vec!["superwire-lsp.exe"];
        }

        vec!["superwire-lsp", "superwire-lsp.exe"]
    }

    fn is_executable_server_binary(candidate_server_path: &str) -> bool {
        Self::is_executable_server_binary_path(&PathBuf::from(candidate_server_path))
    }

    fn is_executable_server_binary_path(candidate_server_path: &PathBuf) -> bool {
        if !fs::metadata(candidate_server_path).is_ok_and(|metadata| metadata.is_file()) {
            return false;
        }

        let (operating_system, _) = zed::current_platform();

        if operating_system != zed::Os::Windows {
            return true;
        }

        candidate_server_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat")
            })
    }

    fn shell_environment_for_worktree(worktree: &zed::Worktree) -> zed::EnvVars {
        if Self::parse_wsl_worktree(worktree).is_some() {
            return Vec::new();
        }

        worktree.shell_env()
    }
}

impl zed::Extension for SuperwireExtension {
    fn new() -> Self {
        Self { cached_server_path: None }
    }

    fn language_server_command(&mut self, language_server_id: &LanguageServerId, worktree: &zed::Worktree) -> Result<zed::Command> {
        if language_server_id.as_ref() != SERVER_NAME {
            return Err(format!("Unknown language server ID: {}", language_server_id.as_ref()));
        }

        self.command_for_server(worktree)
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        LspSettings::for_worktree(language_server_id.as_ref(), worktree).map(|lsp_settings| lsp_settings.settings)
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        LspSettings::for_worktree(language_server_id.as_ref(), worktree).map(|lsp_settings| lsp_settings.initialization_options)
    }
}

zed::register_extension!(SuperwireExtension);
