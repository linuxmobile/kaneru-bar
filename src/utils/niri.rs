use niri_ipc::{self, Action, Reply, Request, Response};
use std::{
    collections::HashMap,
    env,
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};
use thiserror::Error;

use niri_ipc::socket::SOCKET_PATH_ENV;

pub use niri_ipc::{Output, Window, Workspace};

#[derive(Error, Debug)]
pub enum NiriError {
    #[error("Niri socket path environment variable ({SOCKET_PATH_ENV}) not set")]
    SocketPathNotSet,
    #[error("Failed to connect or clone niri socket: {0}")]
    SocketConnection(io::Error),
    #[error("Failed to send request to niri: {0}")]
    RequestSend(io::Error),
    #[error("Failed to read reply from niri: {0}")]
    ReplyReceive(io::Error),
    #[error("Failed to serialize request: {0}")]
    RequestSerialization(serde_json::Error),
    #[error("Failed to deserialize reply: {0}")]
    ReplyDeserialization(serde_json::Error),
    #[error("Niri returned an error reply: {0}")]
    NiriErrorReply(String),
    #[error("Niri returned an unexpected response type. Expected {expected}, got {got:?}")]
    UnexpectedResponse { expected: String, got: Response },
}

fn get_socket_path() -> Result<PathBuf, NiriError> {
    env::var(SOCKET_PATH_ENV)
        .map(PathBuf::from)
        .map_err(|_| NiriError::SocketPathNotSet)
}

fn send_request<T>(
    request: Request,
    expected_response_fn: fn(Response) -> Option<T>,
    expected_name: &str,
) -> Result<T, NiriError> {
    let socket_path = get_socket_path()?;
    let stream = UnixStream::connect(&socket_path).map_err(NiriError::SocketConnection)?;

    let reader_stream = stream.try_clone().map_err(NiriError::SocketConnection)?;
    let writer_stream = stream;

    let mut reader = BufReader::new(reader_stream);
    let mut writer = BufWriter::new(writer_stream);

    let request_json = serde_json::to_string(&request).map_err(NiriError::RequestSerialization)?;

    writer
        .write_all(request_json.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(NiriError::RequestSend)?;

    drop(writer);

    let mut reply_json = String::new();
    reader
        .read_line(&mut reply_json)
        .map_err(NiriError::ReplyReceive)?;

    if reply_json.is_empty() {
        return Err(NiriError::ReplyReceive(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Niri socket closed before sending reply",
        )));
    }

    let reply: Reply =
        serde_json::from_str(&reply_json).map_err(NiriError::ReplyDeserialization)?;

    match reply {
        Ok(response) => {
            if let Some(data) = expected_response_fn(response.clone()) {
                Ok(data)
            } else {
                Err(NiriError::UnexpectedResponse {
                    expected: expected_name.to_string(),
                    got: response,
                })
            }
        }
        Err(e) => Err(NiriError::NiriErrorReply(e)),
    }
}

fn get_outputs() -> Result<HashMap<String, Output>, NiriError> {
    send_request(
        Request::Outputs,
        |resp| match resp {
            Response::Outputs(outputs) => Some(outputs),
            _ => None,
        },
        "Outputs",
    )
}

fn get_workspaces() -> Result<Vec<Workspace>, NiriError> {
    send_request(
        Request::Workspaces,
        |resp| match resp {
            Response::Workspaces(workspaces) => Some(workspaces),
            _ => None,
        },
        "Workspaces",
    )
}

pub fn get_focused_window() -> Result<Option<Window>, NiriError> {
    send_request(
        Request::FocusedWindow,
        |resp| match resp {
            Response::FocusedWindow(window_option) => Some(window_option),
            _ => None,
        },
        "FocusedWindow",
    )
}

fn get_all_windows() -> Result<Vec<Window>, NiriError> {
    send_request(
        Request::Windows,
        |resp| match resp {
            Response::Windows(windows) => Some(windows),
            _ => None,
        },
        "Windows",
    )
}

fn get_workspaces_by_output() -> Result<HashMap<String, Vec<Workspace>>, NiriError> {
    let workspaces = get_workspaces()?;
    let mut grouped: HashMap<String, Vec<Workspace>> = HashMap::new();
    for ws in workspaces {
        if let Some(output_name) = ws.output.clone() {
            grouped.entry(output_name).or_default().push(ws);
        }
    }
    for ws_list in grouped.values_mut() {
        ws_list.sort_by_key(|ws| ws.idx);
    }
    Ok(grouped)
}

fn perform_action(action: Action) -> Result<(), NiriError> {
    send_request(
        Request::Action(action),
        |resp| match resp {
            Response::Handled => Some(()),
            _ => None,
        },
        "Handled (for Action)",
    )
}
