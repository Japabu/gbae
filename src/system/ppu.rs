use std::sync::{Arc, RwLock};

use super::memory::Memory;

pub const FRAMEBUFFER_WIDTH: usize = 240;
pub const FRAMEBUFFER_HEIGHT: usize = 160;
const VRAM_START: u32 = 0x0600_0000;

pub type Framebuffer = [[[u8; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT];

pub struct PPU {
    framebuffer: Arc<RwLock<Framebuffer>>,
}

impl PPU {
    pub fn new() -> (PPU, Arc<RwLock<Framebuffer>>) {
        // Initialize framebuffer to black
        let framebuffer = [[[0; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT];
        let framebuffer = Arc::new(RwLock::new(framebuffer));

        (PPU { framebuffer: framebuffer.clone() }, framebuffer)
    }

    pub fn draw_scanline(&mut self, mem: &mut Memory) {
        if let Ok(mut fb) = self.framebuffer.write() {
            let io = mem.get_io_registers();
            let disp_cnt = io.disp_cnt;
            let y = io.v_count as usize;

            // Check forced blank bit (bit 7)
            // TEMP: Comment out to see BIOS logo data in VRAM
            // if (disp_cnt & 0x80) != 0 {
            //     // Forced blank - draw white
            //     for x in 0..FRAMEBUFFER_WIDTH {
            //         fb[y][x] = [255, 255, 255];
            //     }
            //     return;
            // }

            let mode = disp_cnt & 0x7;
            let bg0_enabled = (disp_cnt & (1 << 8)) != 0;
            let bg1_enabled = (disp_cnt & (1 << 9)) != 0;
            let bg2_enabled = (disp_cnt & (1 << 10)) != 0;
            let bg3_enabled = (disp_cnt & (1 << 11)) != 0;

            // Mode 0: Tile mode with BG0-3
            if mode == 0 {
                // Render highest priority enabled background (BIOS uses BG0)
                if bg0_enabled {
                    self.draw_mode0_bg_scanline(y, &mut fb, mem, 0);
                } else if bg1_enabled {
                    self.draw_mode0_bg_scanline(y, &mut fb, mem, 1);
                } else if bg2_enabled {
                    self.draw_mode0_bg_scanline(y, &mut fb, mem, 2);
                } else if bg3_enabled {
                    self.draw_mode0_bg_scanline(y, &mut fb, mem, 3);
                } else {
                    // No backgrounds enabled, draw black
                    for x in 0..FRAMEBUFFER_WIDTH {
                        fb[y][x] = [0, 0, 0];
                    }
                }
            } else {
                // Fall back to old direct VRAM reading for other modes
                for x in 0..FRAMEBUFFER_WIDTH {
                    let addr = VRAM_START + (y * FRAMEBUFFER_WIDTH + x) as u32 * 2;
                    let color16 = mem.read_u16(addr);
                    let r = ((color16 >> 0) & 0x1F) as u8;
                    let g = ((color16 >> 5) & 0x1F) as u8;
                    let b = ((color16 >> 10) & 0x1F) as u8;
                    fb[y][x][0] = r << 3;
                    fb[y][x][1] = g << 3;
                    fb[y][x][2] = b << 3;
                }
            }
        }
    }

    fn draw_mode0_bg_scanline(&self, y: usize, fb: &mut Framebuffer, mem: &mut Memory, bg_num: u8) {

        // Get BG control register for the specified background
        let bg_cnt = match bg_num {
            0 => mem.get_io_registers().bg0_cnt,
            1 => mem.get_io_registers().bg1_cnt,
            2 => mem.get_io_registers().bg2_cnt,
            3 => mem.get_io_registers().bg3_cnt,
            _ => return, // Invalid BG number
        };

        // Extract character base block (bits 2-3) and screen base block (bits 8-12)
        let char_base = ((bg_cnt >> 2) & 0x3) * 0x4000;  // Character base in VRAM
        let screen_base = ((bg_cnt >> 8) & 0x1F) * 0x800;  // Screen base in VRAM
        let _screen_size = (bg_cnt >> 14) & 0x3;  // 0=256x256, 1=512x256, 2=256x512, 3=512x512

        // For simplicity, assume 256x256 screen (32x32 tiles)
        let tiles_per_row = 32;

        for x in 0..FRAMEBUFFER_WIDTH {
            // Which tile are we in?
            let tile_x = x / 8;
            let tile_y = y / 8;
            let pixel_x = x % 8;
            let pixel_y = y % 8;

            // Get screen entry (tilemap entry)
            let screen_entry_addr = VRAM_START + screen_base as u32 + ((tile_y * tiles_per_row + tile_x) * 2) as u32;
            let screen_entry = mem.read_u16(screen_entry_addr);

            // Extract tile number and palette (assuming 4bpp for now)
            let tile_num = screen_entry & 0x3FF;  // bits 0-9
            let h_flip = (screen_entry & (1 << 10)) != 0;
            let v_flip = (screen_entry & (1 << 11)) != 0;
            let palette_num = (screen_entry >> 12) & 0xF;  // bits 12-15

            // Apply flips
            let actual_pixel_x = if h_flip { 7 - pixel_x } else { pixel_x };
            let actual_pixel_y = if v_flip { 7 - pixel_y } else { pixel_y };

            // Get tile data (4bpp: 32 bytes per tile, 4 bits per pixel)
            let tile_addr = VRAM_START + char_base as u32 + (tile_num * 32) as u32;
            let pixel_addr = tile_addr + (actual_pixel_y * 4 + actual_pixel_x / 2) as u32;
            let pixel_byte = mem.read_u8(pixel_addr);

            // Extract 4-bit palette index
            let palette_index = if actual_pixel_x % 2 == 0 {
                pixel_byte & 0xF
            } else {
                pixel_byte >> 4
            };

            // Read color from palette (BG palette starts at 0x05000000)
            let palette_addr = 0x05000000 + (palette_num * 16 + palette_index as u16) as u32 * 2;
            let color16 = mem.read_u16(palette_addr);

            // Convert to RGB
            let r = ((color16 >> 0) & 0x1F) as u8;
            let g = ((color16 >> 5) & 0x1F) as u8;
            let b = ((color16 >> 10) & 0x1F) as u8;

            fb[y][x][0] = r << 3;
            fb[y][x][1] = g << 3;
            fb[y][x][2] = b << 3;
        }
    }
}
