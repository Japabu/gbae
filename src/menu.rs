use crate::config::{KeyNames, Settings};
use crate::font::{glyph, GLYPH_HEIGHT, GLYPH_WIDTH};
use gbae::system::memory::Key;
use gbae::system::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use winit::keyboard::KeyCode;

const VISIBLE_FILES: usize = 9;
const LINE_WIDTH: usize = 24;
const VOLUME_STEP: u8 = 10;

const TEXT: [u8; 3] = [235, 235, 235];
const HIGHLIGHT: [u8; 3] = [255, 200, 60];
const DIM: [u8; 3] = [120, 120, 120];
const PANEL: [u8; 3] = [24, 24, 32];
const BORDER: [u8; 3] = [90, 90, 110];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    Reset,
    SaveState,
    LoadState,
    LoadRom(PathBuf),
    Quit,
    SettingsChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MainItem {
    Resume,
    Reset,
    SaveState,
    LoadState,
    LoadRom,
    Settings,
    Quit,
}

impl MainItem {
    const ALL: [MainItem; 7] = [
        MainItem::Resume,
        MainItem::Reset,
        MainItem::SaveState,
        MainItem::LoadState,
        MainItem::LoadRom,
        MainItem::Settings,
        MainItem::Quit,
    ];
}

impl Display for MainItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            MainItem::Resume => "Resume",
            MainItem::Reset => "Reset",
            MainItem::SaveState => "Save state",
            MainItem::LoadState => "Load state",
            MainItem::LoadRom => "Load ROM...",
            MainItem::Settings => "Settings",
            MainItem::Quit => "Quit",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingsItem {
    Volume,
    Speed,
    Sound,
    Controls,
    Back,
}

impl SettingsItem {
    const ALL: [SettingsItem; 5] = [SettingsItem::Volume, SettingsItem::Speed, SettingsItem::Sound, SettingsItem::Controls, SettingsItem::Back];

    fn line(self, settings: &Settings) -> String {
        match self {
            SettingsItem::Volume => format!("Volume      < {:>3}% >", settings.volume),
            SettingsItem::Speed => format!("Speed       < {} >", if settings.turbo { "Turbo" } else { "1x" }),
            SettingsItem::Sound => format!("Sound       < {} >", if settings.smooth_audio { "Smooth" } else { "Exact" }),
            SettingsItem::Controls => "Controls...".to_string(),
            SettingsItem::Back => "Back".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Main,
    Settings,
    Controls,
    Files,
}

struct FileEntry {
    name: String,
    path: PathBuf,
    is_directory: bool,
}

struct Line {
    text: String,
    selected: bool,
    enabled: bool,
}

pub struct Menu {
    pub open: bool,
    screen: Screen,
    index: usize,
    capturing: Option<Key>,
    directory: PathBuf,
    entries: Vec<FileEntry>,
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            open: false,
            screen: Screen::Main,
            index: 0,
            capturing: None,
            directory: PathBuf::new(),
            entries: Vec::new(),
        }
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.show(Screen::Main, 0);
    }

    pub fn set_directory(&mut self, directory: &Path) {
        self.directory = directory.to_path_buf();
    }

    pub fn browse(&mut self, directory: &Path) {
        self.open = true;
        self.directory = directory.to_path_buf();
        self.entries = list_directory(directory);
        self.show(Screen::Files, 0);
    }

    fn show(&mut self, screen: Screen, index: usize) {
        self.screen = screen;
        self.index = index;
        self.capturing = None;
    }

    fn item_count(&self) -> usize {
        match self.screen {
            Screen::Main => MainItem::ALL.len(),
            Screen::Settings => SettingsItem::ALL.len(),
            Screen::Controls => Key::ALL.len() + 1,
            Screen::Files => self.entries.len().max(1),
        }
    }

    pub fn key(&mut self, code: KeyCode, settings: &mut Settings) -> Action {
        if let Some(key) = self.capturing.take() {
            settings.bind(key, format!("{:?}", code));
            return Action::SettingsChanged;
        }
        let count = self.item_count();
        match code {
            KeyCode::ArrowUp => self.index = (self.index + count - 1) % count,
            KeyCode::ArrowDown => self.index = (self.index + 1) % count,
            KeyCode::ArrowLeft | KeyCode::ArrowRight => return self.adjust(code == KeyCode::ArrowRight, settings),
            KeyCode::Enter | KeyCode::Space => return self.select(settings),
            KeyCode::Escape => return self.back(),
            _ => {}
        }
        Action::None
    }

    fn adjust(&mut self, increase: bool, settings: &mut Settings) -> Action {
        if self.screen != Screen::Settings {
            return Action::None;
        }
        match SettingsItem::ALL[self.index] {
            SettingsItem::Volume => {
                settings.volume = if increase {
                    (settings.volume + VOLUME_STEP).min(100)
                } else {
                    settings.volume.saturating_sub(VOLUME_STEP)
                };
            }
            SettingsItem::Speed => settings.turbo = !settings.turbo,
            SettingsItem::Sound => settings.smooth_audio = !settings.smooth_audio,
            SettingsItem::Controls | SettingsItem::Back => return Action::None,
        }
        Action::SettingsChanged
    }

    fn select(&mut self, settings: &mut Settings) -> Action {
        match self.screen {
            Screen::Main => match MainItem::ALL[self.index] {
                MainItem::Resume => Action::Close,
                MainItem::Reset => Action::Reset,
                MainItem::SaveState => Action::SaveState,
                MainItem::LoadState => Action::LoadState,
                MainItem::LoadRom => {
                    let directory = self.directory.clone();
                    self.browse(&directory);
                    Action::None
                }
                MainItem::Settings => {
                    self.show(Screen::Settings, 0);
                    Action::None
                }
                MainItem::Quit => Action::Quit,
            },
            Screen::Settings => match SettingsItem::ALL[self.index] {
                SettingsItem::Controls => {
                    self.show(Screen::Controls, 0);
                    Action::None
                }
                SettingsItem::Back => self.back(),
                _ => self.adjust(true, settings),
            },
            Screen::Controls => match Key::ALL.get(self.index) {
                Some(key) => {
                    self.capturing = Some(*key);
                    Action::None
                }
                None => self.back(),
            },
            Screen::Files => match self.entries.get(self.index) {
                Some(entry) if entry.is_directory => {
                    let path = entry.path.clone();
                    self.browse(&path);
                    Action::None
                }
                Some(entry) => Action::LoadRom(entry.path.clone()),
                None => Action::None,
            },
        }
    }

    fn back(&mut self) -> Action {
        let main_position = |item: MainItem| MainItem::ALL.iter().position(|candidate| *candidate == item).unwrap_or(0);
        match self.screen {
            Screen::Main => return Action::Close,
            Screen::Settings => self.show(Screen::Main, main_position(MainItem::Settings)),
            Screen::Controls => self.show(Screen::Settings, SettingsItem::ALL.iter().position(|item| *item == SettingsItem::Controls).unwrap_or(0)),
            Screen::Files => self.show(Screen::Main, main_position(MainItem::LoadRom)),
        }
        Action::None
    }

    pub fn render(&self, frame: &mut Framebuffer, settings: &Settings) {
        for pixel in frame.iter_mut().flatten() {
            *pixel = pixel.map(|channel| channel / 3);
        }
        let (title, lines) = self.lines(settings);
        let height = (lines.len() + 3) * (GLYPH_HEIGHT + 2);
        let top = (FRAMEBUFFER_HEIGHT - height) / 2;
        fill_rect(frame, 16, top - 6, FRAMEBUFFER_WIDTH - 32, height + 12, PANEL);
        draw_border(frame, 16, top - 6, FRAMEBUFFER_WIDTH - 32, height + 12, BORDER);
        draw_text(frame, 24, top, title, HIGHLIGHT);
        for (i, line) in lines.iter().enumerate() {
            let y = top + (i + 2) * (GLYPH_HEIGHT + 2);
            let color = if line.selected {
                HIGHLIGHT
            } else if line.enabled {
                TEXT
            } else {
                DIM
            };
            draw_text(frame, 24, y, if line.selected { ">" } else { " " }, HIGHLIGHT);
            draw_text(frame, 24 + GLYPH_WIDTH * 2, y, &line.text, color);
        }
    }

    fn lines(&self, settings: &Settings) -> (&'static str, Vec<Line>) {
        let line = |index: usize, text: String, enabled: bool| Line {
            text,
            selected: index == self.index,
            enabled,
        };
        match self.screen {
            Screen::Main => ("GBAE", MainItem::ALL.iter().enumerate().map(|(i, item)| line(i, item.to_string(), true)).collect()),
            Screen::Settings => ("Settings", SettingsItem::ALL.iter().enumerate().map(|(i, item)| line(i, item.line(settings), true)).collect()),
            Screen::Controls => {
                let bindings = Key::ALL.iter().enumerate().map(|(i, key)| {
                    let binding = if self.capturing == Some(*key) { "press a key" } else { settings.binding(*key) };
                    line(i, format!("{:<8}{}", key.label(), binding), true)
                });
                ("Controls", bindings.chain(std::iter::once(line(Key::ALL.len(), "Back".to_string(), true))).collect())
            }
            Screen::Files => {
                let first = self.index.saturating_sub(VISIBLE_FILES - 1).min(self.entries.len().saturating_sub(VISIBLE_FILES));
                let lines = self
                    .entries
                    .iter()
                    .enumerate()
                    .skip(first)
                    .take(VISIBLE_FILES)
                    .map(|(i, entry)| line(i, truncate(&entry.name, LINE_WIDTH), !entry.is_directory))
                    .collect();
                ("Load ROM", lines)
            }
        }
    }
}

fn list_directory(directory: &Path) -> Vec<FileEntry> {
    let mut directories = Vec::new();
    let mut roms = Vec::new();
    for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            directories.push(FileEntry {
                name: format!("{}/", name),
                path,
                is_directory: true,
            });
        } else if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("gba")) {
            roms.push(FileEntry { name, path, is_directory: false });
        }
    }
    let by_name = |a: &FileEntry, b: &FileEntry| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    directories.sort_by(by_name);
    roms.sort_by(by_name);
    let parent = directory.parent().map(|parent| FileEntry {
        name: "../".to_string(),
        path: parent.to_path_buf(),
        is_directory: true,
    });
    parent.into_iter().chain(directories).chain(roms).collect()
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        format!("{}~", text.chars().take(width - 1).collect::<String>())
    }
}

pub fn draw_text(frame: &mut Framebuffer, x: usize, y: usize, text: &str, color: [u8; 3]) {
    for (i, character) in text.chars().enumerate() {
        for (row, bits) in glyph(character).iter().enumerate() {
            for column in (0..GLYPH_WIDTH).filter(|column| bits >> column & 1 != 0) {
                let (px, py) = (x + i * GLYPH_WIDTH + column, y + row);
                if let Some(pixel) = frame.get_mut(py).and_then(|row| row.get_mut(px)) {
                    *pixel = color;
                }
            }
        }
    }
}

fn fill_rect(frame: &mut Framebuffer, x: usize, y: usize, width: usize, height: usize, color: [u8; 3]) {
    for row in frame.iter_mut().skip(y).take(height) {
        for pixel in row.iter_mut().skip(x).take(width) {
            *pixel = color;
        }
    }
}

fn draw_border(frame: &mut Framebuffer, x: usize, y: usize, width: usize, height: usize, color: [u8; 3]) {
    fill_rect(frame, x, y, width, 1, color);
    fill_rect(frame, x, y + height - 1, width, 1, color);
    fill_rect(frame, x, y, 1, height, color);
    fill_rect(frame, x + width - 1, y, 1, height, color);
}
