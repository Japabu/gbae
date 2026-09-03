mod audio;
mod config;
mod font;
mod menu;

use audio::Audio;
use config::{Settings, CONFIG_FILE};
use gbae::cartridge::CartridgeInfo;
use gbae::system::apu::SAMPLE_RATE;
use gbae::system::cpu::CPU_FREQUENCY;
use gbae::system::gba::{Gba, CYCLES_PER_SCANLINE, SCANLINES_PER_FRAME};
use gbae::system::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use menu::{Action, Menu};
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

const SAVE_FLUSH_INTERVAL_FRAMES: u32 = 60;
const TURBO_SLICE: Duration = Duration::from_millis(8);

fn frame_duration() -> Duration {
    Duration::from_secs_f64(CYCLES_PER_SCANLINE as f64 * SCANLINES_PER_FRAME as f64 / CPU_FREQUENCY as f64)
}

struct Emulator {
    gba: Gba,
    bios: Vec<u8>,
    rom: Vec<u8>,
    save_path: PathBuf,
    state_path: PathBuf,
    title: String,
    audio: Option<Audio>,
    settings: Settings,
    menu: Menu,
    frames_since_save_check: u32,
}

impl Emulator {
    fn new(bios: Vec<u8>, rom_path: Option<&Path>, settings: Settings) -> Emulator {
        let audio = Audio::new(settings.volume);
        if audio.is_none() {
            eprintln!("No audio output available, running silently");
        }
        let mut emulator = Emulator {
            gba: Gba::new(bios.clone(), Vec::new()),
            bios,
            rom: Vec::new(),
            save_path: PathBuf::new(),
            state_path: PathBuf::new(),
            title: "no ROM".to_string(),
            audio,
            settings,
            menu: Menu::new(),
            frames_since_save_check: 0,
        };
        emulator.configure_audio();
        match rom_path {
            Some(rom_path) => emulator.load_rom(rom_path),
            None => emulator.menu.browse(&std::env::current_dir().unwrap_or_default()),
        }
        emulator
    }

    fn configure_audio(&mut self) {
        let sample_rate = self.audio.as_ref().map_or(SAMPLE_RATE, Audio::sample_rate);
        self.gba.set_audio_sample_rate(sample_rate);
        self.gba.set_smooth_audio(self.settings.smooth_audio);
    }

    fn rom_directory(&self) -> PathBuf {
        self.save_path
            .parent()
            .map(Path::to_path_buf)
            .filter(|directory| !directory.as_os_str().is_empty())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
    }

    fn load_rom(&mut self, rom_path: &Path) {
        let rom = match std::fs::read(rom_path) {
            Ok(rom) => rom,
            Err(error) => {
                eprintln!("Could not read {}: {}", rom_path.display(), error);
                return;
            }
        };
        self.flush_save(true);
        self.title = CartridgeInfo::parse(&rom).map(|cartridge| cartridge.title.trim().to_string()).unwrap_or_default();
        let rom_path = rom_path.canonicalize().unwrap_or_else(|_| rom_path.to_path_buf());
        self.save_path = rom_path.with_extension("sav");
        self.state_path = rom_path.with_extension("state");
        self.rom = rom;
        self.reset();
        self.menu.set_directory(&self.rom_directory());
        eprintln!("Loaded {} ({}), save type {:?}", rom_path.display(), self.title, self.gba.save_type());
    }

    fn reset(&mut self) {
        self.flush_save(true);
        self.gba = Gba::new(self.bios.clone(), self.rom.clone());
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
        if let Some(audio) = &self.audio {
            if !self.settings.turbo {
                audio.push(&samples);
            }
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
                eprintln!("Could not write {}: {}", self.save_path.display(), error);
            }
        }
    }

    fn save_state(&self) {
        match std::fs::write(&self.state_path, self.gba.save_state()) {
            Ok(()) => eprintln!("State saved to {}", self.state_path.display()),
            Err(error) => eprintln!("Could not write {}: {}", self.state_path.display(), error),
        }
    }

    fn load_state(&mut self) {
        let bytes = match std::fs::read(&self.state_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("Could not read {}: {}", self.state_path.display(), error);
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
                eprintln!("Could not load state: {}", error);
                self.reset();
            }
        }
    }

    fn key_event(&mut self, code: KeyCode, pressed: bool) -> Action {
        if self.menu.open {
            if pressed {
                let action = self.menu.key(code, &mut self.settings);
                if action == Action::SettingsChanged {
                    if let Some(audio) = &self.audio {
                        audio.set_volume(self.settings.volume);
                    }
                    self.gba.set_smooth_audio(self.settings.smooth_audio);
                    self.settings.save(Path::new(CONFIG_FILE));
                }
                return action;
            }
        } else if code == KeyCode::Escape {
            if pressed {
                self.menu.toggle();
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
        }
        frame
    }
}

struct App {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    emulator: Emulator,
    next_frame: Instant,
}

impl App {
    fn present(&mut self) {
        let (Some(window), Some(surface)) = (&self.window, &mut self.surface) else {
            return;
        };
        let size = window.inner_size();
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
        let now = Instant::now();
        if self.emulator.settings.turbo {
            let deadline = now + TURBO_SLICE;
            while Instant::now() < deadline {
                self.emulator.run_frame();
            }
            self.next_frame = Instant::now();
        } else if now >= self.next_frame {
            self.emulator.run_frame();
            self.next_frame += frame_duration();
            if self.next_frame + frame_duration() * 4 < now {
                self.next_frame = now;
            }
        } else {
            return;
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn quit(&mut self, event_loop: &ActiveEventLoop) {
        self.emulator.flush_save(true);
        self.emulator.settings.save(Path::new(CONFIG_FILE));
        event_loop.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = WindowAttributes::default()
            .with_title(format!("gbae - {}", self.emulator.title))
            .with_inner_size(LogicalSize::new(FRAMEBUFFER_WIDTH as f64 * 3.0, FRAMEBUFFER_HEIGHT as f64 * 3.0));
        let window = Arc::new(event_loop.create_window(attributes).expect("Failed to create window"));
        let context = softbuffer::Context::new(window.clone()).expect("Failed to create graphics context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("Failed to create surface");
        self.window = Some(window);
        self.surface = Some(surface);
        self.next_frame = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.quit(event_loop),
            WindowEvent::RedrawRequested => self.present(),
            WindowEvent::Resized(_) => {
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
                    Action::LoadRom(path) => {
                        self.emulator.load_rom(&path);
                        if let Some(window) = &self.window {
                            window.set_title(&format!("gbae - {}", self.emulator.title));
                        }
                        self.emulator.menu.toggle();
                    }
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.emulator.menu.open {
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            self.run_pending_frames();
            event_loop.set_control_flow(if self.emulator.settings.turbo {
                ControlFlow::Poll
            } else {
                ControlFlow::WaitUntil(self.next_frame)
            });
        }
    }
}

fn main() {
    let bios_path = std::env::var("GBA_BIOS").unwrap_or_else(|_| "gba_bios.bin".to_string());
    let bios = std::fs::read(&bios_path).unwrap_or_else(|error| {
        eprintln!("Could not read BIOS {}: {}", bios_path, error);
        std::process::exit(1);
    });
    let rom_path = std::env::args().nth(1).map(PathBuf::from).or_else(|| Path::new("rom.gba").exists().then(|| PathBuf::from("rom.gba")));
    let settings = Settings::load(Path::new(CONFIG_FILE));
    let emulator = Emulator::new(bios, rom_path.as_deref(), settings);

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App {
        window: None,
        surface: None,
        emulator,
        next_frame: Instant::now(),
    };
    event_loop.run_app(&mut app).expect("Event loop failed");
}
