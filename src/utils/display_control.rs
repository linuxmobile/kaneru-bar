use brightness::Brightness;
use futures_util::stream::TryStreamExt;
use futures_util::TryFutureExt;
use std::{
    error::Error,
    fmt,
    io,
    process::Stdio,
    sync::atomic::{AtomicU32, Ordering},
};
use tokio::{runtime::Handle, task};

pub const MIN_TEMP: u32 = 2500;
pub const MAX_TEMP: u32 = 6500;
pub const DEFAULT_TEMP: u32 = 3500;

static CURRENT_TEMP: AtomicU32 = AtomicU32::new(DEFAULT_TEMP);

#[derive(Debug)]
pub enum DisplayControlError {
    Brightness(brightness::Error),
    Io(io::Error),
    NoDevice,
    TaskJoinError(String),
    InvalidState(String),
}

impl fmt::Display for DisplayControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayControlError::Brightness(e) => write!(f, "Brightness error: {}", e),
            DisplayControlError::Io(e) => write!(f, "I/O error: {}", e),
            DisplayControlError::NoDevice => write!(f, "No display device found"),
            DisplayControlError::TaskJoinError(e) => write!(f, "Task join error: {}", e),
            DisplayControlError::InvalidState(s) => write!(f, "Invalid state: {}", s),
        }
    }
}

impl Error for DisplayControlError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DisplayControlError::Brightness(e) => Some(e),
            DisplayControlError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<brightness::Error> for DisplayControlError {
    fn from(err: brightness::Error) -> Self {
        DisplayControlError::Brightness(err)
    }
}

impl From<io::Error> for DisplayControlError {
    fn from(err: io::Error) -> Self {
        DisplayControlError::Io(err)
    }
}

impl From<tokio::task::JoinError> for DisplayControlError {
    fn from(err: tokio::task::JoinError) -> Self {
        DisplayControlError::TaskJoinError(err.to_string())
    }
}

pub async fn get_brightness() -> Result<f64, DisplayControlError> {
    task::spawn_blocking(|| {
        let handle = Handle::current();
        handle.block_on(async {
            let maybe_device = brightness::brightness_devices()
                .try_next()
                .map_err(DisplayControlError::from)
                .await?;

            match maybe_device {
                Some(device) => device
                    .get()
                    .map_err(DisplayControlError::from)
                    .await
                    .map(|level| level as f64 / 100.0),
                None => Err(DisplayControlError::NoDevice),
            }
        })
    })
    .await?
}

pub async fn set_brightness(level: f64) -> Result<(), DisplayControlError> {
    let level_u32 = (level.clamp(0.0, 1.0) * 100.0).round() as u32;
    task::spawn_blocking(move || {
        let handle = Handle::current();
        handle.block_on(async move {
            let maybe_device = brightness::brightness_devices()
                .try_next()
                .map_err(DisplayControlError::from)
                .await?;

            match maybe_device {
                Some(mut device) => {
                    device
                        .set(level_u32)
                        .map_err(DisplayControlError::from)
                        .await
                }
                None => Err(DisplayControlError::NoDevice),
            }
        })
    })
    .await?
}

pub async fn is_night_light_on() -> Result<bool, DisplayControlError> {
    let output = tokio::process::Command::new("pgrep")
        .args(["-x", "wlsunset"])
        .output()
        .await?;

    // Check for successful execution of pgrep
    if output.status.success() {
        // If the command succeeded, wlsunset is running
        Ok(true)
    } else {
        // Check exit code to distinguish between "not found" and other errors
        match output.status.code() {
            Some(1) => Ok(false), // Exit code 1 from pgrep means no processes matched
            _ => Err(DisplayControlError::Io(io::Error::new(
                io::ErrorKind::Other,
                format!("pgrep failed with exit code: {:?}", output.status.code())
            ))),
        }
    }
}

pub async fn set_night_light(enable: bool, temp: u32) -> Result<(), DisplayControlError> {
    let temp_clamped = temp.clamp(MIN_TEMP, MAX_TEMP);

    // First check if wlsunset is running
    let is_running = is_night_light_on().await?;

    // If it's running, we need to kill it (either to disable or to restart with new settings)
    if is_running {
        let kill_result = tokio::process::Command::new("pkill")
            .args(["-TERM", "-x", "wlsunset"])
            .status()
            .await;

        // Wait a brief moment to ensure the process is terminated
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if let Err(e) = kill_result {
            eprintln!("Warning: Failed to terminate existing wlsunset process: {}", e);
        }
    }

    if !enable {
        CURRENT_TEMP.store(DEFAULT_TEMP, Ordering::SeqCst);
        return Ok(());
    }

    // Launch wlsunset with the specified temperature
    let mut cmd = tokio::process::Command::new("wlsunset");
    cmd.arg("-T")
       .arg(temp_clamped.to_string())
       .stdout(Stdio::null())
       .stderr(Stdio::null());

    let child = cmd.spawn()?;

    if child.id().is_none() {
        return Err(DisplayControlError::InvalidState("Failed to start wlsunset".into()));
    }

    // Update the stored temperature
    CURRENT_TEMP.store(temp_clamped, Ordering::SeqCst);
    Ok(())
}

pub async fn get_color_temperature() -> Result<u32, DisplayControlError> {
    // Check if night light is on first
    let is_on = is_night_light_on().await?;

    if is_on {
        // Return the cached temperature value
        Ok(CURRENT_TEMP.load(Ordering::SeqCst))
    } else {
        // If night light is off, return the default temperature
        // This ensures consistency between UI and actual state
        Ok(DEFAULT_TEMP)
    }
}

pub fn kelvin_to_slider(kelvin: u32) -> f64 {
    ((kelvin.clamp(MIN_TEMP, MAX_TEMP) - MIN_TEMP) as f64 / (MAX_TEMP - MIN_TEMP) as f64)
        .clamp(0.0, 1.0)
}

pub fn slider_to_kelvin(slider_value: f64) -> u32 {
    (MIN_TEMP as f64 + slider_value.clamp(0.0, 1.0) * (MAX_TEMP - MIN_TEMP) as f64)
        .round() as u32
}
