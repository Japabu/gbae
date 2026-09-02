use gbae::system::memory::Key;
use std::path::Path;

pub const CONFIG_FILE: &str = "gbae.cfg";

const DEFAULT_KEYS: [&str; 10] = ["KeyZ", "KeyX", "Backspace", "Enter", "ArrowRight", "ArrowLeft", "ArrowUp", "ArrowDown", "KeyS", "KeyA"];

pub struct Settings {
    pub volume: u8,
    pub turbo: bool,
    pub keys: [String; 10],
}

impl Settings {
    pub fn defaults() -> Settings {
        Settings {
            volume: 80,
            turbo: false,
            keys: DEFAULT_KEYS.map(String::from),
        }
    }

    pub fn load(path: &Path) -> Settings {
        let mut settings = Settings::defaults();
        let Ok(text) = std::fs::read_to_string(path) else {
            return settings;
        };
        for line in text.lines() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            match name.trim() {
                "volume" => settings.volume = value.trim().parse().unwrap_or(settings.volume).min(100),
                "turbo" => settings.turbo = value.trim() == "true",
                name => {
                    if let Some(key) = Key::ALL.iter().find(|key| key_setting_name(**key) == name) {
                        settings.keys[*key as usize] = value.trim().to_string();
                    }
                }
            }
        }
        settings
    }

    pub fn save(&self, path: &Path) {
        let mut text = format!("volume={}\nturbo={}\n", self.volume, self.turbo);
        for key in Key::ALL {
            text.push_str(&format!("{}={}\n", key_setting_name(key), self.keys[key as usize]));
        }
        if let Err(error) = std::fs::write(path, text) {
            eprintln!("Could not write {}: {}", path.display(), error);
        }
    }

    pub fn key_for(&self, key_code_name: &str) -> Option<Key> {
        Key::ALL.into_iter().find(|key| self.keys[*key as usize] == key_code_name)
    }
}

pub fn key_setting_name(key: Key) -> &'static str {
    match key {
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

pub fn key_label(key: Key) -> &'static str {
    match key {
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
