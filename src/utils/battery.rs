use battery::{
    units::{
        electric_potential::volt, energy::watt_hour, power::watt, ratio::percent, time::second,
        ElectricPotential, Energy, Power, Ratio, Time,
    },
    Battery, Manager, State,
};
use std::{
    collections::HashSet,
    error::Error,
    fmt, fs, io,
    path::Path,
    process::{Command, Output, Stdio},
    str::FromStr,
    time::Duration,
};

const CONSERVATION_MODE_PATH: &str =
    "/sys/devices/pci0000:00/0000:00:14.3/PNP0C09:00/VPC2004:00/conservation_mode";

#[derive(Debug)]
pub enum BatteryUtilError {
    Io(io::Error),
    BatteryManager(battery::Error),
    NoBatteryFound,
    CommandNotFound(String, io::Error),
    CommandFailed(String, String),
    ParseError(String),
    SysfsNotFound(String),
    PermissionDenied(String),
    Utf8Error(std::string::FromUtf8Error),
}

impl fmt::Display for BatteryUtilError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BatteryUtilError::Io(e) => write!(f, "I/O error: {}", e),
            BatteryUtilError::BatteryManager(e) => write!(f, "Battery manager error: {}", e),
            BatteryUtilError::NoBatteryFound => write!(f, "No battery device found"),
            BatteryUtilError::CommandNotFound(cmd, e) => {
                write!(f, "Command not found '{}': {}", cmd, e)
            }
            BatteryUtilError::CommandFailed(cmd, stderr) => {
                write!(f, "Command failed '{}': {}", cmd, stderr)
            }
            BatteryUtilError::ParseError(s) => write!(f, "Parse error: {}", s),
            BatteryUtilError::SysfsNotFound(path) => write!(f, "Sysfs path not found: {}", path),
            BatteryUtilError::PermissionDenied(path) => {
                write!(f, "Permission denied for path: {}", path)
            }
            BatteryUtilError::Utf8Error(e) => write!(f, "UTF8 conversion error: {}", e),
        }
    }
}

impl Error for BatteryUtilError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            BatteryUtilError::Io(e) => Some(e),
            BatteryUtilError::BatteryManager(e) => Some(e),
            BatteryUtilError::CommandNotFound(_, e) => Some(e),
            BatteryUtilError::Utf8Error(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for BatteryUtilError {
    fn from(err: io::Error) -> Self {
        BatteryUtilError::Io(err)
    }
}

impl From<battery::Error> for BatteryUtilError {
    fn from(err: battery::Error) -> Self {
        BatteryUtilError::BatteryManager(err)
    }
}

impl From<std::string::FromUtf8Error> for BatteryUtilError {
    fn from(err: std::string::FromUtf8Error) -> Self {
        BatteryUtilError::Utf8Error(err)
    }
}

#[derive(Debug, Clone, Default)]
pub struct BatteryDetails {
    pub percentage: Option<f32>,
    pub state: Option<State>,
    pub time_to_full: Option<Duration>,
    pub time_to_empty: Option<Duration>,
    pub energy_rate_watts: Option<f32>,
    pub voltage_volts: Option<f32>,
    pub health_percentage: Option<f32>,
    pub cycle_count: Option<u32>,
    pub icon_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
    Unknown(String),
}

impl fmt::Display for PowerProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PowerProfile::PowerSaver => write!(f, "power-saver"),
            PowerProfile::Balanced => write!(f, "balanced"),
            PowerProfile::Performance => write!(f, "performance"),
            PowerProfile::Unknown(s) => write!(f, "{}", s),
        }
    }
}

impl FromStr for PowerProfile {
    type Err = BatteryUtilError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "power-saver" => Ok(PowerProfile::PowerSaver),
            "balanced" => Ok(PowerProfile::Balanced),
            "performance" => Ok(PowerProfile::Performance),
            other => Ok(PowerProfile::Unknown(other.to_string())),
        }
    }
}

fn run_command(program: &str, args: &[&str]) -> Result<Output, BatteryUtilError> {
    let output = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| BatteryUtilError::CommandNotFound(program.to_string(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(BatteryUtilError::CommandFailed(
            format!("{} {}", program, args.join(" ")),
            stderr,
        ))
    } else {
        Ok(output)
    }
}

fn parse_percentage(value: Ratio) -> f32 {
    value.get::<percent>()
}

fn parse_duration(value: Time) -> Option<Duration> {
    let secs = value.get::<second>();
    if secs.is_finite() && secs > 0.0 {
        Some(Duration::from_secs_f32(secs))
    } else {
        None
    }
}

fn parse_power(value: Power) -> f32 {
    value.get::<watt>()
}

fn parse_voltage(value: ElectricPotential) -> f32 {
    value.get::<volt>()
}

#[allow(dead_code)]
fn parse_energy(value: Energy) -> f32 {
    value.get::<watt_hour>()
}

fn get_icon_name(percentage: Option<f32>, state: Option<State>) -> Option<String> {
    let result = match (state, percentage) {
        (Some(s), Some(p)) => {
            let percentage_int = p.round() as u8;
            let icon_str = match s {
                State::Charging => match percentage_int {
                    0..=10 => "battery-caution-charging-symbolic",
                    11..=30 => "battery-low-charging-symbolic",
                    31..=95 => "battery-good-charging-symbolic",
                    _ => "battery-full-charging-symbolic",
                },
                State::Discharging => match percentage_int {
                    0..=10 => "battery-caution-symbolic",
                    11..=30 => "battery-low-symbolic",
                    31..=95 => "battery-good-symbolic",
                    _ => "battery-full-symbolic",
                },
                State::Full => "battery-full-charged-symbolic",
                State::Empty => "battery-empty-symbolic",
                State::Unknown => match percentage_int {
                    0..=10 => "battery-caution-symbolic",
                    11..=30 => "battery-low-symbolic",
                    31..=95 => "battery-good-symbolic",
                    _ => "battery-full-symbolic",
                },
                _ => "battery-missing-symbolic",
            };
            Some(icon_str.to_string())
        }
        _ => Some("battery-missing-symbolic".to_string()),
    };
    result
}

pub struct BatteryService {
    manager: Manager,
}

impl BatteryService {
    pub fn new() -> Result<Self, BatteryUtilError> {
        let manager = Manager::new()?;
        Ok(Self { manager })
    }

    fn find_primary_battery(&self) -> Result<Battery, BatteryUtilError> {
        self.manager
            .batteries()?
            .next()
            .ok_or(BatteryUtilError::NoBatteryFound)?
            .map_err(BatteryUtilError::BatteryManager)
    }

    pub fn get_primary_battery_details(&mut self) -> Result<BatteryDetails, BatteryUtilError> {
        let mut battery = self.find_primary_battery()?;
        self.manager.refresh(&mut battery)?;

        let percentage_val = battery.state_of_charge();
        let state_val = battery.state();
        let time_to_full_val = battery.time_to_full();
        let time_to_empty_val = battery.time_to_empty();
        let energy_rate_val = battery.energy_rate();
        let voltage_val = battery.voltage();
        let health_val = battery.state_of_health();
        let cycle_count_val = battery.cycle_count();

        let percentage = Some(parse_percentage(percentage_val));
        let state = Some(state_val);
        let time_to_full = time_to_full_val.and_then(parse_duration);
        let time_to_empty = time_to_empty_val.and_then(parse_duration);
        let energy_rate = Some(parse_power(energy_rate_val));
        let voltage = Some(parse_voltage(voltage_val));
        let health = Some(parse_percentage(health_val));
        let icon_name = get_icon_name(percentage, state);

        let details = BatteryDetails {
            percentage,
            state,
            time_to_full,
            time_to_empty,
            energy_rate_watts: energy_rate,
            voltage_volts: voltage,
            health_percentage: health,
            cycle_count: cycle_count_val,
            icon_name,
        };
        Ok(details)
    }
}

pub fn get_active_power_profile() -> Result<PowerProfile, BatteryUtilError> {
    let output = run_command("powerprofilesctl", &["get"])?;
    let profile_str = String::from_utf8(output.stdout)?.trim().to_string();
    PowerProfile::from_str(&profile_str)
}

pub fn set_power_profile(profile: PowerProfile) -> Result<(), BatteryUtilError> {
    run_command("powerprofilesctl", &["set", &profile.to_string()])?;
    Ok(())
}

pub fn get_available_power_profiles() -> Result<Vec<PowerProfile>, BatteryUtilError> {
    let output = run_command("powerprofilesctl", &["list"])?;
    let list_str = String::from_utf8(output.stdout)?;

    let mut profiles = Vec::new();
    let mut seen = HashSet::new();

    if list_str.contains("power-saver") {
        if seen.insert(PowerProfile::PowerSaver) {
            profiles.push(PowerProfile::PowerSaver);
        }
    }
    if list_str.contains("balanced") {
        if seen.insert(PowerProfile::Balanced) {
            profiles.push(PowerProfile::Balanced);
        }
    }
    if list_str.contains("performance") {
        if seen.insert(PowerProfile::Performance) {
            profiles.push(PowerProfile::Performance);
        }
    }

    profiles.sort_by_key(|p| match p {
        PowerProfile::PowerSaver => 0,
        PowerProfile::Balanced => 1,
        PowerProfile::Performance => 2,
        PowerProfile::Unknown(_) => 3,
    });
    Ok(profiles)
}

pub fn get_conservation_mode() -> Result<bool, BatteryUtilError> {
    let path = Path::new(CONSERVATION_MODE_PATH);
    if !path.exists() {
        return Err(BatteryUtilError::SysfsNotFound(
            CONSERVATION_MODE_PATH.to_string(),
        ));
    }
    match fs::read_to_string(path) {
        Ok(content) => match content.trim().parse::<u8>() {
            Ok(1) => Ok(true),
            Ok(0) => Ok(false),
            _ => Err(BatteryUtilError::ParseError(format!(
                "Unexpected content in {}: {}",
                CONSERVATION_MODE_PATH, content
            ))),
        },
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Err(
            BatteryUtilError::PermissionDenied(CONSERVATION_MODE_PATH.to_string()),
        ),
        Err(e) => Err(BatteryUtilError::Io(e)),
    }
}

pub fn set_conservation_mode(enabled: bool) -> Result<(), BatteryUtilError> {
    let value = if enabled { "1" } else { "0" };
    let command_str = format!("echo {} > {}", value, CONSERVATION_MODE_PATH);

    let output = Command::new("sudo")
        .args(["-n", "sh", "-c", &command_str])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| BatteryUtilError::CommandNotFound("sudo".to_string(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(BatteryUtilError::CommandFailed(
            format!("sudo -n sh -c '{}'", command_str),
            stderr,
        ))
    } else {
        Ok(())
    }
}

pub fn format_time_option(duration: Option<Duration>) -> String {
    match duration {
        Some(d) => {
            let total_seconds = d.as_secs();
            let hours = total_seconds / 3600;
            let minutes = (total_seconds % 3600) / 60;
            if hours > 0 {
                format!("{}h {:02}m", hours, minutes)
            } else {
                format!("{}m", minutes)
            }
        }
        None => "".to_string(),
    }
}

pub fn format_charge_status(details: &BatteryDetails) -> String {
    match details.state {
        Some(State::Charging) => {
            let time_str = format_time_option(details.time_to_full);
            if time_str.is_empty() {
                "Charging".to_string()
            } else {
                format!("Charging ({})", time_str)
            }
        }
        Some(State::Discharging) => {
            let time_str = format_time_option(details.time_to_empty);
            if time_str.is_empty() {
                "Discharging".to_string()
            } else {
                format!("{} remaining", time_str)
            }
        }
        Some(State::Full) => "Fully Charged".to_string(),
        Some(State::Empty) => "Empty".to_string(),
        Some(State::Unknown) => "Calculating...".to_string(),
        None => "State N/A".to_string(),
        _ => "Calculating...".to_string(),
    }
}
