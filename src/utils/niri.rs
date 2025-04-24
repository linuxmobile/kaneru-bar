use niri_ipc::{self, Action, Reply, Request, Response};
use std::{
    collections::HashMap,
    env,
    error::Error,
    fmt,
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

use niri_ipc::socket::SOCKET_PATH_ENV;

pub use niri_ipc::{Output, Window, Workspace};

#[derive(Debug)]
pub enum NiriError {
    SocketPathNotSet,
    SocketConnection(io::Error),
    RequestSend(io::Error),
    ReplyReceive(io::Error),
    RequestSerialization(serde_json::Error),
    ReplyDeserialization(serde_json::Error),
    NiriErrorReply(String),
    UnexpectedResponse { expected: String, got: Response },
}

impl fmt::Display for NiriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NiriError::SocketPathNotSet => write!(
                f,
                "Niri socket path environment variable ({}) not set",
                SOCKET_PATH_ENV
            ),
            NiriError::SocketConnection(e) => {
                write!(f, "Failed to connect or clone niri socket: {}", e)
            }
            NiriError::RequestSend(e) => write!(f, "Failed to send request to niri: {}", e),
            NiriError::ReplyReceive(e) => write!(f, "Failed to read reply from niri: {}", e),
            NiriError::RequestSerialization(e) => write!(f, "Failed to serialize request: {}", e),
            NiriError::ReplyDeserialization(e) => write!(f, "Failed to deserialize reply: {}", e),
            NiriError::NiriErrorReply(s) => write!(f, "Niri returned an error reply: {}", s),
            NiriError::UnexpectedResponse { expected, got } => write!(
                f,
                "Niri returned an unexpected response type. Expected {}, got {:?}",
                expected, got
            ),
        }
    }
}

impl Error for NiriError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            NiriError::SocketConnection(e) => Some(e),
            NiriError::RequestSend(e) => Some(e),
            NiriError::ReplyReceive(e) => Some(e),
            NiriError::RequestSerialization(e) => Some(e),
            NiriError::ReplyDeserialization(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for NiriError {
    fn from(err: io::Error) -> Self {
        NiriError::ReplyReceive(err)
    }
}
impl From<serde_json::Error> for NiriError {
    fn from(err: serde_json::Error) -> Self {
        NiriError::ReplyDeserialization(err)
    }
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    // Added pub
    send_request(
        Request::FocusedWindow,
        |resp| match resp {
            Response::FocusedWindow(window_option) => Some(window_option),
            _ => None,
        },
        "FocusedWindow",
    )
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
