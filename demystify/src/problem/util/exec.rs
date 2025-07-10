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
            _ => Err(format!("Invalid RunMethod: {s}")),
        }
    }
}

impl std::fmt::Display for RunMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunMethod::Native => write!(f, "native"),
            RunMethod::Docker => write!(f, "docker"),
            RunMethod::Podman => write!(f, "podman"),
        }
    }
}

/// Global configuration for the runner
pub static RUN_METHOD: OnceLock<RunMethod> = OnceLock::new();

/// Get the current run method, auto-detecting if not already initialized
pub fn get_run_method() -> RunMethod {
    *RUN_METHOD.get_or_init(detect_run_method)
}

/// Set the run method explicitly
pub fn set_run_method(method: RunMethod) {
    let _ = RUN_METHOD.set(method);
}

/// Auto-detect the best available run method
fn detect_run_method() -> RunMethod {
    // Check if we have the necessary tools for native execution
    if which("conjure").is_ok() && which("savilerow").is_ok() {
        let output = Command::new("conjure")
            .arg("--version")
            .output()
            .expect("Failed to execute conjure --version");

        if !output.status.success() {
            eprintln!(
                "ERROR: The command 'conjure' does not appear to be constraint tool, you have another 'conjure' program installed:"
            );
            eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
            eprintln!("Going to try docker or podman instead.")
        } else {
            return RunMethod::Native;
        }
    }

    // Check for container tools
    if which("podman").is_ok() {
        eprintln!("Using podman");
        return RunMethod::Podman;
    }

    if which("docker").is_ok() {
        eprintln!("Using docker");
        return RunMethod::Docker;
    }

    // Default to native if we couldn't detect anything
    // This might fail later, but at least we tried
    RunMethod::Native
}

/// Program runner to execute commands in different environments
pub struct ProgramRunner;

impl ProgramRunner {
    /// Run `conjure --version` and return its output
    pub fn get_conjure_version() -> Result<String, String> {
        let current_dir =
            std::env::current_dir().map_err(|e| format!("Failed to get current directory: {e}"))?;
        let mut cmd = Self::prepare("conjure", &current_dir);
        cmd.arg("--version");

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to execute conjure: {e}"))?;

        if output.status.success() {
            Ok("Using ".to_owned()
                + &get_run_method().to_string()
                + " conjure, version:\n"
                + &String::from_utf8_lossy(&output.stdout))
        } else {
            Err(format!(
                "Conjure failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Prepare a `Command` to run a program, either natively or in a container
    #[must_use]
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
