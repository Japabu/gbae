use pixels::{Pixels, SurfaceTexture};
use std::sync::{Arc, RwLock};
use winit::{
    application::ApplicationHandler,
    dpi::Size,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowButtons, WindowId},
};

use super::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};

pub struct Display {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    framebuffer: Arc<RwLock<Framebuffer>>,
}

#[derive(Debug)]
pub enum DisplayEvent {
    RedrawRequested,
}

impl Display {
    pub fn new(framebuffer: Arc<RwLock<Framebuffer>>) -> (Self, EventLoop<DisplayEvent>) {
        let event_loop = EventLoop::<DisplayEvent>::with_user_event().build().expect("Failed to create event loop");
        event_loop.set_control_flow(ControlFlow::Poll);

        (
            Self {
                window: None,
                pixels: None,
                framebuffer,
            },
            event_loop,
        )
    }
}

impl ApplicationHandler<DisplayEvent> for Display {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = WindowAttributes::default()
            .with_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE)
            .with_title("GBA Display".to_string())
            .with_inner_size(Size::Physical((FRAMEBUFFER_WIDTH as u32 * 4, FRAMEBUFFER_HEIGHT as u32 * 4).into()));

        #[cfg(target_arch = "wasm32")]
        let attributes = winit::platform::web::WindowAttributesExtWebSys::with_append(attributes, true);

        let window = Arc::new(event_loop.create_window(attributes).expect("Failed to create window"));

        let window_size = window.inner_size();
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, window.clone());
        let pixels = Pixels::new(FRAMEBUFFER_WIDTH as u32, FRAMEBUFFER_HEIGHT as u32, surface_texture).expect("Failed to create pixels buffer");

        self.window = Some(window.clone());
        self.pixels = Some(pixels);
        window.request_redraw();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: DisplayEvent) {
        match event {
            DisplayEvent::RedrawRequested => {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => {
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let pixels = self.pixels.as_mut().unwrap();
                let window = self.window.as_ref().unwrap();

                let (width, height) = {
                    let size = window.inner_size();
                    (size.width, size.height)
                };

                pixels.resize_surface(width, height).expect("Failed to resize surface");

                let framebuffer = &self.framebuffer.read().unwrap();
                let frame = pixels.frame_mut();

                for (i, pixel) in frame.chunks_exact_mut(4).enumerate() {
                    let x = (i % FRAMEBUFFER_WIDTH) as usize;
                    let y = (i / FRAMEBUFFER_WIDTH) as usize;
                    pixel[0] = framebuffer[y][x][0]; // R
                    pixel[1] = framebuffer[y][x][1]; // G
                    pixel[2] = framebuffer[y][x][2]; // B
                    pixel[3] = 255; // A
                }

                pixels.render().expect("Failed to render frame");
            }
            _ => (),
        }
    }
}
