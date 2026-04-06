use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use zed_extension_api::settings::LspSettings;
use zed_extension_api::{self as zed, LanguageServerId, Result};

const SERVER_NAME: &str = "superwire-lsp";
const EXTENSION_ID: &str = "superwire";
const SERVER_PATH_ENVIRONMENT_VARIABLE: &str = "SUPERWIRE_LSP_PATH";
const BUNDLED_BINARY_DIRECTORY_NAME: &str = "bin";

struct SuperwireExtension {
    cached_server_path: Option<String>,
}

impl SuperwireExtension {
    fn command_for_server(&mut self, language_server_id: &LanguageServerId, worktree: &zed::Worktree) -> Result<zed::Command> {
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
            return Ok(Self::server_command(configured_server_path, command_arguments));
        }

        if let Some(environment_server_path) = Self::resolve_environment_server_path() {
            return Ok(Self::server_command(environment_server_path, command_arguments));
        }

        if let Some(cached_server_path) = self.cached_server_path.clone() {
            if Self::is_executable_server_binary(&cached_server_path) {
                return Ok(Self::server_command(cached_server_path, command_arguments));
            }
        }

        if let Some(bundled_server_path) = Self::resolve_bundled_server_path() {
            self.cached_server_path = Some(bundled_server_path.clone());

            return Ok(Self::server_command(bundled_server_path, command_arguments));
        }

        if let Some(worktree_bundled_server_path) = Self::resolve_worktree_bundled_server_path(worktree) {
            self.cached_server_path = Some(worktree_bundled_server_path.clone());

            return Ok(Self::server_command(worktree_bundled_server_path, command_arguments));
        }

        if let Some(path_server_path) = worktree.which(SERVER_NAME) {
            self.cached_server_path = Some(path_server_path.clone());

            return Ok(Self::server_command(path_server_path, command_arguments));
        }

        if let Some(local_server_path) = Self::resolve_local_development_server_path(worktree) {
            self.cached_server_path = Some(local_server_path.clone());

            return Ok(Self::server_command(local_server_path, command_arguments));
        }

        if let Some(workspace_server_command) = Self::build_workspace_server_command(language_server_id, worktree, &command_arguments)? {
            return Ok(workspace_server_command);
        }

        Err(format!(
            "Could not find a Superwire language server binary. Expected bundled binary at {}",
            Self::bundled_binary_path_hint()
        ))
    }

    fn resolve_environment_server_path() -> Option<String> {
        let configured_server_path = env::var(SERVER_PATH_ENVIRONMENT_VARIABLE).ok()?;

        if Self::is_executable_server_binary(&configured_server_path) {
            return Some(configured_server_path);
        }

        None
    }

    fn resolve_bundled_server_path() -> Option<String> {
        let current_directory = env::current_dir().ok()?;
        let extension_directories = Self::candidate_extension_directories(&current_directory);
        let bundled_directory_name = Self::bundled_platform_directory_name();
        let binary_filename = Self::server_binary_filename();

        for extension_directory in extension_directories {
            let candidate_paths = [
                extension_directory
                    .join(BUNDLED_BINARY_DIRECTORY_NAME)
                    .join(&bundled_directory_name)
                    .join(&binary_filename),
                extension_directory.join(BUNDLED_BINARY_DIRECTORY_NAME).join(&binary_filename),
            ];

            for candidate_path in candidate_paths {
                if Self::is_executable_server_binary_path(&candidate_path) {
                    return Some(candidate_path.to_string_lossy().to_string());
                }
            }
        }

        None
    }

    fn candidate_extension_directories(current_directory: &Path) -> Vec<PathBuf> {
        let mut extension_directories = Vec::new();

        Self::push_candidate_directory(&mut extension_directories, current_directory.to_path_buf());

        Self::push_candidate_directory(
            &mut extension_directories,
            current_directory.join("..").join("..").join("installed").join(EXTENSION_ID),
        );

        Self::push_candidate_directory(
            &mut extension_directories,
            current_directory.join("..").join("..").join(EXTENSION_ID),
        );

        if let Some(local_app_data_directory) = env::var_os("LOCALAPPDATA") {
            Self::push_candidate_directory(
                &mut extension_directories,
                PathBuf::from(local_app_data_directory)
                    .join("Zed")
                    .join("extensions")
                    .join("installed")
                    .join(EXTENSION_ID),
            );
        }

        if let Some(home_directory) = env::var_os("HOME") {
            Self::push_candidate_directory(
                &mut extension_directories,
                PathBuf::from(home_directory)
                    .join(".local")
                    .join("share")
                    .join("zed")
                    .join("extensions")
                    .join("installed")
                    .join(EXTENSION_ID),
            );
        }

        extension_directories
    }

    fn push_candidate_directory(extension_directories: &mut Vec<PathBuf>, candidate_directory: PathBuf) {
        if extension_directories.contains(&candidate_directory) {
            return;
        }

        extension_directories.push(candidate_directory);
    }

    fn resolve_local_development_server_path(worktree: &zed::Worktree) -> Option<String> {
        let server_binary_filename = Self::server_binary_filename();
        let worktree_root_directory = PathBuf::from(worktree.root_path());

        for ancestor_directory in worktree_root_directory.ancestors().take(4) {
            let candidate_directories = [ancestor_directory.join("target/release"), ancestor_directory.join("target/debug")];

            for candidate_directory in candidate_directories {
                let candidate_server_path = candidate_directory.join(&server_binary_filename);

                if Self::is_executable_server_binary_path(&candidate_server_path) {
                    return Some(candidate_server_path.to_string_lossy().to_string());
                }
            }
        }

        None
    }

    fn resolve_worktree_bundled_server_path(worktree: &zed::Worktree) -> Option<String> {
        let worktree_root_directory = PathBuf::from(worktree.root_path());
        let bundled_directory_name = Self::bundled_platform_directory_name();
        let server_binary_filename = Self::server_binary_filename();

        let candidate_paths = [
            worktree_root_directory
                .join("editors")
                .join("zed")
                .join(BUNDLED_BINARY_DIRECTORY_NAME)
                .join(&bundled_directory_name)
                .join(&server_binary_filename),
            worktree_root_directory
                .join(BUNDLED_BINARY_DIRECTORY_NAME)
                .join(&bundled_directory_name)
                .join(&server_binary_filename),
            worktree_root_directory
                .join(BUNDLED_BINARY_DIRECTORY_NAME)
                .join(&server_binary_filename),
        ];

        for candidate_path in candidate_paths {
            if Self::is_executable_server_binary_path(&candidate_path) {
                return Some(candidate_path.to_string_lossy().to_string());
            }
        }

        None
    }

    fn build_workspace_server_command(
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
        command_arguments: &[String],
    ) -> Result<Option<zed::Command>> {
        if worktree.read_text_file("crates/lsp/Cargo.toml").is_err() {
            return Ok(None);
        }

        zed::set_language_server_installation_status(language_server_id, &zed::LanguageServerInstallationStatus::CheckingForUpdate);

        let Some(cargo_binary_path) = Self::resolve_cargo_binary_path(worktree) else {
            let error_message = format!(
                "Could not find cargo executable for workspace fallback. Bundle the LSP at {}",
                Self::bundled_binary_path_hint()
            );

            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Failed(error_message.clone()),
            );

            return Err(error_message);
        };
        let lsp_manifest_path = PathBuf::from(worktree.root_path())
            .join("crates")
            .join("lsp")
            .join("Cargo.toml")
            .to_string_lossy()
            .to_string();

        let mut cargo_arguments = vec![
            "run".to_string(),
            "--manifest-path".to_string(),
            lsp_manifest_path,
            "--bin".to_string(),
            SERVER_NAME.to_string(),
            "--".to_string(),
        ];

        cargo_arguments.extend(command_arguments.iter().cloned());

        Ok(Some(zed::Command {
            command: cargo_binary_path,
            args: cargo_arguments,
            env: Vec::new(),
        }))
    }

    fn resolve_cargo_binary_path(worktree: &zed::Worktree) -> Option<String> {
        if let Some(cargo_binary_path) = worktree.which("cargo") {
            return Some(cargo_binary_path);
        }

        if let Some(cargo_binary_path) = worktree.which("cargo.exe") {
            return Some(cargo_binary_path);
        }

        if let Ok(cargo_binary_path) = env::var("CARGO") {
            if Self::is_executable_server_binary(&cargo_binary_path) {
                return Some(cargo_binary_path);
            }
        }

        if let Ok(user_profile_directory) = env::var("USERPROFILE") {
            let cargo_binary_path = PathBuf::from(user_profile_directory).join(".cargo").join("bin").join("cargo.exe");

            if Self::is_executable_server_binary_path(&cargo_binary_path) {
                return Some(cargo_binary_path.to_string_lossy().to_string());
            }
        }

        if let Ok(home_directory) = env::var("HOME") {
            let cargo_binary_path = PathBuf::from(home_directory).join(".cargo").join("bin").join("cargo");

            if Self::is_executable_server_binary_path(&cargo_binary_path) {
                return Some(cargo_binary_path.to_string_lossy().to_string());
            }

            let windows_cargo_binary_path = cargo_binary_path.with_extension("exe");

            if Self::is_executable_server_binary_path(&windows_cargo_binary_path) {
                return Some(windows_cargo_binary_path.to_string_lossy().to_string());
            }
        }

        None
    }

    fn server_command(server_path: String, command_arguments: Vec<String>) -> zed::Command {
        zed::Command {
            command: server_path,
            args: command_arguments,
            env: Vec::new(),
        }
    }

    fn server_binary_filename() -> String {
        let (operating_system, _) = zed::current_platform();

        if operating_system == zed::Os::Windows {
            return format!("{}.exe", SERVER_NAME);
        }

        SERVER_NAME.to_string()
    }

    fn bundled_platform_directory_name() -> String {
        let (operating_system, architecture) = zed::current_platform();

        let operating_system_name = match operating_system {
            zed::Os::Mac => "macos",
            zed::Os::Linux => "linux",
            zed::Os::Windows => "windows",
        };
        let architecture_name = match architecture {
            zed::Architecture::Aarch64 => "aarch64",
            zed::Architecture::X86 => "x86",
            zed::Architecture::X8664 => "x86_64",
        };

        format!("{}-{}", operating_system_name, architecture_name)
    }

    fn bundled_binary_path_hint() -> String {
        Path::new(BUNDLED_BINARY_DIRECTORY_NAME)
            .join(Self::bundled_platform_directory_name())
            .join(Self::server_binary_filename())
            .to_string_lossy()
            .to_string()
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
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
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

        self.command_for_server(language_server_id, worktree)
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
