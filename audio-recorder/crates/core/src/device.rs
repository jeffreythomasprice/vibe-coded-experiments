use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Direction {
    Input,
    Output,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Direction::Input => "input",
            Direction::Output => "output",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceKey {
    pub direction: Direction,
    pub node_name: String,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid device key {0:?}, expected input:<node> or output:<node>")]
pub struct ParseKeyError(String);

impl DeviceKey {
    pub fn new(direction: Direction, node_name: impl Into<String>) -> Self {
        Self { direction, node_name: node_name.into() }
    }

    pub fn sanitized(&self) -> String {
        let name: String = self
            .node_name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
            .collect();
        format!("{}-{}", self.direction.as_str(), name)
    }
}

impl fmt::Display for DeviceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.direction.as_str(), self.node_name)
    }
}

impl FromStr for DeviceKey {
    type Err = ParseKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (dir, name) = s.split_once(':').ok_or_else(|| ParseKeyError(s.to_string()))?;
        let direction = match dir {
            "input" => Direction::Input,
            "output" => Direction::Output,
            _ => return Err(ParseKeyError(s.to_string())),
        };
        if name.is_empty() {
            return Err(ParseKeyError(s.to_string()));
        }
        Ok(Self { direction, node_name: name.to_string() })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub key: DeviceKey,
    pub description: String,
    pub pw_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    pub key: DeviceKey,
    pub description: String,
    pub present: bool,
    pub enabled: bool,
    pub recording: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_round_trips() {
        let key: DeviceKey = "output:alsa_output.pci-0000_00_1f.3.analog-stereo".parse().unwrap();
        assert_eq!(key.direction, Direction::Output);
        assert_eq!(key.to_string(), "output:alsa_output.pci-0000_00_1f.3.analog-stereo");
    }

    #[test]
    fn key_with_colons_in_name() {
        let key: DeviceKey = "input:a:b".parse().unwrap();
        assert_eq!(key.node_name, "a:b");
    }

    #[test]
    fn bad_keys_rejected() {
        assert!("sink:x".parse::<DeviceKey>().is_err());
        assert!("input:".parse::<DeviceKey>().is_err());
        assert!("nothing".parse::<DeviceKey>().is_err());
    }

    #[test]
    fn sanitized_is_filesystem_safe() {
        let key = DeviceKey::new(Direction::Input, "bluez/input:X Y");
        assert_eq!(key.sanitized(), "input-bluez_input_X_Y");
    }
}
