use gbae::system::memory::Key;
use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

pub const CONFIG_FILE: &str = "gbae.cfg";

const DEFAULT_KEYS: [&str; 10] = ["KeyZ", "KeyX", "Backspace", "Enter", "ArrowRight", "ArrowLeft", "ArrowUp", "ArrowDown", "KeyS", "KeyA"];

pub trait KeyNames {
    fn setting_name(self) -> &'static str;
    fn label(self) -> &'static str;
}

impl KeyNames for Key {
    fn setting_name(self) -> &'static str {
        match self {
            Key::A => "key.a",
            Key::B => "key.b",
            Key::Select => "key.select",
            Key::Start => "key.start",
            Key::Right => "key.right",
            Key::Left => "key.left",
            Key::Up => "key.up",
            Key::Down => "key.down",
            Key::R => "key.r",
            Key::L => "key.l",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Key::A => "A",
            Key::B => "B",
            Key::Select => "Select",
            Key::Start => "Start",
            Key::Right => "Right",
            Key::Left => "Left",
            Key::Up => "Up",
            Key::Down => "Down",
            Key::R => "R",
            Key::L => "L",
        }
    }
}

pub struct Settings {
    pub volume: u8,
    pub turbo: bool,
    pub smooth_audio: bool,
    pub keys: [String; 10],
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            volume: 80,
            turbo: false,
            smooth_audio: false,
            keys: DEFAULT_KEYS.map(String::from),
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read_to_string(path).ok().and_then(|text| text.parse().ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        if let Err(error) = std::fs::write(path, self.to_string()) {
            eprintln!("Could not write {}: {}", path.display(), error);
        }
    }

    pub fn binding(&self, key: Key) -> &str {
        &self.keys[key as usize]
    }

    pub fn bind(&mut self, key: Key, key_code_name: String) {
        self.keys[key as usize] = key_code_name;
    }

    pub fn key_for(&self, key_code_name: &str) -> Option<Key> {
        Key::ALL.into_iter().find(|key| self.binding(*key) == key_code_name)
    }
}

impl FromStr for Settings {
    type Err = std::convert::Infallible;

    fn from_str(text: &str) -> Result<Settings, Self::Err> {
        let mut settings = Settings::default();
        for (name, value) in text.lines().filter_map(|line| line.split_once('=')) {
            let (name, value) = (name.trim(), value.trim());
            match name {
                "volume" => settings.volume = value.parse::<u32>().map_or(settings.volume, |volume| volume.min(100) as u8),
                "turbo" => settings.turbo = value == "true",
                "smooth_audio" => settings.smooth_audio = value == "true",
                _ => {
                    if let Some(key) = Key::ALL.into_iter().find(|key| key.setting_name() == name) {
                        settings.bind(key, value.to_string());
                    }
                }
            }
        }
        Ok(settings)
    }
}

impl Display for Settings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "volume={}", self.volume)?;
        writeln!(f, "turbo={}", self.turbo)?;
        writeln!(f, "smooth_audio={}", self.smooth_audio)?;
        for key in Key::ALL {
            writeln!(f, "{}={}", key.setting_name(), self.binding(key))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_round_trip() {
        let mut settings = Settings::default();
        settings.volume = 40;
        settings.smooth_audio = true;
        settings.bind(Key::L, "KeyQ".to_string());
        let parsed: Settings = settings.to_string().parse().unwrap();
        assert_eq!(parsed.volume, 40);
        assert!(parsed.smooth_audio && !parsed.turbo);
        assert_eq!(parsed.key_for("KeyQ"), Some(Key::L));
        assert_eq!(parsed.key_for("KeyZ"), Some(Key::A));
    }

    #[test]
    fn test_unknown_lines_and_bad_values_are_ignored() {
        let parsed: Settings = "volume=999\nnonsense\nother=1\n".parse().unwrap();
        assert_eq!(parsed.volume, 100);
        assert_eq!(parsed.binding(Key::A), "KeyZ");
    }
}
