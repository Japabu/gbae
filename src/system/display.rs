use softbuffer::{Context, Surface};
use std::{
    num::NonZeroU32,
    rc::Rc,
    sync::{Arc, RwLock},
};
use winit::{
    application::ApplicationHandler,
    dpi::Size,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowButtons, WindowId},
};

use super::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};

pub struct Display {
    window: Option<Rc<Window>>,
    context: Option<Context<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
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
                context: None,
                surface: None,
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

        let window = Rc::new(event_loop.create_window(attributes).expect("Failed to create window"));
        let context = Context::new(window.clone()).expect("Failed to create context");
        let surface = Surface::new(&context, window.clone()).expect("Failed to create surface");

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);
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
                        let fy = y as usize * FRAMEBUFFER_HEIGHT / height as usize;
                        let fx = x as usize * FRAMEBUFFER_WIDTH / width as usize;
                        let red = framebuffer[fy][fx][0] as u32;
                        let green = framebuffer[fy][fx][1] as u32;
                        let blue = framebuffer[fy][fx][2] as u32;
                        buffer[(y * width + x) as usize] = blue | (green << 8) | (red << 16);
                    }
                }

                buffer.present().unwrap();
            }
            _ => (),
        }
    }
}
