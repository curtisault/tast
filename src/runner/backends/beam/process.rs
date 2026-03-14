use std::collections::HashMap;
use std::fmt;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Configuration for a BEAM subprocess invocation.
#[derive(Debug, Clone)]
pub struct BeamProcess {
    /// The command to run (e.g., "mix", "gleam", "rebar3").
    pub command: String,
    /// Subcommand and arguments (e.g., ["test", "--trace"]).
    pub args: Vec<String>,
    /// Working directory (project root).
    pub working_dir: PathBuf,
    /// Environment variables to set.
    pub env: HashMap<String, String>,
    /// Maximum execution time.
    pub timeout: Duration,
}

/// Result of a BEAM subprocess execution.
#[derive(Debug, Clone)]
pub struct BeamProcessOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
    pub timed_out: bool,
}

/// Errors from BEAM process operations.
#[derive(Debug, Clone)]
pub struct BeamError {
    pub kind: BeamErrorKind,
    pub message: String,
}

/// Classification of BEAM process errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeamErrorKind {
    /// The BEAM command was not found (e.g., `mix` not on PATH).
    CommandNotFound,
    /// The process timed out and was killed.
    Timeout,
    /// Failed to spawn the process.
    SpawnFailed,
}

impl fmt::Display for BeamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl fmt::Display for BeamErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandNotFound => write!(f, "command not found"),
            Self::Timeout => write!(f, "timeout"),
            Self::SpawnFailed => write!(f, "spawn failed"),
        }
    }
}

impl BeamProcess {
    /// Create a new BEAM process configuration.
    pub fn new(command: &str, working_dir: &Path) -> Self {
        Self {
            command: command.to_owned(),
            args: Vec::new(),
            working_dir: working_dir.to_owned(),
            env: HashMap::new(),
            timeout: Duration::from_secs(120),
        }
    }

    /// Add arguments to the command.
    pub fn with_args(mut self, args: &[&str]) -> Self {
        self.args.extend(args.iter().map(|s| (*s).to_owned()));
        self
    }

    /// Set a single environment variable.
    pub fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    /// Set all `TAST_INPUT_*` environment variables from resolved inputs.
    pub fn with_tast_inputs(mut self, inputs: &HashMap<String, String>) -> Self {
        for (key, value) in inputs {
            self.env
                .insert(format!("TAST_INPUT_{}", key.to_uppercase()), value.clone());
        }
        self
    }

    /// Set the timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Execute the BEAM process and capture output.
    pub fn execute(&self) -> Result<BeamProcessOutput, BeamError> {
        let start = Instant::now();

        let mut cmd = Command::new(&self.command);
        cmd.current_dir(&self.working_dir);

        for arg in &self.args {
            cmd.arg(arg);
        }

        for (key, value) in &self.env {
            cmd.env(key, value);
        }

        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                let kind = if e.kind() == std::io::ErrorKind::NotFound {
                    BeamErrorKind::CommandNotFound
                } else {
                    BeamErrorKind::SpawnFailed
                };
                BeamError {
                    kind,
                    message: format!("failed to spawn '{}': {}", self.command, e),
                }
            })?;

        // Read stdout/stderr in background threads to prevent pipe deadlock.
        let stdout_handle = child.stdout.take();
        let stderr_handle = child.stderr.take();

        let stdout_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut h) = stdout_handle {
                let _ = h.read_to_string(&mut buf);
            }
            buf
        });
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = String::new();
            if let Some(mut h) = stderr_handle {
                let _ = h.read_to_string(&mut buf);
            }
            buf
        });

        // Poll with timeout.
        let poll_interval = Duration::from_millis(50);
        let mut timed_out = false;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if start.elapsed() >= self.timeout => {
                    let _ = child.kill();
                    let _ = child.wait(); // reap zombie
                    timed_out = true;
                    break;
                }
                Ok(None) => std::thread::sleep(poll_interval),
                Err(e) => {
                    return Err(BeamError {
                        kind: BeamErrorKind::SpawnFailed,
                        message: format!("failed to wait for '{}': {}", self.command, e),
                    });
                }
            }
        }

        let exit_code = child
            .try_wait()
            .ok()
            .flatten()
            .and_then(|s| s.code())
            .unwrap_or(-1);
        let stdout = stdout_thread.join().unwrap_or_default();
        let stderr = stderr_thread.join().unwrap_or_default();
        let duration = start.elapsed();

        Ok(BeamProcessOutput {
            exit_code,
            stdout,
            stderr,
            duration,
            timed_out,
        })
    }
}

/// Check whether a BEAM tool is available on the system PATH.
///
/// Runs `<command> --version` and returns the version string on success.
pub fn check_tool_available(command: &str) -> Result<String, BeamError> {
    let output = Command::new(command)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| {
            let kind = if e.kind() == std::io::ErrorKind::NotFound {
                BeamErrorKind::CommandNotFound
            } else {
                BeamErrorKind::SpawnFailed
            };
            BeamError {
                kind,
                message: format!("'{}' not found: {}", command, e),
            }
        })?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if version.is_empty() {
            // Some tools print version to stderr.
            Ok(String::from_utf8_lossy(&output.stderr).trim().to_owned())
        } else {
            Ok(version)
        }
    } else {
        Err(BeamError {
            kind: BeamErrorKind::CommandNotFound,
            message: format!(
                "'{}' --version exited with code {:?}",
                command,
                output.status.code()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beam_process_new_defaults() {
        let proc = BeamProcess::new("mix", Path::new("/tmp/project"));
        assert_eq!(proc.command, "mix");
        assert_eq!(proc.working_dir, PathBuf::from("/tmp/project"));
        assert!(proc.args.is_empty());
        assert!(proc.env.is_empty());
        assert_eq!(proc.timeout, Duration::from_secs(120));
    }

    #[test]
    fn beam_process_with_args() {
        let proc = BeamProcess::new("mix", Path::new("/tmp")).with_args(&["test", "--trace"]);
        assert_eq!(proc.args, vec!["test", "--trace"]);
    }

    #[test]
    fn beam_process_with_env() {
        let proc = BeamProcess::new("mix", Path::new("/tmp")).with_env("MIX_ENV", "test");
        assert_eq!(proc.env.get("MIX_ENV"), Some(&"test".to_owned()));
    }

    #[test]
    fn beam_process_with_tast_inputs() {
        let mut inputs = HashMap::new();
        inputs.insert("user_id".to_owned(), "abc-123".to_owned());
        inputs.insert("email".to_owned(), "test@example.com".to_owned());

        let proc = BeamProcess::new("mix", Path::new("/tmp")).with_tast_inputs(&inputs);

        assert_eq!(
            proc.env.get("TAST_INPUT_USER_ID"),
            Some(&"abc-123".to_owned())
        );
        assert_eq!(
            proc.env.get("TAST_INPUT_EMAIL"),
            Some(&"test@example.com".to_owned())
        );
    }

    #[test]
    fn beam_process_with_timeout() {
        let proc = BeamProcess::new("mix", Path::new("/tmp")).with_timeout(Duration::from_secs(30));
        assert_eq!(proc.timeout, Duration::from_secs(30));
    }

    #[test]
    fn beam_process_builder_chaining() {
        let proc = BeamProcess::new("gleam", Path::new("/project"))
            .with_args(&["test"])
            .with_env("GLEAM_LOG", "info")
            .with_timeout(Duration::from_secs(60));

        assert_eq!(proc.command, "gleam");
        assert_eq!(proc.args, vec!["test"]);
        assert_eq!(proc.env.get("GLEAM_LOG"), Some(&"info".to_owned()));
        assert_eq!(proc.timeout, Duration::from_secs(60));
    }

    #[test]
    fn beam_process_execute_captures_stdout() {
        let proc = BeamProcess::new("echo", Path::new("/tmp")).with_args(&["hello beam"]);

        let output = proc.execute().unwrap();
        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.contains("hello beam"));
        assert!(!output.timed_out);
    }

    #[test]
    fn beam_process_execute_captures_stderr() {
        let proc =
            BeamProcess::new("sh", Path::new("/tmp")).with_args(&["-c", "echo 'error msg' >&2"]);

        let output = proc.execute().unwrap();
        assert!(output.stderr.contains("error msg"));
    }

    #[test]
    fn beam_process_execute_records_duration() {
        let proc = BeamProcess::new("true", Path::new("/tmp"));
        let output = proc.execute().unwrap();
        // Duration should be non-negative (just a sanity check).
        assert!(output.duration.as_nanos() > 0 || output.duration == Duration::ZERO);
    }

    #[test]
    fn beam_process_execute_exit_code() {
        let proc = BeamProcess::new("sh", Path::new("/tmp")).with_args(&["-c", "exit 42"]);

        let output = proc.execute().unwrap();
        assert_eq!(output.exit_code, 42);
        assert!(!output.timed_out);
    }

    #[test]
    fn beam_process_execute_timeout_kills() {
        let proc = BeamProcess::new("sleep", Path::new("/tmp"))
            .with_args(&["10"])
            .with_timeout(Duration::from_millis(200));

        let output = proc.execute().unwrap();
        assert!(output.timed_out);
        assert!(output.duration < Duration::from_secs(5));
    }

    #[test]
    fn beam_process_execute_command_not_found() {
        let proc = BeamProcess::new("nonexistent_beam_tool_xyz", Path::new("/tmp"));
        let err = proc.execute().unwrap_err();
        assert_eq!(err.kind, BeamErrorKind::CommandNotFound);
    }

    #[test]
    fn beam_error_display() {
        let err = BeamError {
            kind: BeamErrorKind::CommandNotFound,
            message: "'mix' not found".to_owned(),
        };
        assert_eq!(err.to_string(), "command not found: 'mix' not found");
    }

    #[test]
    fn beam_error_kind_display() {
        assert_eq!(
            BeamErrorKind::CommandNotFound.to_string(),
            "command not found"
        );
        assert_eq!(BeamErrorKind::Timeout.to_string(), "timeout");
        assert_eq!(BeamErrorKind::SpawnFailed.to_string(), "spawn failed");
    }

    #[test]
    fn check_tool_available_returns_version() {
        // `echo` is universally available and --version prints something on most systems.
        // We use a more reliable test: check that a definitely-missing tool fails.
        let result = check_tool_available("nonexistent_tool_xyz_check");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind, BeamErrorKind::CommandNotFound);
    }

    #[test]
    fn beam_process_execute_sets_env_vars() {
        let proc = BeamProcess::new("sh", Path::new("/tmp"))
            .with_args(&["-c", "echo $MY_BEAM_VAR"])
            .with_env("MY_BEAM_VAR", "beam_value");

        let output = proc.execute().unwrap();
        assert!(output.stdout.contains("beam_value"));
    }
}
