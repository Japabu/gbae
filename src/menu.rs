use crate::config::{key_label, Settings};
use crate::font::{glyph, GLYPH_HEIGHT, GLYPH_WIDTH};
use gbae::system::memory::Key;
use gbae::system::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use winit::keyboard::KeyCode;

const TEXT: [u8; 3] = [235, 235, 235];
const HIGHLIGHT: [u8; 3] = [255, 200, 60];
const DIM: [u8; 3] = [120, 120, 120];
const PANEL: [u8; 3] = [24, 24, 32];
const BORDER: [u8; 3] = [90, 90, 110];

const MAIN_ITEMS: [&str; 7] = ["Resume", "Reset", "Save state", "Load state", "Load ROM...", "Settings", "Quit"];
const SETTINGS_ITEMS: [&str; 4] = ["Volume", "Speed", "Controls...", "Back"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    Reset,
    SaveState,
    LoadState,
    LoadRom,
    Quit,
    SettingsChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Main,
    Settings,
    Controls,
}

pub struct Menu {
    pub open: bool,
    screen: Screen,
    index: usize,
    capturing: Option<Key>,
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            open: false,
            screen: Screen::Main,
            index: 0,
            capturing: None,
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.screen = Screen::Main;
        self.index = 0;
        self.capturing = None;
    }

    pub fn key(&mut self, code: KeyCode, settings: &mut Settings) -> Action {
        if let Some(key) = self.capturing {
            settings.keys[key as usize] = format!("{:?}", code);
            self.capturing = None;
            return Action::SettingsChanged;
        }
        let item_count = match self.screen {
            Screen::Main => MAIN_ITEMS.len(),
            Screen::Settings => SETTINGS_ITEMS.len(),
            Screen::Controls => Key::ALL.len() + 1,
        };
        match code {
            KeyCode::ArrowUp => self.index = (self.index + item_count - 1) % item_count,
            KeyCode::ArrowDown => self.index = (self.index + 1) % item_count,
            KeyCode::ArrowLeft | KeyCode::ArrowRight => return self.adjust(code == KeyCode::ArrowRight, settings),
            KeyCode::Enter | KeyCode::Space => return self.select(settings),
            KeyCode::Escape => return self.back(),
            _ => {}
        }
        Action::None
    }

    fn adjust(&mut self, increase: bool, settings: &mut Settings) -> Action {
        if self.screen == Screen::Settings {
            match self.index {
                0 => {
                    settings.volume = if increase { (settings.volume + 10).min(100) } else { settings.volume.saturating_sub(10) };
                    return Action::SettingsChanged;
                }
                1 => {
                    settings.turbo = !settings.turbo;
                    return Action::SettingsChanged;
                }
                _ => {}
            }
        }
        Action::None
    }

    fn select(&mut self, settings: &mut Settings) -> Action {
        match self.screen {
            Screen::Main => match self.index {
                0 => Action::Close,
                1 => Action::Reset,
                2 => Action::SaveState,
                3 => Action::LoadState,
                4 => Action::LoadRom,
                5 => {
                    self.screen = Screen::Settings;
                    self.index = 0;
                    Action::None
                }
                _ => Action::Quit,
            },
            Screen::Settings => match self.index {
                0 | 1 => self.adjust(true, settings),
                2 => {
                    self.screen = Screen::Controls;
                    self.index = 0;
                    Action::None
                }
                _ => self.back(),
            },
            Screen::Controls => {
                if self.index < Key::ALL.len() {
                    self.capturing = Some(Key::ALL[self.index]);
                    Action::None
                } else {
                    self.back()
                }
            }
        }
    }

    fn back(&mut self) -> Action {
        match self.screen {
            Screen::Main => Action::Close,
            Screen::Settings => {
                self.screen = Screen::Main;
                self.index = 5;
                Action::None
            }
            Screen::Controls => {
                self.screen = Screen::Settings;
                self.index = 2;
                Action::None
            }
        }
    }

    pub fn render(&self, frame: &mut Framebuffer, settings: &Settings) {
        for row in frame.iter_mut() {
            for pixel in row.iter_mut() {
                *pixel = pixel.map(|channel| channel / 3);
            }
        }
        let (title, lines) = self.lines(settings);
        let height = (lines.len() + 3) * (GLYPH_HEIGHT + 2);
        let top = (FRAMEBUFFER_HEIGHT - height) / 2;
        fill_rect(frame, 16, top - 6, FRAMEBUFFER_WIDTH - 32, height + 12, PANEL);
        draw_border(frame, 16, top - 6, FRAMEBUFFER_WIDTH - 32, height + 12, BORDER);
        draw_text(frame, 24, top, title, HIGHLIGHT);
        for (i, (text, selected, enabled)) in lines.iter().enumerate() {
            let y = top + (i + 2) * (GLYPH_HEIGHT + 2);
            let color = if *selected { HIGHLIGHT } else if *enabled { TEXT } else { DIM };
            draw_text(frame, 24, y, if *selected { ">" } else { " " }, HIGHLIGHT);
            draw_text(frame, 24 + GLYPH_WIDTH * 2, y, text, color);
        }
    }

    fn lines(&self, settings: &Settings) -> (&'static str, Vec<(String, bool, bool)>) {
        match self.screen {
            Screen::Main => ("GBAE", MAIN_ITEMS.iter().enumerate().map(|(i, item)| (item.to_string(), i == self.index, true)).collect()),
            Screen::Settings => {
                let values = [
                    format!("Volume      < {:>3}% >", settings.volume),
                    format!("Speed       < {} >", if settings.turbo { "Turbo" } else { "1x" }),
                    SETTINGS_ITEMS[2].to_string(),
                    SETTINGS_ITEMS[3].to_string(),
                ];
                ("Settings", values.into_iter().enumerate().map(|(i, item)| (item, i == self.index, true)).collect())
            }
            Screen::Controls => {
                let mut lines: Vec<(String, bool, bool)> = Key::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, key)| {
                        let binding = if self.capturing == Some(*key) { "press a key".to_string() } else { settings.keys[*key as usize].clone() };
                        (format!("{:<8}{}", key_label(*key), binding), i == self.index, true)
                    })
                    .collect();
                lines.push(("Back".to_string(), self.index == Key::ALL.len(), true));
                ("Controls", lines)
            }
        }
    }
}

pub fn draw_text(frame: &mut Framebuffer, x: usize, y: usize, text: &str, color: [u8; 3]) {
    for (i, character) in text.chars().enumerate() {
        let bitmap = glyph(character);
        for (row, bits) in bitmap.iter().enumerate() {
            for column in 0..GLYPH_WIDTH {
                if bits >> column & 1 != 0 {
                    let (px, py) = (x + i * GLYPH_WIDTH + column, y + row);
                    if px < FRAMEBUFFER_WIDTH && py < FRAMEBUFFER_HEIGHT {
                        frame[py][px] = color;
                    }
                }
            }
        }
    }
}

fn fill_rect(frame: &mut Framebuffer, x: usize, y: usize, width: usize, height: usize, color: [u8; 3]) {
    for py in y..(y + height).min(FRAMEBUFFER_HEIGHT) {
        for px in x..(x + width).min(FRAMEBUFFER_WIDTH) {
            frame[py][px] = color;
        }
    }
}

fn draw_border(frame: &mut Framebuffer, x: usize, y: usize, width: usize, height: usize, color: [u8; 3]) {
    fill_rect(frame, x, y, width, 1, color);
    fill_rect(frame, x, y + height - 1, width, 1, color);
    fill_rect(frame, x, y, 1, height, color);
    fill_rect(frame, x + width - 1, y, 1, height, color);
}
