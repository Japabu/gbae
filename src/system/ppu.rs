use crate::bits::Bits;

use super::{
    memory::{IoRegisters, Memory},
    state::{Reader, StateError, Writer},
};

pub const FRAMEBUFFER_WIDTH: usize = 240;
pub const FRAMEBUFFER_HEIGHT: usize = 160;

const TRANSPARENT: u16 = 0x8000;
const BG_TILE_LIMIT: usize = 0x1_0000;
const OBJ_TILE_BASE: usize = 0x1_0000;
const OBJ_TILE_MASK: usize = 0x7FFF;
const OBJ_PALETTE_BASE: usize = 0x200;
const NO_OBJ_PRIORITY: u8 = 4;
const OBJ_SIZES: [[(i32, i32); 4]; 3] = [[(8, 8), (16, 16), (32, 32), (64, 64)], [(16, 8), (32, 8), (32, 16), (64, 32)], [(8, 16), (8, 32), (16, 32), (32, 64)]];

const DISPCNT_FRAME_SELECT: u16 = 1 << 4;
const DISPCNT_HBLANK_INTERVAL_FREE: u16 = 1 << 5;
const DISPCNT_OBJ_ONE_DIMENSIONAL: u16 = 1 << 6;
const DISPCNT_FORCED_BLANK: u16 = 1 << 7;
const DISPCNT_OBJ: u16 = 1 << 12;
const DISPCNT_WIN0: u16 = 1 << 13;
const DISPCNT_WIN1: u16 = 1 << 14;
const DISPCNT_OBJ_WINDOW: u16 = 1 << 15;

const BGCNT_MOSAIC: u16 = 1 << 6;
const BGCNT_EIGHT_BIT: u16 = 1 << 7;
const BGCNT_WRAPAROUND: u16 = 1 << 13;

const WINDOW_OBJ: u16 = 1 << 4;
const WINDOW_EFFECTS: u16 = 1 << 5;
const WINDOW_EVERYTHING: u16 = 0x3F;

const BLEND_ALPHA: u16 = 1;
const BLEND_BRIGHTEN: u16 = 2;
const BLEND_DARKEN: u16 = 3;

const OBJ_MODE_SEMI_TRANSPARENT: u16 = 1;
const OBJ_MODE_WINDOW: u16 = 2;
const OBJ_MODE_PROHIBITED: u16 = 3;
const OBJ_CYCLES_PER_LINE: i32 = 1210;
const OBJ_CYCLES_PER_LINE_HBLANK_FREE: i32 = 954;

pub type Framebuffer = [[[u8; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT];

#[derive(Debug, Clone, Copy)]
struct ObjPixel {
    color: u16,
    priority: u8,
    semi_transparent: bool,
}

const NO_OBJ_PIXEL: ObjPixel = ObjPixel {
    color: TRANSPARENT,
    priority: NO_OBJ_PRIORITY,
    semi_transparent: false,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Background(usize),
    Object(u8),
    Backdrop,
}

impl Layer {
    fn blend_bit(self) -> u16 {
        match self {
            Layer::Background(bg) => 1 << bg,
            Layer::Object(_) => 1 << 4,
            Layer::Backdrop => 1 << 5,
        }
    }

    fn window_bit(self) -> u16 {
        match self {
            Layer::Background(bg) => 1 << bg,
            Layer::Object(_) => WINDOW_OBJ,
            Layer::Backdrop => WINDOW_EVERYTHING,
        }
    }
}

pub struct PPU {
    framebuffer: Framebuffer,
    affine_reference: [[i32; 2]; 2],
    affine_mosaic_reference: [[i32; 2]; 2],
    bg_lines: [[u16; FRAMEBUFFER_WIDTH]; 4],
    obj_line: [ObjPixel; FRAMEBUFFER_WIDTH],
    obj_window: [bool; FRAMEBUFFER_WIDTH],
    layer_order: [Layer; 8],
    layer_count: usize,
}

impl PPU {
    pub fn new() -> PPU {
        PPU {
            framebuffer: [[[0; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT],
            affine_reference: [[0; 2]; 2],
            affine_mosaic_reference: [[0; 2]; 2],
            bg_lines: [[TRANSPARENT; FRAMEBUFFER_WIDTH]; 4],
            obj_line: [NO_OBJ_PIXEL; FRAMEBUFFER_WIDTH],
            obj_window: [false; FRAMEBUFFER_WIDTH],
            layer_order: [Layer::Backdrop; 8],
            layer_count: 0,
        }
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn save_state(&self, writer: &mut Writer) {
        for row in self.framebuffer.iter() {
            for pixel in row {
                writer.bytes(pixel);
            }
        }
        for reference in &self.affine_reference {
            writer.i32s(reference);
        }
        for reference in &self.affine_mosaic_reference {
            writer.i32s(reference);
        }
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        for row in self.framebuffer.iter_mut() {
            for pixel in row.iter_mut() {
                reader.bytes_into(pixel)?;
            }
        }
        for reference in &mut self.affine_reference {
            reader.i32s(reference)?;
        }
        for reference in &mut self.affine_mosaic_reference {
            reader.i32s(reference)?;
        }
        Ok(())
    }

    pub fn latch_affine_references(&mut self, io: &mut IoRegisters) {
        for bg in 0..2 {
            if io.v_count == 0 || io.bg_reference_written[bg] {
                self.affine_reference[bg] = [(io.bg_reference[bg][0]).sign_extended(28) as i32, (io.bg_reference[bg][1]).sign_extended(28) as i32];
                io.bg_reference_written[bg] = false;
            }
        }
    }

    pub fn draw_scanline(&mut self, mem: &Memory) {
        let io = mem.io();
        let y = io.v_count as usize;

        let (_, mosaic_y) = mosaic_size(true, io.mosaic);
        if y % mosaic_y == 0 {
            self.affine_mosaic_reference = self.affine_reference;
        }

        if io.disp_cnt & DISPCNT_FORCED_BLANK != 0 {
            self.framebuffer[y] = [[255; 3]; FRAMEBUFFER_WIDTH];
        } else {
            self.render_backgrounds(y, io, mem.vram(), mem.palette_ram());
            self.render_objects(y, io, mem.vram(), mem.palette_ram(), mem.oam());
            self.sort_layers(io);
            self.compose(y, io, mem.palette_ram());
            if io.green_swap & 1 != 0 {
                for pair in self.framebuffer[y].chunks_exact_mut(2) {
                    let green = pair[0][1];
                    pair[0][1] = pair[1][1];
                    pair[1][1] = green;
                }
            }
        }

        for bg in 0..2 {
            let [_, pb, _, pd] = io.bg_parameters[bg].map(|parameter| parameter as i16 as i32);
            self.affine_reference[bg][0] = self.affine_reference[bg][0].wrapping_add(pb);
            self.affine_reference[bg][1] = self.affine_reference[bg][1].wrapping_add(pd);
        }
    }

    fn render_backgrounds(&mut self, y: usize, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        for line in self.bg_lines.iter_mut() {
            line.fill(TRANSPARENT);
        }
        let enabled = |bg: usize| io.disp_cnt & (1 << (8 + bg)) != 0;
        match io.disp_cnt & 0b111 {
            0 => {
                for bg in 0..4 {
                    if enabled(bg) {
                        self.render_text_background(bg, y, io, vram, palette);
                    }
                }
            }
            1 => {
                for bg in 0..2 {
                    if enabled(bg) {
                        self.render_text_background(bg, y, io, vram, palette);
                    }
                }
                if enabled(2) {
                    self.render_affine_background(2, io, vram, palette);
                }
            }
            2 => {
                for bg in 2..4 {
                    if enabled(bg) {
                        self.render_affine_background(bg, io, vram, palette);
                    }
                }
            }
            3..=5 => {
                if enabled(2) {
                    self.render_bitmap_background(io, vram, palette);
                }
            }
            _ => {}
        }
    }

    fn render_text_background(&mut self, bg: usize, y: usize, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        let control = io.bg_cnt[bg];
        let char_base = ((control >> 2) & 0b11) as usize * 0x4000;
        let screen_base = ((control >> 8) & 0x1F) as usize * 0x800;
        let eight_bit = control & BGCNT_EIGHT_BIT != 0;
        let size = control >> 14;
        let width_mask = if size & 0b01 != 0 { 511 } else { 255 };
        let height_mask = if size & 0b10 != 0 { 511 } else { 255 };
        let blocks_per_row = if size == 0b11 { 2 } else { 1 };
        let (mosaic_x, mosaic_y) = mosaic_size(control & BGCNT_MOSAIC != 0, io.mosaic);

        let yy = (y - y % mosaic_y + io.bg_v_offset[bg] as usize) & height_mask;
        let h_offset = io.bg_h_offset[bg] as usize;
        let line = &mut self.bg_lines[bg];
        let mut tile_column = usize::MAX;
        let mut entry = 0;

        for x in 0..FRAMEBUFFER_WIDTH {
            let xx = (x - x % mosaic_x + h_offset) & width_mask;
            if xx >> 3 != tile_column {
                tile_column = xx >> 3;
                let block = (yy >> 8) * blocks_per_row + (xx >> 8);
                let entry_offset = screen_base + block * 0x800 + (((yy & 0xFF) >> 3) * 32 + ((xx & 0xFF) >> 3)) * 2;
                entry = halfword(vram, entry_offset & 0xFFFF);
            }
            let tile = (entry & 0x3FF) as usize;
            let px = if entry & 0x400 != 0 { 7 - (xx & 7) } else { xx & 7 };
            let py = if entry & 0x800 != 0 { 7 - (yy & 7) } else { yy & 7 };
            line[x] = if eight_bit {
                bg_tile_color(vram, palette, char_base + tile * 64, px, py, true, 0)
            } else {
                bg_tile_color(vram, palette, char_base + tile * 32, px, py, false, (entry >> 12) as usize)
            };
        }
    }

    fn render_affine_background(&mut self, bg: usize, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        let control = io.bg_cnt[bg];
        let char_base = ((control >> 2) & 0b11) as usize * 0x4000;
        let screen_base = ((control >> 8) & 0x1F) as usize * 0x800;
        let size = 128 << (control >> 14);
        let wraps = control & BGCNT_WRAPAROUND != 0;
        let mosaic = control & BGCNT_MOSAIC != 0;
        let (mosaic_x, _) = mosaic_size(mosaic, io.mosaic);
        let [pa, _, pc, _] = io.bg_parameters[bg - 2].map(|parameter| parameter as i16 as i32);
        let [mut px, mut py] = if mosaic { self.affine_mosaic_reference[bg - 2] } else { self.affine_reference[bg - 2] };
        let line = &mut self.bg_lines[bg];

        for x in 0..FRAMEBUFFER_WIDTH {
            let (mut tx, mut ty) = (px >> 8, py >> 8);
            if wraps {
                tx &= size - 1;
                ty &= size - 1;
            }
            line[x] = if (0..size).contains(&tx) && (0..size).contains(&ty) {
                let (tx, ty) = (tx as usize, ty as usize);
                let tile = bg_vram_byte(vram, screen_base + (ty / 8) * (size as usize / 8) + tx / 8) as usize;
                bg_tile_color(vram, palette, char_base + tile * 64, tx & 7, ty & 7, true, 0)
            } else {
                TRANSPARENT
            };
            px = px.wrapping_add(pa);
            py = py.wrapping_add(pc);
        }
        apply_horizontal_mosaic(line, mosaic_x);
    }

    fn render_bitmap_background(&mut self, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        let mode = io.disp_cnt & 0b111;
        let (width, height) = if mode == 5 { (160, 128) } else { (240, 160) };
        let frame = if mode != 3 && io.disp_cnt & DISPCNT_FRAME_SELECT != 0 { 0xA000 } else { 0 };
        let mosaic = io.bg_cnt[2] & BGCNT_MOSAIC != 0;
        let (mosaic_x, _) = mosaic_size(mosaic, io.mosaic);
        let [pa, _, pc, _] = io.bg_parameters[0].map(|parameter| parameter as i16 as i32);
        let [mut px, mut py] = if mosaic { self.affine_mosaic_reference[0] } else { self.affine_reference[0] };
        let line = &mut self.bg_lines[2];

        for x in 0..FRAMEBUFFER_WIDTH {
            let (tx, ty) = (px >> 8, py >> 8);
            line[x] = if (0..width).contains(&tx) && (0..height).contains(&ty) {
                let (tx, ty) = (tx as usize, ty as usize);
                match mode {
                    3 => halfword(vram, (ty * 240 + tx) * 2) & 0x7FFF,
                    4 => palette_color(palette, vram[frame + ty * 240 + tx] as usize),
                    _ => halfword(vram, frame + (ty * 160 + tx) * 2) & 0x7FFF,
                }
            } else {
                TRANSPARENT
            };
            px = px.wrapping_add(pa);
            py = py.wrapping_add(pc);
        }
        apply_horizontal_mosaic(line, mosaic_x);
    }

    fn render_objects(&mut self, y: usize, io: &IoRegisters, vram: &[u8], palette: &[u8], oam: &[u8]) {
        self.obj_line.fill(NO_OBJ_PIXEL);
        self.obj_window.fill(false);
        if io.disp_cnt & DISPCNT_OBJ != 0 {
            let mut budget = if io.disp_cnt & DISPCNT_HBLANK_INTERVAL_FREE != 0 {
                OBJ_CYCLES_PER_LINE_HBLANK_FREE
            } else {
                OBJ_CYCLES_PER_LINE
            };
            for index in 0..128 {
                budget -= self.render_object(index, y, io, vram, palette, oam, budget);
                if budget <= 0 {
                    break;
                }
            }
        }
    }

    fn render_object(&mut self, index: usize, y: usize, io: &IoRegisters, vram: &[u8], palette: &[u8], oam: &[u8], budget: i32) -> i32 {
        let attribute0 = halfword(oam, index * 8);
        let attribute1 = halfword(oam, index * 8 + 2);
        let attribute2 = halfword(oam, index * 8 + 4);

        let affine = attribute0 & 0x100 != 0;
        let double_size_or_disabled = attribute0 & 0x200 != 0;
        let mode = (attribute0 >> 10) & 0b11;
        let shape = (attribute0 >> 14) as usize;
        if (!affine && double_size_or_disabled) || mode == OBJ_MODE_PROHIBITED || shape >= OBJ_SIZES.len() {
            return 0;
        }

        let (width, height) = OBJ_SIZES[shape][(attribute1 >> 14) as usize];
        let (box_width, box_height) = if affine && double_size_or_disabled { (width * 2, height * 2) } else { (width, height) };
        let mut sprite_y = (attribute0 & 0xFF) as i32;
        if sprite_y >= FRAMEBUFFER_HEIGHT as i32 {
            sprite_y -= 256;
        }
        let mut sprite_x = (attribute1 & 0x1FF) as i32;
        if sprite_x >= FRAMEBUFFER_WIDTH as i32 {
            sprite_x -= 512;
        }

        let (mosaic_x, mosaic_y) = mosaic_size(attribute0 & 0x1000 != 0, io.mosaic >> 8);
        let row = (y - y % mosaic_y) as i32 - sprite_y;
        if row < 0 || row >= box_height {
            return 0;
        }
        let cycles = if affine { box_width * 2 + 10 } else { box_width };
        if cycles > budget {
            return cycles;
        }

        let eight_bit = attribute0 & 0x2000 != 0;
        let tile_base = (attribute2 & 0x3FF) as usize;
        if io.disp_cnt & 0b111 >= 3 && tile_base < 512 {
            return cycles;
        }
        let priority = ((attribute2 >> 10) & 0b11) as u8;
        let palette_bank = (attribute2 >> 12) as usize;
        let tiles_per_unit = if eight_bit { 2 } else { 1 };
        let stride = if io.disp_cnt & DISPCNT_OBJ_ONE_DIMENSIONAL != 0 {
            (width as usize / 8) * tiles_per_unit
        } else {
            32
        };
        let [pa, pb, pc, pd] = if affine {
            let group = ((attribute1 >> 9) & 0x1F) as usize * 32;
            [6, 14, 22, 30].map(|offset| halfword(oam, group + offset) as i16 as i32)
        } else {
            [0x100, 0, 0, 0x100]
        };
        let horizontal_flip = !affine && attribute1 & 0x1000 != 0;
        let vertical_flip = !affine && attribute1 & 0x2000 != 0;

        for screen_column in 0..box_width {
            let screen_x = sprite_x + screen_column;
            if !(0..FRAMEBUFFER_WIDTH as i32).contains(&screen_x) {
                continue;
            }
            let column = screen_x - screen_x % mosaic_x as i32 - sprite_x;
            if column < 0 {
                continue;
            }
            let (tx, ty) = if affine {
                let ox = column - box_width / 2;
                let oy = row - box_height / 2;
                (((pa * ox + pb * oy) >> 8) + width / 2, ((pc * ox + pd * oy) >> 8) + height / 2)
            } else {
                (if horizontal_flip { width - 1 - column } else { column }, if vertical_flip { height - 1 - row } else { row })
            };
            if !(0..width).contains(&tx) || !(0..height).contains(&ty) {
                continue;
            }
            let (tx, ty) = (tx as usize, ty as usize);
            let tile = tile_base + (ty / 8) * stride + (tx / 8) * tiles_per_unit;
            let color_index = obj_tile_pixel(vram, tile, tx & 7, ty & 7, eight_bit);
            if color_index == 0 {
                continue;
            }

            let screen_x = screen_x as usize;
            if mode == OBJ_MODE_WINDOW {
                self.obj_window[screen_x] = true;
            } else if priority < self.obj_line[screen_x].priority {
                let palette_index = if eight_bit { color_index } else { palette_bank * 16 + color_index };
                self.obj_line[screen_x] = ObjPixel {
                    color: halfword(palette, OBJ_PALETTE_BASE + palette_index * 2) & 0x7FFF,
                    priority,
                    semi_transparent: mode == OBJ_MODE_SEMI_TRANSPARENT,
                };
            }
        }
        cycles
    }

    fn compose(&mut self, y: usize, io: &IoRegisters, palette: &[u8]) {
        let backdrop = halfword(palette, 0) & 0x7FFF;
        let windows_enabled = io.disp_cnt & (DISPCNT_WIN0 | DISPCNT_WIN1 | DISPCNT_OBJ_WINDOW) != 0;
        let blend_mode = (io.blend_cnt >> 6) & 0b11;
        let eva = (io.blend_alpha & 0x1F).min(16) as u32;
        let evb = ((io.blend_alpha >> 8) & 0x1F).min(16) as u32;
        let evy = (io.blend_y & 0x1F).min(16) as u32;

        for x in 0..FRAMEBUFFER_WIDTH {
            let window = if windows_enabled { self.window_control(x, y, io) } else { WINDOW_EVERYTHING };
            let [(top, top_color), (second, second_color)] = self.top_layers(x, window, backdrop);
            let first_target = io.blend_cnt & top.blend_bit() != 0;
            let second_target = io.blend_cnt & (second.blend_bit() << 8) != 0;
            let semi_transparent_object = matches!(top, Layer::Object(_)) && self.obj_line[x].semi_transparent;

            let color = if window & WINDOW_EFFECTS == 0 {
                top_color
            } else if semi_transparent_object && second_target {
                alpha_blend(top_color, second_color, eva, evb)
            } else {
                match blend_mode {
                    BLEND_ALPHA if first_target && second_target => alpha_blend(top_color, second_color, eva, evb),
                    BLEND_BRIGHTEN if first_target => brighten(top_color, evy),
                    BLEND_DARKEN if first_target => darken(top_color, evy),
                    _ => top_color,
                }
            };
            self.framebuffer[y][x] = rgb888(color);
        }
    }

    fn sort_layers(&mut self, io: &IoRegisters) {
        let background_active = |bg: usize| match io.disp_cnt & 0b111 {
            0 => true,
            1 => bg <= 2,
            2 => bg >= 2,
            _ => bg == 2,
        };
        self.layer_count = 0;
        for priority in 0..4 {
            if io.disp_cnt & DISPCNT_OBJ != 0 {
                self.layer_order[self.layer_count] = Layer::Object(priority as u8);
                self.layer_count += 1;
            }
            for bg in 0..4 {
                if io.disp_cnt & (1 << (8 + bg)) != 0 && background_active(bg) && io.bg_cnt[bg] & 0b11 == priority {
                    self.layer_order[self.layer_count] = Layer::Background(bg);
                    self.layer_count += 1;
                }
            }
        }
    }

    fn top_layers(&self, x: usize, window: u16, backdrop: u16) -> [(Layer, u16); 2] {
        let mut layers = [(Layer::Backdrop, backdrop); 2];
        let mut found = 0;
        let object = self.obj_line[x];
        for &layer in &self.layer_order[..self.layer_count] {
            if found == 2 {
                break;
            }
            let color = match layer {
                Layer::Object(priority) if object.priority == priority => object.color,
                Layer::Object(_) => TRANSPARENT,
                Layer::Background(bg) => self.bg_lines[bg][x],
                Layer::Backdrop => backdrop,
            };
            if color != TRANSPARENT && window & layer.window_bit() != 0 {
                layers[found] = (layer, color);
                found += 1;
            }
        }
        layers
    }

    fn window_control(&self, x: usize, y: usize, io: &IoRegisters) -> u16 {
        if io.disp_cnt & DISPCNT_WIN0 != 0 && inside_window(io.win_h[0], io.win_v[0], x, y) {
            io.win_in & WINDOW_EVERYTHING
        } else if io.disp_cnt & DISPCNT_WIN1 != 0 && inside_window(io.win_h[1], io.win_v[1], x, y) {
            (io.win_in >> 8) & WINDOW_EVERYTHING
        } else if io.disp_cnt & DISPCNT_OBJ_WINDOW != 0 && self.obj_window[x] {
            (io.win_out >> 8) & WINDOW_EVERYTHING
        } else {
            io.win_out & WINDOW_EVERYTHING
        }
    }
}

fn halfword(buffer: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buffer[offset], buffer[offset + 1]])
}

fn bg_vram_byte(vram: &[u8], offset: usize) -> u8 {
    if offset < BG_TILE_LIMIT {
        vram[offset]
    } else {
        0
    }
}

fn palette_color(palette: &[u8], index: usize) -> u16 {
    if index == 0 {
        TRANSPARENT
    } else {
        halfword(palette, index * 2) & 0x7FFF
    }
}

fn bg_tile_color(vram: &[u8], palette: &[u8], tile_offset: usize, px: usize, py: usize, eight_bit: bool, palette_bank: usize) -> u16 {
    if tile_offset >= BG_TILE_LIMIT {
        TRANSPARENT
    } else if eight_bit {
        palette_color(palette, vram[tile_offset + py * 8 + px] as usize)
    } else {
        let byte = vram[tile_offset + py * 4 + px / 2];
        let index = (if px & 1 == 0 { byte & 0xF } else { byte >> 4 }) as usize;
        if index == 0 {
            TRANSPARENT
        } else {
            palette_color(palette, palette_bank * 16 + index)
        }
    }
}

fn obj_tile_pixel(vram: &[u8], tile: usize, px: usize, py: usize, eight_bit: bool) -> usize {
    if eight_bit {
        vram[OBJ_TILE_BASE + ((tile * 32 + py * 8 + px) & OBJ_TILE_MASK)] as usize
    } else {
        let byte = vram[OBJ_TILE_BASE + ((tile * 32 + py * 4 + px / 2) & OBJ_TILE_MASK)];
        (if px & 1 == 0 { byte & 0xF } else { byte >> 4 }) as usize
    }
}

fn mosaic_size(enabled: bool, mosaic: u16) -> (usize, usize) {
    if enabled {
        ((mosaic & 0xF) as usize + 1, ((mosaic >> 4) & 0xF) as usize + 1)
    } else {
        (1, 1)
    }
}

fn apply_horizontal_mosaic(line: &mut [u16; FRAMEBUFFER_WIDTH], mosaic_x: usize) {
    if mosaic_x > 1 {
        for x in 0..FRAMEBUFFER_WIDTH {
            line[x] = line[x - x % mosaic_x];
        }
    }
}

fn inside_window(horizontal: u16, vertical: u16, x: usize, y: usize) -> bool {
    inside_range(x, horizontal, FRAMEBUFFER_WIDTH) && inside_range(y, vertical, FRAMEBUFFER_HEIGHT)
}

fn inside_range(value: usize, bounds: u16, limit: usize) -> bool {
    let start = (bounds >> 8) as usize;
    let mut end = (bounds & 0xFF) as usize;
    if end > limit || start > end {
        end = limit;
    }
    start <= value && value < end
}

fn channels(color: u16) -> [u32; 3] {
    [(color & 0x1F) as u32, ((color >> 5) & 0x1F) as u32, ((color >> 10) & 0x1F) as u32]
}

fn from_channels([r, g, b]: [u32; 3]) -> u16 {
    (r | g << 5 | b << 10) as u16
}

fn alpha_blend(first: u16, second: u16, eva: u32, evb: u32) -> u16 {
    let first = channels(first);
    let second = channels(second);
    from_channels([0, 1, 2].map(|i| ((first[i] * eva + second[i] * evb) >> 4).min(31)))
}

fn brighten(color: u16, evy: u32) -> u16 {
    from_channels(channels(color).map(|c| c + ((31 - c) * evy >> 4)))
}

fn darken(color: u16, evy: u32) -> u16 {
    from_channels(channels(color).map(|c| c - (c * evy >> 4)))
}

fn rgb888(color: u16) -> [u8; 3] {
    channels(color).map(|c| (c << 3) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_effects() {
        let red = from_channels([31, 0, 0]);
        let green = from_channels([0, 31, 0]);
        assert_eq!(channels(alpha_blend(red, green, 8, 8)), [15, 15, 0]);
        assert_eq!(channels(alpha_blend(red, red, 16, 16)), [31, 0, 0]);
        assert_eq!(channels(brighten(red, 16)), [31, 31, 31]);
        assert_eq!(channels(darken(red, 16)), [0, 0, 0]);
        assert_eq!(channels(darken(red, 8)), [16, 0, 0]);
    }

    #[test]
    fn test_window_ranges() {
        assert!(inside_range(8, 8 << 8 | 16, 240));
        assert!(!inside_range(16, 8 << 8 | 16, 240));
        assert!(inside_range(239, 8 << 8 | 0, 240));
        assert!(!inside_range(100, 200 << 8 | 100, 240));
        assert!(inside_range(210, 200 << 8 | 100, 240));
    }
}
