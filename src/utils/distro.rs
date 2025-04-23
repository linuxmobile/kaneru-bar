use std::{
    collections::HashMap,
    error::Error,
    fmt, fs,
    io::{self, BufRead, BufReader},
    path::Path,
};

const OS_RELEASE_PATH: &str = "/etc/os-release";

#[derive(Debug)]
pub enum DistroInfoError {
    Io(io::Error),
    ParseError(String),
}

impl fmt::Display for DistroInfoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DistroInfoError::Io(e) => write!(f, "I/O error reading os-release: {}", e),
            DistroInfoError::ParseError(s) => write!(f, "Failed to parse os-release line: {}", s),
        }
    }
}

impl Error for DistroInfoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DistroInfoError::Io(e) => Some(e),
            DistroInfoError::ParseError(_) => None,
        }
    }
}

impl From<io::Error> for DistroInfoError {
    fn from(err: io::Error) -> Self {
        DistroInfoError::Io(err)
    }
}

fn parse_os_release(path: &Path) -> Result<HashMap<String, String>, DistroInfoError> {
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut vars = HashMap::new();

    for line_result in reader.lines() {
        let line = line_result?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            vars.insert(key, value);
        } else {
            return Err(DistroInfoError::ParseError(line.to_string()));
        }
    }

    Ok(vars)
}

pub fn get_distro_icon_name() -> Result<Option<String>, DistroInfoError> {
    let os_release_path = Path::new(OS_RELEASE_PATH);
    if !os_release_path.exists() {
        return Ok(None);
    }

    let vars = parse_os_release(os_release_path)?;

    if let Some(logo) = vars.get("LOGO") {
        if !logo.is_empty() {
            return Ok(Some(logo.clone()));
        }
    }

    if let Some(id) = vars.get("ID") {
        if !id.is_empty() {
            return Ok(Some(id.clone()));
        }
    }

    Ok(None)
}
