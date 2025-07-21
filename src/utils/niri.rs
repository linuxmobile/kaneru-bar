use niri_ipc::socket::SOCKET_PATH_ENV;
use niri_ipc::{self, Reply, Request, Response};
use std::{
    env,
    error::Error,
    fmt,
    io::{self, BufRead, BufReader, BufWriter, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};

pub use niri_ipc::Window;

#[derive(Debug)]
pub enum NiriError {
    SocketPathNotSet,
    Connection(io::Error),
    IPC(io::Error),
    Serialization(serde_json::Error),
    Deserialization(serde_json::Error),
    NiriErrorReply(String),
    UnexpectedResponse {
        expected: &'static str,
        got: Response,
    },
}

impl fmt::Display for NiriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NiriError::SocketPathNotSet => write!(
                f,
                "Niri socket path environment variable ({}) not set",
                SOCKET_PATH_ENV
            ),
            NiriError::Connection(e) => write!(f, "Failed to connect to niri socket: {}", e),
            NiriError::IPC(e) => write!(f, "Niri IPC communication error: {}", e),
            NiriError::Serialization(e) => write!(f, "Failed to serialize request: {}", e),
            NiriError::Deserialization(e) => write!(f, "Failed to deserialize reply: {}", e),
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
            NiriError::Connection(e) | NiriError::IPC(e) => Some(e),
            NiriError::Serialization(e) => Some(e),
            NiriError::Deserialization(e) => Some(e),
            _ => None,
        }
    }
}

fn send_request<T>(
    request: Request,
    expected_response_fn: fn(Response) -> Option<T>,
    expected_name: &'static str,
) -> Result<T, NiriError> {
    let socket_path = env::var(SOCKET_PATH_ENV)
        .map(PathBuf::from)
        .map_err(|_| NiriError::SocketPathNotSet)?;

    let stream = UnixStream::connect(&socket_path).map_err(NiriError::Connection)?;
    let reader_stream = stream.try_clone().map_err(NiriError::Connection)?;
    let writer_stream = stream;

    let mut writer = BufWriter::new(writer_stream);
    let mut reader = BufReader::new(reader_stream);

    let request_json = serde_json::to_string(&request).map_err(NiriError::Serialization)?;

    writer
        .write_all(request_json.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(NiriError::IPC)?;

    drop(writer);

    let mut reply_json = String::new();
    reader.read_line(&mut reply_json).map_err(NiriError::IPC)?;

    if reply_json.is_empty() {
        return Err(NiriError::IPC(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Niri socket closed before sending reply",
        )));
    }

    let reply: Reply = serde_json::from_str(&reply_json).map_err(NiriError::Deserialization)?;

    match reply {
        Ok(response) => {
            if let Some(data) = expected_response_fn(response.clone()) {
                Ok(data)
            } else {
                Err(NiriError::UnexpectedResponse {
                    expected: expected_name,
                    got: response,
                })
            }
        }
        Err(e) => Err(NiriError::NiriErrorReply(e)),
    }
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

pub fn get_windows() -> Result<Vec<Window>, NiriError> {
    send_request(
        Request::Windows,
        |resp| match resp {
            Response::Windows(windows) => Some(windows),
            _ => None,
        },
        "Windows",
    )
}
