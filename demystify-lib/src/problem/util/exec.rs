use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;
use std::io;
use which::which;

/// Enum representing the method used to run commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMethod {
    Native,
    Docker,
    Podman,
}

/// Global configuration for the runner
pub static RUN_METHOD: OnceLock<RunMethod> = OnceLock::new();

/// Get the current run method, auto-detecting if not already initialized
pub fn get_run_method() -> RunMethod {
    *RUN_METHOD.get_or_init(|| detect_run_method())
}

/// Set the run method explicitly
pub fn set_run_method(method: RunMethod) {
    let _ = RUN_METHOD.set(method);
}

/// Auto-detect the best available run method
fn detect_run_method() -> RunMethod {
    // Check if we have the necessary tools for native execution
    if which("conjure").is_ok() && which("savilerow").is_ok() {
        return RunMethod::Native;
    }
    
    // Check for container tools
    if which("podman").is_ok() {
        return RunMethod::Podman;
    }
    
    if which("docker").is_ok() {
        return RunMethod::Docker;
    }
    
    // Default to native if we couldn't detect anything
    // This might fail later, but at least we tried
    RunMethod::Native
}

/// Program runner to execute commands in different environments
pub struct ProgramRunner;

impl ProgramRunner {
    /// Run a command from the specified directory
    pub fn run(program: &str, args: &[&str], working_dir: impl AsRef<Path>) -> io::Result<Output> {
        match get_run_method() {
            RunMethod::Native => {
                let mut command = Command::new(program);
                command.args(args);
                command.current_dir(working_dir.as_ref());
                command.output()
            },
            RunMethod::Docker | RunMethod::Podman => {
                let container_cmd = if get_run_method() == RunMethod::Docker { "docker" } else { "podman" };
                
                // Get the absolute path to working directory
                let abs_path = working_dir.as_ref().canonicalize()?;
                
                // Build the container command
                let mut container_command = Command::new(container_cmd);
                container_command
                    .arg("run")
                    .arg("--rm")
                    .arg("-v")
                    .arg(format!("{}:/workspace:Z", abs_path.to_str()
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Path is not valid UTF-8"))?))
                    .arg("-w")
                    .arg("/workspace")
                    .arg("ghcr.io/conjure-cp/conjure:main")
                    .arg(program).args(args).current_dir(working_dir.as_ref());
                
                container_command.output()
            }
        }
    }
}