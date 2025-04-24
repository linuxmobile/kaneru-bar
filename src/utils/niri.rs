use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::process::{Command, Output};
use std::string::FromUtf8Error;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum NiriError {
    #[error("Failed to execute niri command: {0}")]
    CommandIo(#[from] io::Error),
    #[error("niri command failed with status: {status}\nStderr: {stderr}")]
    CommandFailed { status: String, stderr: String },
    #[error("Failed to decode command output (stdout) as UTF-8: {0}")]
    OutputUtf8(#[from] FromUtf8Error),
    #[error("Failed to decode command error output (stderr) as UTF-8: {0}")]
    StderrUtf8(FromUtf8Error),
    #[error("Failed to parse JSON output: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("Niri returned unexpected or empty data for '{command}'")]
    UnexpectedData { command: String },
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub description: String,
    pub width: i32,
    pub height: i32,
    pub refresh: i32,
    pub scale: f64,
    pub is_active: bool,
    pub is_focused: bool,
    pub workspaces: Vec<u32>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Workspace {
    pub id: u64,
    pub idx: u32,
    pub output: String,
    pub is_active: bool,
    pub is_urgent: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct Window {
    pub id: u64,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub workspace: Option<u32>,
    #[serde(default)]
    pub is_focused: bool,
    #[serde(default)]
    pub is_fullscreen: bool,
    #[serde(default)]
    pub is_floating: bool,
}

fn exec_niri_cmd(command: &str) -> Result<Output, NiriError> {
    let output = Command::new("niri")
        .args(["msg", "--json", command])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).map_err(NiriError::StderrUtf8)?;
        return Err(NiriError::CommandFailed {
            status: output.status.to_string(),
            stderr,
        });
    }

    Ok(output)
}

fn exec_and_parse<T: DeserializeOwned>(command: &str) -> Result<T, NiriError> {
    let output = exec_niri_cmd(command)?;
    let stdout = String::from_utf8(output.stdout)?;
    let stdout_trimmed = stdout.trim();

    if stdout_trimmed.is_empty() {
        return Err(NiriError::UnexpectedData {
            command: command.to_string(),
        });
    }

    serde_json::from_str(stdout_trimmed).map_err(|e| {
        eprintln!(
            "JSON parsing failed for command '{}'. Input: '{}'",
            command, stdout_trimmed
        );
        NiriError::JsonParse(e)
    })
}

pub fn get_monitors() -> Result<HashMap<String, Monitor>, NiriError> {
    exec_and_parse("outputs")
}

pub fn get_workspaces() -> Result<Vec<Workspace>, NiriError> {
    exec_and_parse("workspaces")
}

pub fn get_focused_window() -> Result<Option<Window>, NiriError> {
    match exec_and_parse::<Option<Window>>("focused-window") {
        Ok(window_option) => Ok(window_option),
        Err(NiriError::UnexpectedData { command }) if command == "focused-window" => Ok(None),
        Err(e @ NiriError::JsonParse(_)) => {
            eprintln!(
                "Warning: Niri returned malformed JSON for focused-window, treating as none: {}",
                e
            );
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

pub fn get_all_windows() -> Result<Vec<Window>, NiriError> {
    exec_and_parse("windows")
}

pub fn perform_action(action: &str, args: &[&str]) -> Result<(), NiriError> {
    let mut command_args = vec!["msg", "action", action];
    command_args.extend_from_slice(args);

    let output = Command::new("niri").args(&command_args).output()?;

    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).map_err(NiriError::StderrUtf8)?;
        Err(NiriError::CommandFailed {
            status: output.status.to_string(),
            stderr,
        })
    } else {
        Ok(())
    }
}

pub fn get_workspaces_by_monitor() -> Result<HashMap<String, Vec<Workspace>>, NiriError> {
    let workspaces = get_workspaces()?;
    let mut grouped: HashMap<String, Vec<Workspace>> = HashMap::new();
    for ws in workspaces {
        grouped.entry(ws.output.clone()).or_default().push(ws);
    }
    for ws_list in grouped.values_mut() {
        ws_list.sort_by_key(|ws| ws.idx);
    }
    Ok(grouped)
}
