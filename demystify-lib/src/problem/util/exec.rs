use std::process::Command;
use std::sync::OnceLock;
use which::which;

/// Enum representing the method used to run commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMethod {
    Native,
    Docker,
    Podman,
}

impl std::str::FromStr for RunMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "native" => Ok(RunMethod::Native),
            "docker" => Ok(RunMethod::Docker),
            "podman" => Ok(RunMethod::Podman),
            _ => Err(format!("Invalid RunMethod: {}", s)),
        }
    }
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
    /// Prepare a `Command` to run a program, either natively or in a container
    pub fn prepare(program: &str, localdir: &std::path::Path) -> Command {
        match get_run_method() {
            RunMethod::Native => {
                // Create a native command
                let mut cmd = Command::new(program);
                cmd.current_dir(localdir);
                cmd
            }
            RunMethod::Docker | RunMethod::Podman => {
                let container_cmd = if get_run_method() == RunMethod::Docker {
                    "docker"
                } else {
                    "podman"
                };

                // Build the container command
                let mut container_command = Command::new(container_cmd);
                container_command
                    .current_dir(localdir)
                    .arg("run")
                    .arg("--rm")
                    .arg("-v")
                    .arg(".:/workspace:Z")
                    .arg("-w")
                    .arg("/workspace")
                    .arg("ghcr.io/conjure-cp/conjure:main")
                    .arg(program);

                container_command
            }
        }
    }
}
