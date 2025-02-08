use std::sync::{Arc, RwLock};

use super::memory::Memory;

const BUFFER_WIDTH: usize = 240;
const BUFFER_HEIGHT: usize = 160;

type Framebuffer = [[[u8; 3]; BUFFER_WIDTH]; BUFFER_HEIGHT];

pub struct PPU {
    framebuffer: Arc<RwLock<Framebuffer>>,
    frame_counter: u64,
}

impl PPU {
    pub fn new() -> (PPU, Arc<RwLock<Framebuffer>>) {
        let mut framebuffer = [[[0; 3]; BUFFER_WIDTH]; BUFFER_HEIGHT];

        // Initialize test pattern
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

        let framebuffer = Arc::new(RwLock::new(framebuffer));

        (
            PPU {
                framebuffer: framebuffer.clone(),
                frame_counter: 0,
            },
            framebuffer,
        )
    }

    pub fn get_frame_counter(&self) -> u64 {
        self.frame_counter
    }

    pub fn draw_frame(&mut self, _mem: &mut Memory) {
        self.frame_counter += 1;
        // Get write access to framebuffer
        if let Ok(mut fb) = self.framebuffer.write() {
            // Create a simple animation by shifting colors over time
            let t = (self.frame_counter as f32) / 30.0;

            for y in 0..BUFFER_HEIGHT {
                for x in 0..BUFFER_WIDTH {
                    fb[y][x][0] = (f32::cos(y as f32 / BUFFER_HEIGHT as f32 * std::f32::consts::PI * 2.0 + t) * 120.0 + 120.0) as u8;
                    fb[y][x][1] = (f32::cos(x as f32 / BUFFER_WIDTH as f32 * std::f32::consts::PI * 2.0 + t) * 120.0 + 120.0) as u8;
                    fb[y][x][2] = (f32::cos(y as f32 / BUFFER_HEIGHT as f32 * std::f32::consts::PI * 3.0 + t) * 120.0 + 120.0) as u8 / 2
                        + (f32::cos(x as f32 / BUFFER_WIDTH as f32 * std::f32::consts::PI * 3.0 + t) * 120.0 + 120.0) as u8 / 2;

                    // Keep corner markers
                    if x < 10 && y < 10 {
                        fb[y][x] = [255, 0, 0];
                    } else if x >= BUFFER_WIDTH - 10 && y < 10 {
                        fb[y][x] = [0, 255, 0];
                    } else if x < 10 && y >= BUFFER_HEIGHT - 10 {
                        fb[y][x] = [0, 0, 255];
                    }
                }
            }
        }
    }
}
