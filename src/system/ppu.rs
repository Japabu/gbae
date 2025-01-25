use std::{
    num::NonZeroU32,
    rc::Rc,
    sync::{Arc, RwLock},
};

use softbuffer::{Context, Surface};
use winit::{
    application::ApplicationHandler,
    dpi::Size,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowAttributes, WindowButtons, WindowId},
};

const BUFFER_WIDTH: usize = 240;
const BUFFER_HEIGHT: usize = 160;

type Framebuffer = [[[u8; 3]; BUFFER_WIDTH]; BUFFER_HEIGHT];

pub struct PPU {
    // framebuffer[row][col][color]
    framebuffer: Arc<RwLock<Framebuffer>>,
    event_loop_proxy: EventLoopProxy<()>,
}

impl PPU {
    pub fn new() -> PPU {
        let mut framebuffer = [[[0; 3]; BUFFER_WIDTH]; BUFFER_HEIGHT];
        for y in 0..BUFFER_HEIGHT {
            for x in 0..BUFFER_WIDTH {
                framebuffer[y][x][0] = (f32::cos(y as f32 / BUFFER_HEIGHT as f32 * std::f32::consts::PI * 2f32) * 120f32 + 120f32) as u8;
                framebuffer[y][x][1] = (f32::cos(x as f32 / BUFFER_WIDTH as f32 * std::f32::consts::PI * 2f32) * 120f32 + 120f32) as u8;
                framebuffer[y][x][2] = (f32::cos(y as f32 / BUFFER_HEIGHT as f32 * std::f32::consts::PI * 3f32) * 120f32 + 120f32) as u8 / 2
                    + (f32::cos(x as f32 / BUFFER_WIDTH as f32 * std::f32::consts::PI * 3f32) * 120f32 + 120f32) as u8 / 2;

                if x < 10 && y < 10 {
                    framebuffer[y][x] = [255, 0, 0];
                } else if x >= BUFFER_WIDTH - 10 && y < 10 {
                    framebuffer[y][x] = [0, 255, 0];
                } else if x < 10 && y >= BUFFER_HEIGHT - 10 {
                    framebuffer[y][x] = [0, 0, 255];
                }
            }
        }

        let event_loop = EventLoop::new().expect("Failed to create event loop");
        event_loop.set_control_flow(ControlFlow::Poll);

        let ppu = PPU {
            framebuffer: Arc::new(RwLock::new(framebuffer)),
            event_loop_proxy: event_loop.create_proxy(),
        };

        let mut app = App {
            window: None,
            context: None,
            surface: None,
            framebuffer: Arc::new(RwLock::new(framebuffer.clone())),
        };

        #[cfg(not(any(target_arch = "wasm32", target_arch = "wasm64")))]
        event_loop.run_app(&mut app).unwrap();

        #[cfg(any(target_arch = "wasm32", target_arch = "wasm64"))]
        winit::platform::web::EventLoopExtWebSys::spawn_app(event_loop, app);

        ppu
    }
}

struct App {
    window: Option<Rc<Window>>,
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    framebuffer: Arc<RwLock<Framebuffer>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = WindowAttributes::default()
            .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
            .with_title("PPU".to_string())
            .with_inner_size(Size::Physical((BUFFER_WIDTH as u32 * 4, BUFFER_HEIGHT as u32 * 4).into()));

        #[cfg(target_arch = "wasm32")]
        let attributes = winit::platform::web::WindowAttributesExtWebSys::with_append(attributes, true);

        let window = Rc::new(event_loop.create_window(attributes).expect("Failed to create window"));
        let context = Context::new(window.clone()).expect("Failed to create context");
        let surface = Surface::new(&context, window.clone()).expect("Failed to create surface");

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let surface = self.surface.as_mut().unwrap();
                let window = self.window.as_ref().unwrap();

                let (width, height) = {
                    let size = window.inner_size();
                    (size.width, size.height)
                };
                surface.resize(NonZeroU32::new(width).unwrap(), NonZeroU32::new(height).unwrap()).unwrap();

                let mut buffer = surface.buffer_mut().unwrap();
                let framebuffer = &self.framebuffer.read().unwrap();
                for y in 0..height {
                    for x in 0..width {
                        let fy = y as usize * BUFFER_HEIGHT / height as usize;
                        let fx = x as usize * BUFFER_WIDTH / width as usize;
                        let red = framebuffer[fy][fx][0] as u32;
                        let green = framebuffer[fy][fx][1] as u32;
                        let blue = framebuffer[fy][fx][2] as u32;
                        buffer[(y * width + x) as usize] = blue | (green << 8) | (red << 16);
                    }
                }

                buffer.present().unwrap();

                self.window.as_ref().unwrap().request_redraw();
            }
            _ => (),
        }
    }
}
