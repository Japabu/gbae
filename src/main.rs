mod audio;
mod config;
mod font;
mod menu;

use audio::Audio;
use config::Settings;
use gbae::cartridge::CartridgeInfo;
use gbae::system::apu::SAMPLE_RATE;
use gbae::system::cpu::CPU_FREQUENCY;
use gbae::system::gba::{Gba, CYCLES_PER_SCANLINE, SCANLINES_PER_FRAME};
use gbae::system::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use menu::{Action, Menu};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const USAGE: &str = "usage: gbae [-h | --help] [-V | --version] [ROM]";
const SAVE_FLUSH_INTERVAL_FRAMES: u32 = 60;
const WORK_SLICE: Duration = Duration::from_millis(8);
const AUDIO_QUEUE_FRACTION_OF_SECOND: u32 = 20;
const NOTICE_DURATION: Duration = Duration::from_millis(1500);

fn frame_duration() -> Duration {
    Duration::from_secs_f64(CYCLES_PER_SCANLINE as f64 * SCANLINES_PER_FRAME as f64 / CPU_FREQUENCY as f64)
}

#[derive(Debug, PartialEq, Eq)]
enum Arguments {
    Run(Option<PathBuf>),
    Help,
    Version,
}

fn parse_arguments(arguments: impl Iterator<Item = String>) -> Result<Arguments, String> {
    let mut rom = None;
    let mut options_ended = false;
    for argument in arguments {
        match argument.as_str() {
            "--" if !options_ended => options_ended = true,
            "-h" | "--help" if !options_ended => return Ok(Arguments::Help),
            "-V" | "--version" if !options_ended => return Ok(Arguments::Version),
            option if !options_ended && option.starts_with('-') && option != "-" => return Err(format!("unknown option {}", option)),
            _ if rom.is_some() => return Err("more than one ROM given".to_string()),
            _ => rom = Some(PathBuf::from(&argument)),
        }
    }
    Ok(Arguments::Run(rom))
}

struct Notice {
    text: String,
    shown_at: Instant,
}

struct Emulator {
    gba: Gba,
    rom: Vec<u8>,
    save_path: PathBuf,
    state_path: PathBuf,
    title: String,
    audio: Option<Audio>,
    settings: Settings,
    config_path: Option<PathBuf>,
    menu: Menu,
    turbo: bool,
    notice: Option<Notice>,
    frames_since_save_check: u32,
}

impl Emulator {
    fn new(settings: Settings, config_path: Option<PathBuf>, audio: Option<Audio>) -> Emulator {
        let mut emulator = Emulator {
            gba: Gba::new(Vec::new()),
            rom: Vec::new(),
            save_path: PathBuf::new(),
            state_path: PathBuf::new(),
            title: "no ROM".to_string(),
            audio,
            settings,
            config_path,
            menu: Menu::new(),
            turbo: false,
            notice: None,
            frames_since_save_check: 0,
        };
        emulator.configure_audio();
        emulator
    }

    fn speed(&self) -> Option<f64> {
        if self.turbo {
            self.settings.turbo.multiplier()
        } else {
            Some(1.0)
        }
    }

    fn configure_audio(&mut self) {
        let sample_rate = self.audio.as_ref().map_or(SAMPLE_RATE, Audio::sample_rate);
        let multiplier = self.speed().unwrap_or(1.0);
        self.gba.set_audio_sample_rate((f64::from(sample_rate) / multiplier).round() as u32);
    }

    fn needs_audio(&self) -> bool {
        self.audio
            .as_ref()
            .is_some_and(|audio| audio.queued_frames() < (audio.sample_rate() / AUDIO_QUEUE_FRACTION_OF_SECOND) as usize)
    }

    fn save_settings(&self) {
        match &self.config_path {
            Some(path) => self.settings.save(path),
            None => eprintln!("gbae: no configuration directory, settings are not saved"),
        }
    }

    fn apply_settings(&mut self) {
        if let Some(audio) = &self.audio {
            audio.set_volume(self.settings.volume);
        }
        self.configure_audio();
        self.save_settings();
    }

    fn toggle_turbo(&mut self) {
        self.turbo = !self.turbo;
        self.configure_audio();
        let speed = if self.turbo { self.settings.turbo.to_string() } else { "1x".to_string() };
        self.notice = Some(Notice {
            text: format!("Speed {}", speed),
            shown_at: Instant::now(),
        });
    }

    fn rom_directory(&self) -> PathBuf {
        self.save_path
            .parent()
            .map(Path::to_path_buf)
            .filter(|directory| !directory.as_os_str().is_empty())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    fn load_rom(&mut self, rom_path: &Path) -> std::io::Result<()> {
        let rom = std::fs::read(rom_path)?;
        self.flush_save(true);
        self.title = CartridgeInfo::parse(&rom).map(|cartridge| cartridge.title.trim().to_string()).unwrap_or_default();
        let rom_path = rom_path.canonicalize().unwrap_or_else(|_| rom_path.to_path_buf());
        self.save_path = rom_path.with_extension("sav");
        self.state_path = rom_path.with_extension("state");
        self.rom = rom;
        self.reset();
        self.menu.set_directory(&self.rom_directory());
        eprintln!("Loaded {} ({}), save type {:?}", rom_path.display(), self.title, self.gba.save_type());
        Ok(())
    }

    fn reset(&mut self) {
        self.flush_save(true);
        self.gba = Gba::new(self.rom.clone());
        self.configure_audio();
        if let Ok(save) = std::fs::read(&self.save_path) {
            self.gba.load_save_data(&save);
        }
        if let Some(audio) = &self.audio {
            audio.clear();
        }
    }

    fn run_frame(&mut self) {
        if let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) {
            self.gba.set_time(now.as_secs());
        }
        self.gba.run_frame();
        let samples = self.gba.take_audio_samples();
        if let (Some(audio), Some(_)) = (&self.audio, self.speed()) {
            audio.push(&samples);
        }
        self.frames_since_save_check += 1;
        if self.frames_since_save_check >= SAVE_FLUSH_INTERVAL_FRAMES {
            self.frames_since_save_check = 0;
            self.flush_save(false);
        }
    }

    fn flush_save(&mut self, force: bool) {
        if (self.gba.take_save_dirty() || force) && !self.gba.save_data().is_empty() && !self.save_path.as_os_str().is_empty() {
            if let Err(error) = std::fs::write(&self.save_path, self.gba.save_data()) {
                eprintln!("gbae: cannot write {}: {}", self.save_path.display(), error);
            }
        }
    }

    fn save_state(&self) {
        match std::fs::write(&self.state_path, self.gba.save_state()) {
            Ok(()) => eprintln!("State saved to {}", self.state_path.display()),
            Err(error) => eprintln!("gbae: cannot write {}: {}", self.state_path.display(), error),
        }
    }

    fn load_state(&mut self) {
        let bytes = match std::fs::read(&self.state_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("gbae: cannot read {}: {}", self.state_path.display(), error);
                return;
            }
        };
        match self.gba.load_state(&bytes) {
            Ok(()) => {
                if let Some(audio) = &self.audio {
                    audio.clear();
                }
                eprintln!("State loaded from {}", self.state_path.display());
            }
            Err(error) => {
                eprintln!("gbae: cannot load state: {}", error);
                self.reset();
            }
        }
    }

    fn key_event(&mut self, code: KeyCode, pressed: bool) -> Action {
        if self.menu.open {
            if pressed {
                let action = self.menu.key(code, &mut self.settings);
                if action == Action::SettingsChanged {
                    self.apply_settings();
                }
                return action;
            }
        } else if code == KeyCode::Escape {
            if pressed {
                self.menu.toggle();
            }
        } else if code == KeyCode::Tab {
            if pressed {
                self.toggle_turbo();
            }
        } else if let Some(key) = self.settings.key_for(&format!("{:?}", code)) {
            self.gba.set_key(key, pressed);
        }
        Action::None
    }

    fn compose(&self) -> Framebuffer {
        let mut frame = *self.gba.framebuffer();
        if self.menu.open {
            self.menu.render(&mut frame, &self.settings);
        } else if let Some(notice) = self.notice.as_ref().filter(|notice| notice.shown_at.elapsed() < NOTICE_DURATION) {
            menu::draw_notice(&mut frame, &notice.text);
        }
        frame
    }
}

struct App {
    context: softbuffer::Context<OwnedDisplayHandle>,
    window: Option<Arc<dyn Window>>,
    surface: Option<softbuffer::Surface<OwnedDisplayHandle, Arc<dyn Window>>>,
    emulator: Emulator,
    next_frame: Instant,
}

impl App {
    fn present(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.surface_size();
        let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if surface.resize(width, height).is_err() {
            return;
        }
        let Ok(mut buffer) = surface.buffer_mut() else {
            return;
        };
        let frame = self.emulator.compose();
        let scale = (size.width as usize / FRAMEBUFFER_WIDTH).min(size.height as usize / FRAMEBUFFER_HEIGHT).max(1);
        let offset_x = (size.width as usize).saturating_sub(FRAMEBUFFER_WIDTH * scale) / 2;
        let offset_y = (size.height as usize).saturating_sub(FRAMEBUFFER_HEIGHT * scale) / 2;
        buffer.fill(0);
        for (y, row) in frame.iter().enumerate() {
            for (x, [r, g, b]) in row.iter().enumerate() {
                let color = (*r as u32) << 16 | (*g as u32) << 8 | *b as u32;
                for dy in 0..scale {
                    let target_y = offset_y + y * scale + dy;
                    if target_y >= size.height as usize {
                        break;
                    }
                    let start = target_y * size.width as usize + offset_x + x * scale;
                    let end = (start + scale).min((target_y + 1) * size.width as usize);
                    buffer[start..end].fill(color);
                }
            }
        }
        let _ = buffer.present();
    }

    fn run_pending_frames(&mut self) {
        let started = Instant::now();
        let mut ran = false;
        match (self.emulator.speed(), self.emulator.audio.is_some()) {
            (None, _) => {
                while started.elapsed() < WORK_SLICE {
                    self.emulator.run_frame();
                    ran = true;
                }
            }
            (Some(_), true) => {
                while self.emulator.needs_audio() && started.elapsed() < WORK_SLICE {
                    self.emulator.run_frame();
                    ran = true;
                }
            }
            (Some(multiplier), false) => {
                let interval = frame_duration().div_f64(multiplier);
                while self.next_frame <= started && started.elapsed() < WORK_SLICE {
                    self.emulator.run_frame();
                    self.next_frame += interval;
                    ran = true;
                }
                if self.next_frame < started {
                    self.next_frame = started;
                }
            }
        }
        if ran {
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }

    fn quit(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.emulator.flush_save(true);
        self.emulator.save_settings();
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title(format!("gbae - {}", self.emulator.title))
            .with_surface_size(LogicalSize::new(FRAMEBUFFER_WIDTH as f64 * 3.0, FRAMEBUFFER_HEIGHT as f64 * 3.0));
        let window: Arc<dyn Window> = event_loop.create_window(attributes).expect("Failed to create window").into();
        let surface = softbuffer::Surface::new(&self.context, window.clone()).expect("Failed to create surface");
        self.window = Some(window);
        self.surface = Some(surface);
        self.next_frame = Instant::now();
    }

    fn proxy_wake_up(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(event_loop),
            WindowEvent::RedrawRequested => self.present(),
            WindowEvent::SurfaceResized(_) => {
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    physical_key: PhysicalKey::Code(code),
                    state,
                    repeat: false,
                    ..
                },
                ..
            } => {
                let action = self.emulator.key_event(code, state == ElementState::Pressed);
                match action {
                    Action::Close => self.emulator.menu.toggle(),
                    Action::Reset => {
                        self.emulator.reset();
                        self.emulator.menu.toggle();
                    }
                    Action::SaveState => {
                        self.emulator.save_state();
                        self.emulator.menu.toggle();
                    }
                    Action::LoadState => {
                        self.emulator.load_state();
                        self.emulator.menu.toggle();
                    }
                    Action::LoadRom(path) => match self.emulator.load_rom(&path) {
                        Ok(()) => {
                            if let Some(window) = &self.window {
                                window.set_title(&format!("gbae - {}", self.emulator.title));
                            }
                            self.emulator.menu.toggle();
                        }
                        Err(error) => eprintln!("gbae: cannot read {}: {}", path.display(), error),
                    },
                    Action::Quit => self.quit(event_loop),
                    Action::None | Action::SettingsChanged => {}
                }
                self.next_frame = Instant::now();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.emulator.menu.open {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        self.run_pending_frames();
        event_loop.set_control_flow(match (self.emulator.speed(), self.emulator.audio.is_some()) {
            (None, _) => ControlFlow::Poll,
            (Some(_), true) => ControlFlow::Wait,
            (Some(_), false) => ControlFlow::WaitUntil(self.next_frame),
        });
    }
}

fn main() -> ExitCode {
    let rom_path = match parse_arguments(std::env::args().skip(1)) {
        Ok(Arguments::Run(rom_path)) => rom_path,
        Ok(Arguments::Help) => {
            println!("{}", USAGE);
            return ExitCode::SUCCESS;
        }
        Ok(Arguments::Version) => {
            println!("gbae {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("gbae: {}\n{}", message, USAGE);
            return ExitCode::from(2);
        }
    };
    let config_path = config::config_path();
    let settings = config_path.as_deref().map(Settings::load).unwrap_or_default();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let context = softbuffer::Context::new(event_loop.owned_display_handle()).expect("Failed to create graphics context");
    let proxy = event_loop.create_proxy();
    let audio = Audio::new(settings.volume, move || proxy.wake_up());
    if audio.is_none() {
        eprintln!("gbae: no audio output available, running silently");
    }
    let mut emulator = Emulator::new(settings, config_path, audio);
    match rom_path {
        Some(rom_path) => {
            if let Err(error) = emulator.load_rom(&rom_path) {
                eprintln!("gbae: cannot read {}: {}", rom_path.display(), error);
                return ExitCode::FAILURE;
            }
        }
        None => emulator.menu.browse(&std::env::current_dir().unwrap_or_default()),
    }

    let app = App {
        context,
        window: None,
        surface: None,
        emulator,
        next_frame: Instant::now(),
    };
    event_loop.run_app(app).expect("Event loop failed");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Result<Arguments, String> {
        parse_arguments(arguments.iter().map(|argument| argument.to_string()))
    }

    #[test]
    fn test_arguments() {
        assert_eq!(parse(&[]), Ok(Arguments::Run(None)));
        assert_eq!(parse(&["game.gba"]), Ok(Arguments::Run(Some(PathBuf::from("game.gba")))));
        assert_eq!(parse(&["--help"]), Ok(Arguments::Help));
        assert_eq!(parse(&["-V"]), Ok(Arguments::Version));
        assert_eq!(parse(&["--", "-odd.gba"]), Ok(Arguments::Run(Some(PathBuf::from("-odd.gba")))));
        assert_eq!(parse(&["--fast"]), Err("unknown option --fast".to_string()));
        assert_eq!(parse(&["a.gba", "b.gba"]), Err("more than one ROM given".to_string()));
    }
}
