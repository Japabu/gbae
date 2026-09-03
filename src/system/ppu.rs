use crate::bits::Bits;

use super::{
    memory::{IoRegisters, Memory},
    state::{Reader, StateError, Writer},
};

pub const FRAMEBUFFER_WIDTH: usize = 240;
pub const FRAMEBUFFER_HEIGHT: usize = 160;

const BG_TILE_LIMIT: usize = 0x1_0000;
const OBJ_TILE_BASE: usize = 0x1_0000;
const OBJ_TILE_MASK: usize = 0x7FFF;
const OBJ_PALETTE_BASE: usize = 0x200;
const OBJ_COUNT: usize = 128;
const NO_OBJ_PRIORITY: u8 = 4;
const OBJ_CYCLES_PER_LINE: i32 = 1210;
const OBJ_CYCLES_PER_LINE_HBLANK_FREE: i32 = 954;

pub type Framebuffer = [[[u8; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT];

fn halfword(buffer: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([buffer[offset], buffer[offset + 1]])
}

fn signed(value: u16) -> i32 {
    i32::from(value as i16)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(u16);

type Pixel = Option<Color>;

impl Color {
    const MAX_CHANNEL: u32 = 31;

    fn from_palette(palette: &[u8], index: usize) -> Color {
        Color(halfword(palette, index * 2).bits(0..15))
    }

    fn from_vram(vram: &[u8], offset: usize) -> Color {
        Color(halfword(vram, offset).bits(0..15))
    }

    fn channels(self) -> [u32; 3] {
        [u32::from(self.0.bits(0..5)), u32::from(self.0.bits(5..10)), u32::from(self.0.bits(10..15))]
    }

    fn from_channels([red, green, blue]: [u32; 3]) -> Color {
        Color((red | green << 5 | blue << 10) as u16)
    }

    fn blend(self, other: Color, eva: u32, evb: u32) -> Color {
        let (first, second) = (self.channels(), other.channels());
        Color::from_channels([0, 1, 2].map(|channel| ((first[channel] * eva + second[channel] * evb) >> 4).min(Color::MAX_CHANNEL)))
    }

    fn brighten(self, evy: u32) -> Color {
        Color::from_channels(self.channels().map(|channel| channel + (((Color::MAX_CHANNEL - channel) * evy) >> 4)))
    }

    fn darken(self, evy: u32) -> Color {
        Color::from_channels(self.channels().map(|channel| channel - ((channel * evy) >> 4)))
    }

    fn rgb888(self) -> [u8; 3] {
        self.channels().map(|channel| (channel << 3) as u8)
    }
}

#[derive(Debug, Clone, Copy)]
struct DisplayControl(u16);

impl DisplayControl {
    fn mode(self) -> u16 {
        self.0.bits(0..3)
    }

    fn frame_select(self) -> bool {
        self.0.bit(4)
    }

    fn hblank_interval_free(self) -> bool {
        self.0.bit(5)
    }

    fn objects_one_dimensional(self) -> bool {
        self.0.bit(6)
    }

    fn forced_blank(self) -> bool {
        self.0.bit(7)
    }

    fn background_enabled(self, bg: usize) -> bool {
        self.0.bit(8 + bg as u32)
    }

    fn objects_enabled(self) -> bool {
        self.0.bit(12)
    }

    fn window_enabled(self, window: usize) -> bool {
        self.0.bit(13 + window as u32)
    }

    fn object_window_enabled(self) -> bool {
        self.0.bit(15)
    }

    fn windows_enabled(self) -> bool {
        self.0.bits(13..16) != 0
    }

    fn is_bitmap(self) -> bool {
        self.mode() >= 3
    }

    fn background_active(self, bg: usize) -> bool {
        match self.mode() {
            0 => true,
            1 => bg <= 2,
            2 => bg >= 2,
            _ => bg == 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BackgroundControl(u16);

impl BackgroundControl {
    fn priority(self) -> u16 {
        self.0.bits(0..2)
    }

    fn character_base(self) -> usize {
        usize::from(self.0.bits(2..4)) * 0x4000
    }

    fn mosaic(self) -> bool {
        self.0.bit(6)
    }

    fn eight_bit(self) -> bool {
        self.0.bit(7)
    }

    fn screen_base(self) -> usize {
        usize::from(self.0.bits(8..13)) * 0x800
    }

    fn wraps(self) -> bool {
        self.0.bit(13)
    }

    fn size(self) -> u16 {
        self.0.bits(14..16)
    }
}

#[derive(Debug, Clone, Copy)]
struct MosaicSize {
    width: usize,
    height: usize,
}

impl MosaicSize {
    const NONE: MosaicSize = MosaicSize { width: 1, height: 1 };

    fn decode(field: u16) -> MosaicSize {
        MosaicSize {
            width: usize::from(field.bits(0..4)) + 1,
            height: usize::from(field.bits(4..8)) + 1,
        }
    }

    fn background(io: &IoRegisters, enabled: bool) -> MosaicSize {
        if enabled {
            MosaicSize::decode(io.mosaic.bits(0..8))
        } else {
            MosaicSize::NONE
        }
    }

    fn object(io: &IoRegisters, enabled: bool) -> MosaicSize {
        if enabled {
            MosaicSize::decode(io.mosaic.bits(8..16))
        } else {
            MosaicSize::NONE
        }
    }

    fn snap_x(self, x: usize) -> usize {
        x - x % self.width
    }

    fn snap_y(self, y: usize) -> usize {
        y - y % self.height
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlendMode {
    Off,
    Alpha,
    Brighten,
    Darken,
}

#[derive(Debug, Clone, Copy)]
struct Blend {
    mode: BlendMode,
    control: u16,
    eva: u32,
    evb: u32,
    evy: u32,
}

impl Blend {
    fn read(io: &IoRegisters) -> Blend {
        let coefficient = |field: u16| u32::from(field).min(16);
        Blend {
            mode: match io.blend_cnt.bits(6..8) {
                0 => BlendMode::Off,
                1 => BlendMode::Alpha,
                2 => BlendMode::Brighten,
                _ => BlendMode::Darken,
            },
            control: io.blend_cnt,
            eva: coefficient(io.blend_alpha.bits(0..5)),
            evb: coefficient(io.blend_alpha.bits(8..13)),
            evy: coefficient(io.blend_y.bits(0..5)),
        }
    }

    fn first_target(&self, layer: Layer) -> bool {
        self.control.bit(layer.bit())
    }

    fn second_target(&self, layer: Layer) -> bool {
        self.control.bit(8 + layer.bit())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowControl(u16);

impl WindowControl {
    const EVERYTHING: WindowControl = WindowControl(0x3F);

    fn shows(self, layer: Layer) -> bool {
        match layer {
            Layer::Backdrop => true,
            layer => self.0.bit(layer.bit()),
        }
    }

    fn effects(self) -> bool {
        self.0.bit(5)
    }
}

fn inside_range(value: usize, bounds: u16, limit: usize) -> bool {
    let start = usize::from(bounds.bits(8..16));
    let mut end = usize::from(bounds.bits(0..8));
    if end > limit || start > end {
        end = limit;
    }
    (start..end).contains(&value)
}

fn inside_window(horizontal: u16, vertical: u16, x: usize, y: usize) -> bool {
    inside_range(x, horizontal, FRAMEBUFFER_WIDTH) && inside_range(y, vertical, FRAMEBUFFER_HEIGHT)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Background(usize),
    Object(u8),
    Backdrop,
}

impl Layer {
    fn bit(self) -> u32 {
        match self {
            Layer::Background(bg) => bg as u32,
            Layer::Object(_) => 4,
            Layer::Backdrop => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectMode {
    Normal,
    SemiTransparent,
    Window,
    Prohibited,
}

#[derive(Debug, Clone, Copy)]
enum Shape {
    Square,
    Wide,
    Tall,
}

impl Shape {
    fn size(self, index: usize) -> (i32, i32) {
        let sizes = match self {
            Shape::Square => [(8, 8), (16, 16), (32, 32), (64, 64)],
            Shape::Wide => [(16, 8), (32, 8), (32, 16), (64, 32)],
            Shape::Tall => [(8, 16), (8, 32), (16, 32), (32, 64)],
        };
        sizes[index]
    }
}

#[derive(Debug, Clone, Copy)]
struct Object {
    y: i32,
    x: i32,
    affine: bool,
    double_size: bool,
    mode: ObjectMode,
    mosaic: bool,
    eight_bit: bool,
    shape: Option<Shape>,
    affine_group: usize,
    horizontal_flip: bool,
    vertical_flip: bool,
    size_index: usize,
    tile: usize,
    priority: u8,
    palette_bank: usize,
}

impl Object {
    fn read(oam: &[u8], index: usize) -> Object {
        let [attribute0, attribute1, attribute2] = [0, 2, 4].map(|offset| halfword(oam, index * 8 + offset));
        let affine = attribute0.bit(8);
        let mut y = i32::from(attribute0.bits(0..8));
        if y >= FRAMEBUFFER_HEIGHT as i32 {
            y -= 256;
        }
        let mut x = i32::from(attribute1.bits(0..9));
        if x >= FRAMEBUFFER_WIDTH as i32 {
            x -= 512;
        }
        Object {
            y,
            x,
            affine,
            double_size: attribute0.bit(9),
            mode: match attribute0.bits(10..12) {
                0 => ObjectMode::Normal,
                1 => ObjectMode::SemiTransparent,
                2 => ObjectMode::Window,
                _ => ObjectMode::Prohibited,
            },
            mosaic: attribute0.bit(12),
            eight_bit: attribute0.bit(13),
            shape: match attribute0.bits(14..16) {
                0 => Some(Shape::Square),
                1 => Some(Shape::Wide),
                2 => Some(Shape::Tall),
                _ => None,
            },
            affine_group: usize::from(attribute1.bits(9..14)),
            horizontal_flip: !affine && attribute1.bit(12),
            vertical_flip: !affine && attribute1.bit(13),
            size_index: usize::from(attribute1.bits(14..16)),
            tile: usize::from(attribute2.bits(0..10)),
            priority: attribute2.bits(10..12) as u8,
            palette_bank: usize::from(attribute2.bits(12..16)),
        }
    }

    fn is_disabled(&self) -> bool {
        (!self.affine && self.double_size) || self.mode == ObjectMode::Prohibited || self.shape.is_none()
    }

    fn size(&self) -> (i32, i32) {
        self.shape.map_or((8, 8), |shape| shape.size(self.size_index))
    }

    fn bounding_box(&self) -> (i32, i32) {
        let (width, height) = self.size();
        if self.affine && self.double_size {
            (width * 2, height * 2)
        } else {
            (width, height)
        }
    }

    fn affine_parameters(&self, oam: &[u8]) -> [i32; 4] {
        if self.affine {
            let group = self.affine_group * 32;
            [6, 14, 22, 30].map(|offset| signed(halfword(oam, group + offset)))
        } else {
            [0x100, 0, 0, 0x100]
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ObjPixel {
    color: Pixel,
    priority: u8,
    semi_transparent: bool,
}

const NO_OBJ_PIXEL: ObjPixel = ObjPixel {
    color: None,
    priority: NO_OBJ_PRIORITY,
    semi_transparent: false,
};

pub struct PPU {
    drawing: Box<Framebuffer>,
    framebuffer: Box<Framebuffer>,
    affine_reference: [[i32; 2]; 2],
    affine_mosaic_reference: [[i32; 2]; 2],
    bg_lines: [[Pixel; FRAMEBUFFER_WIDTH]; 4],
    obj_line: [ObjPixel; FRAMEBUFFER_WIDTH],
    obj_window: [bool; FRAMEBUFFER_WIDTH],
    layers: [Layer; 8],
    layer_count: usize,
}

impl Default for PPU {
    fn default() -> PPU {
        PPU::new()
    }
}

impl PPU {
    pub fn new() -> PPU {
        PPU {
            drawing: Box::new([[[0; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT]),
            framebuffer: Box::new([[[0; 3]; FRAMEBUFFER_WIDTH]; FRAMEBUFFER_HEIGHT]),
            affine_reference: [[0; 2]; 2],
            affine_mosaic_reference: [[0; 2]; 2],
            bg_lines: [[None; FRAMEBUFFER_WIDTH]; 4],
            obj_line: [NO_OBJ_PIXEL; FRAMEBUFFER_WIDTH],
            obj_window: [false; FRAMEBUFFER_WIDTH],
            layers: [Layer::Backdrop; 8],
            layer_count: 0,
        }
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn finish_frame(&mut self) {
        std::mem::swap(&mut self.drawing, &mut self.framebuffer);
    }

    pub fn save_state(&self, writer: &mut Writer) {
        for pixel in self.framebuffer.iter().flatten() {
            writer.bytes(pixel);
        }
        for reference in self.affine_reference.iter().chain(&self.affine_mosaic_reference) {
            writer.i32s(reference);
        }
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        for pixel in self.framebuffer.iter_mut().flatten() {
            reader.bytes_into(pixel)?;
        }
        *self.drawing = *self.framebuffer;
        for reference in self.affine_reference.iter_mut().chain(&mut self.affine_mosaic_reference) {
            reader.i32s(reference)?;
        }
        Ok(())
    }

    pub fn latch_affine_references(&mut self, io: &mut IoRegisters) {
        for bg in 0..2 {
            if io.v_count == 0 || io.bg_reference_written[bg] {
                self.affine_reference[bg] = io.bg_reference[bg].map(|reference| reference.sign_extended(28) as i32);
                io.bg_reference_written[bg] = false;
            }
        }
    }

    pub fn draw_scanline(&mut self, mem: &Memory) {
        let io = mem.io();
        let display = DisplayControl(io.disp_cnt);
        let y = usize::from(io.v_count);

        if MosaicSize::background(io, true).snap_y(y) == y {
            self.affine_mosaic_reference = self.affine_reference;
        }

        if display.forced_blank() {
            self.drawing[y] = [[255; 3]; FRAMEBUFFER_WIDTH];
        } else {
            self.render_backgrounds(y, io, mem.vram(), mem.palette_ram());
            self.render_objects(y, mem);
            self.sort_layers(io);
            self.compose(y, io, mem.palette_ram());
            if io.green_swap.bit(0) {
                for pair in self.drawing[y].chunks_exact_mut(2) {
                    let green = pair[0][1];
                    pair[0][1] = pair[1][1];
                    pair[1][1] = green;
                }
            }
        }

        for (reference, parameters) in self.affine_reference.iter_mut().zip(&io.bg_parameters) {
            let [_, pb, _, pd] = parameters.map(signed);
            reference[0] = reference[0].wrapping_add(pb);
            reference[1] = reference[1].wrapping_add(pd);
        }
    }

    fn render_backgrounds(&mut self, y: usize, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        for line in self.bg_lines.iter_mut() {
            line.fill(None);
        }
        let display = DisplayControl(io.disp_cnt);
        for bg in (0..4).filter(|bg| display.background_enabled(*bg) && display.background_active(*bg)) {
            match (display.mode(), bg) {
                (0, _) | (1, 0 | 1) => self.render_text_background(bg, y, io, vram, palette),
                (1 | 2, _) => self.render_affine_background(bg, io, vram, palette),
                (3..=5, 2) => self.render_bitmap_background(io, vram, palette),
                _ => {}
            }
        }
    }

    fn render_text_background(&mut self, bg: usize, y: usize, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        let control = BackgroundControl(io.bg_cnt[bg]);
        let character_base = control.character_base();
        let screen_base = control.screen_base();
        let eight_bit = control.eight_bit();
        let size = control.size();
        let width_mask = if size.bit(0) { 511 } else { 255 };
        let height_mask = if size.bit(1) { 511 } else { 255 };
        let blocks_per_row = if size == 0b11 { 2 } else { 1 };
        let mosaic = MosaicSize::background(io, control.mosaic());

        let yy = (mosaic.snap_y(y) + usize::from(io.bg_v_offset[bg])) & height_mask;
        let h_offset = usize::from(io.bg_h_offset[bg]);
        let line = &mut self.bg_lines[bg];
        let mut tile_column = usize::MAX;
        let mut entry = 0;

        for (x, pixel) in line.iter_mut().enumerate() {
            let xx = (mosaic.snap_x(x) + h_offset) & width_mask;
            if xx >> 3 != tile_column {
                tile_column = xx >> 3;
                let block = (yy >> 8) * blocks_per_row + (xx >> 8);
                let entry_offset = screen_base + block * 0x800 + (((yy & 0xFF) >> 3) * 32 + ((xx & 0xFF) >> 3)) * 2;
                entry = halfword(vram, entry_offset & 0xFFFF);
            }
            let tile = usize::from(entry.bits(0..10));
            let px = if entry.bit(10) { 7 - (xx & 7) } else { xx & 7 };
            let py = if entry.bit(11) { 7 - (yy & 7) } else { yy & 7 };
            *pixel = if eight_bit {
                bg_tile_color(vram, palette, character_base + tile * 64, px, py, true, 0)
            } else {
                bg_tile_color(vram, palette, character_base + tile * 32, px, py, false, usize::from(entry.bits(12..16)))
            };
        }
    }

    fn render_affine_background(&mut self, bg: usize, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        let control = BackgroundControl(io.bg_cnt[bg]);
        let character_base = control.character_base();
        let screen_base = control.screen_base();
        let size = 128 << control.size();
        let wraps = control.wraps();
        let mosaic = MosaicSize::background(io, control.mosaic());
        let [pa, _, pc, _] = io.bg_parameters[bg - 2].map(signed);
        let [mut px, mut py] = if control.mosaic() { self.affine_mosaic_reference[bg - 2] } else { self.affine_reference[bg - 2] };
        let line = &mut self.bg_lines[bg];

        for pixel in line.iter_mut() {
            let (mut tx, mut ty) = (px >> 8, py >> 8);
            if wraps {
                tx &= size - 1;
                ty &= size - 1;
            }
            *pixel = if (0..size).contains(&tx) && (0..size).contains(&ty) {
                let (tx, ty) = (tx as usize, ty as usize);
                let tile = usize::from(bg_vram_byte(vram, screen_base + (ty / 8) * (size as usize / 8) + tx / 8));
                bg_tile_color(vram, palette, character_base + tile * 64, tx & 7, ty & 7, true, 0)
            } else {
                None
            };
            px = px.wrapping_add(pa);
            py = py.wrapping_add(pc);
        }
        apply_horizontal_mosaic(line, mosaic);
    }

    fn render_bitmap_background(&mut self, io: &IoRegisters, vram: &[u8], palette: &[u8]) {
        let display = DisplayControl(io.disp_cnt);
        let mode = display.mode();
        let (width, height) = if mode == 5 { (160, 128) } else { (240, 160) };
        let frame = if mode != 3 && display.frame_select() { 0xA000 } else { 0 };
        let control = BackgroundControl(io.bg_cnt[2]);
        let mosaic = MosaicSize::background(io, control.mosaic());
        let [pa, _, pc, _] = io.bg_parameters[0].map(signed);
        let [mut px, mut py] = if control.mosaic() { self.affine_mosaic_reference[0] } else { self.affine_reference[0] };
        let line = &mut self.bg_lines[2];

        for pixel in line.iter_mut() {
            let (tx, ty) = (px >> 8, py >> 8);
            *pixel = if (0..width).contains(&tx) && (0..height).contains(&ty) {
                let (tx, ty) = (tx as usize, ty as usize);
                match mode {
                    3 => Some(Color::from_vram(vram, (ty * 240 + tx) * 2)),
                    4 => palette_color(palette, usize::from(vram[frame + ty * 240 + tx])),
                    _ => Some(Color::from_vram(vram, frame + (ty * 160 + tx) * 2)),
                }
            } else {
                None
            };
            px = px.wrapping_add(pa);
            py = py.wrapping_add(pc);
        }
        apply_horizontal_mosaic(line, mosaic);
    }

    fn render_objects(&mut self, y: usize, mem: &Memory) {
        self.obj_line.fill(NO_OBJ_PIXEL);
        self.obj_window.fill(false);
        let display = DisplayControl(mem.io().disp_cnt);
        if !display.objects_enabled() {
            return;
        }
        let mut budget = if display.hblank_interval_free() { OBJ_CYCLES_PER_LINE_HBLANK_FREE } else { OBJ_CYCLES_PER_LINE };
        for index in 0..OBJ_COUNT {
            budget -= self.render_object(Object::read(mem.oam(), index), y, mem, budget);
            if budget <= 0 {
                break;
            }
        }
    }

    fn render_object(&mut self, object: Object, y: usize, mem: &Memory, budget: i32) -> i32 {
        if object.is_disabled() {
            return 0;
        }
        let (io, vram, palette, oam) = (mem.io(), mem.vram(), mem.palette_ram(), mem.oam());
        let display = DisplayControl(io.disp_cnt);
        let (width, height) = object.size();
        let (box_width, box_height) = object.bounding_box();
        let mosaic = MosaicSize::object(io, object.mosaic);
        let row = mosaic.snap_y(y) as i32 - object.y;
        if !(0..box_height).contains(&row) {
            return 0;
        }
        let cycles = if object.affine { box_width * 2 + 10 } else { box_width };
        if cycles > budget {
            return cycles;
        }
        if display.is_bitmap() && object.tile < 512 {
            return cycles;
        }

        let tiles_per_unit = if object.eight_bit { 2 } else { 1 };
        let stride = if display.objects_one_dimensional() { (width as usize / 8) * tiles_per_unit } else { 32 };
        let [pa, pb, pc, pd] = object.affine_parameters(oam);

        for screen_column in 0..box_width {
            let screen_x = object.x + screen_column;
            if !(0..FRAMEBUFFER_WIDTH as i32).contains(&screen_x) {
                continue;
            }
            let column = mosaic.snap_x(screen_x as usize) as i32 - object.x;
            if column < 0 {
                continue;
            }
            let (tx, ty) = if object.affine {
                let ox = column - box_width / 2;
                let oy = row - box_height / 2;
                (((pa * ox + pb * oy) >> 8) + width / 2, ((pc * ox + pd * oy) >> 8) + height / 2)
            } else {
                (
                    if object.horizontal_flip { width - 1 - column } else { column },
                    if object.vertical_flip { height - 1 - row } else { row },
                )
            };
            if !(0..width).contains(&tx) || !(0..height).contains(&ty) {
                continue;
            }
            let (tx, ty) = (tx as usize, ty as usize);
            let tile = object.tile + (ty / 8) * stride + (tx / 8) * tiles_per_unit;
            let color_index = obj_tile_pixel(vram, tile, tx & 7, ty & 7, object.eight_bit);
            if color_index == 0 {
                continue;
            }

            let screen_x = screen_x as usize;
            if object.mode == ObjectMode::Window {
                self.obj_window[screen_x] = true;
            } else if object.priority < self.obj_line[screen_x].priority {
                let palette_index = if object.eight_bit { color_index } else { object.palette_bank * 16 + color_index };
                self.obj_line[screen_x] = ObjPixel {
                    color: Some(Color::from_palette(palette, OBJ_PALETTE_BASE / 2 + palette_index)),
                    priority: object.priority,
                    semi_transparent: object.mode == ObjectMode::SemiTransparent,
                };
            }
        }
        cycles
    }

    fn compose(&mut self, y: usize, io: &IoRegisters, palette: &[u8]) {
        let backdrop = Color::from_palette(palette, 0);
        let display = DisplayControl(io.disp_cnt);
        let blend = Blend::read(io);

        for x in 0..FRAMEBUFFER_WIDTH {
            let window = if display.windows_enabled() { self.window_control(x, y, io) } else { WindowControl::EVERYTHING };
            let [(top, top_color), (second, second_color)] = self.top_layers(x, window, backdrop);
            let first_target = blend.first_target(top);
            let second_target = blend.second_target(second);
            let semi_transparent_object = matches!(top, Layer::Object(_)) && self.obj_line[x].semi_transparent;

            let color = if !window.effects() {
                top_color
            } else if semi_transparent_object && second_target {
                top_color.blend(second_color, blend.eva, blend.evb)
            } else {
                match blend.mode {
                    BlendMode::Alpha if first_target && second_target => top_color.blend(second_color, blend.eva, blend.evb),
                    BlendMode::Brighten if first_target => top_color.brighten(blend.evy),
                    BlendMode::Darken if first_target => top_color.darken(blend.evy),
                    _ => top_color,
                }
            };
            self.drawing[y][x] = color.rgb888();
        }
    }

    fn sort_layers(&mut self, io: &IoRegisters) {
        let display = DisplayControl(io.disp_cnt);
        self.layer_count = 0;
        let mut push = |layer: Layer| {
            self.layers[self.layer_count] = layer;
            self.layer_count += 1;
        };
        for priority in 0..4 {
            if display.objects_enabled() {
                push(Layer::Object(priority as u8));
            }
            for bg in 0..4 {
                if display.background_enabled(bg) && display.background_active(bg) && BackgroundControl(io.bg_cnt[bg]).priority() == priority {
                    push(Layer::Background(bg));
                }
            }
        }
    }

    fn top_layers(&self, x: usize, window: WindowControl, backdrop: Color) -> [(Layer, Color); 2] {
        let object = self.obj_line[x];
        let visible = self.layers[..self.layer_count].iter().filter_map(|layer| {
            let color = match *layer {
                Layer::Object(priority) if object.priority == priority => object.color,
                Layer::Object(_) => None,
                Layer::Background(bg) => self.bg_lines[bg][x],
                Layer::Backdrop => Some(backdrop),
            };
            color.filter(|_| window.shows(*layer)).map(|color| (*layer, color))
        });
        let mut layers = [(Layer::Backdrop, backdrop); 2];
        for (slot, found) in layers.iter_mut().zip(visible) {
            *slot = found;
        }
        layers
    }

    fn window_control(&self, x: usize, y: usize, io: &IoRegisters) -> WindowControl {
        let display = DisplayControl(io.disp_cnt);
        let bits = if display.window_enabled(0) && inside_window(io.win_h[0], io.win_v[0], x, y) {
            io.win_in.bits(0..6)
        } else if display.window_enabled(1) && inside_window(io.win_h[1], io.win_v[1], x, y) {
            io.win_in.bits(8..14)
        } else if display.object_window_enabled() && self.obj_window[x] {
            io.win_out.bits(8..14)
        } else {
            io.win_out.bits(0..6)
        };
        WindowControl(bits)
    }
}

fn bg_vram_byte(vram: &[u8], offset: usize) -> u8 {
    if offset < BG_TILE_LIMIT {
        vram[offset]
    } else {
        0
    }
}

fn palette_color(palette: &[u8], index: usize) -> Pixel {
    (index != 0).then(|| Color::from_palette(palette, index))
}

fn bg_tile_color(vram: &[u8], palette: &[u8], tile_offset: usize, px: usize, py: usize, eight_bit: bool, palette_bank: usize) -> Pixel {
    if tile_offset >= BG_TILE_LIMIT {
        None
    } else if eight_bit {
        palette_color(palette, usize::from(vram[tile_offset + py * 8 + px]))
    } else {
        let byte = vram[tile_offset + py * 4 + px / 2];
        let index = usize::from(if px.is_multiple_of(2) { byte.bits(0..4) } else { byte.bits(4..8) });
        (index != 0).then(|| Color::from_palette(palette, palette_bank * 16 + index))
    }
}

fn obj_tile_pixel(vram: &[u8], tile: usize, px: usize, py: usize, eight_bit: bool) -> usize {
    if eight_bit {
        usize::from(vram[OBJ_TILE_BASE + ((tile * 32 + py * 8 + px) & OBJ_TILE_MASK)])
    } else {
        let byte = vram[OBJ_TILE_BASE + ((tile * 32 + py * 4 + px / 2) & OBJ_TILE_MASK)];
        usize::from(if px.is_multiple_of(2) { byte.bits(0..4) } else { byte.bits(4..8) })
    }
}

fn apply_horizontal_mosaic(line: &mut [Pixel; FRAMEBUFFER_WIDTH], mosaic: MosaicSize) {
    if mosaic.width > 1 {
        for x in 0..FRAMEBUFFER_WIDTH {
            line[x] = line[mosaic.snap_x(x)];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_effects() {
        let red = Color::from_channels([31, 0, 0]);
        let green = Color::from_channels([0, 31, 0]);
        assert_eq!(red.blend(green, 8, 8).channels(), [15, 15, 0]);
        assert_eq!(red.blend(red, 16, 16).channels(), [31, 0, 0]);
        assert_eq!(red.brighten(16).channels(), [31, 31, 31]);
        assert_eq!(red.darken(16).channels(), [0, 0, 0]);
        assert_eq!(red.darken(8).channels(), [16, 0, 0]);
        assert_eq!(red.rgb888(), [248, 0, 0]);
    }

    #[test]
    fn test_window_ranges() {
        assert!(inside_range(8, 8 << 8 | 16, 240));
        assert!(!inside_range(16, 8 << 8 | 16, 240));
        assert!(inside_range(239, 8 << 8, 240));
        assert!(!inside_range(100, 200 << 8 | 100, 240));
        assert!(inside_range(210, 200 << 8 | 100, 240));
    }

    #[test]
    fn test_object_attributes_decode() {
        let mut oam = vec![0u8; 8];
        oam[0..2].copy_from_slice(&(0x00A0u16 | 1 << 8 | 1 << 9 | 1 << 13 | 2 << 14).to_le_bytes());
        oam[2..4].copy_from_slice(&(0x01F0u16 | 3 << 9 | 3 << 14).to_le_bytes());
        oam[4..6].copy_from_slice(&(0x123u16 | 2 << 10 | 7 << 12).to_le_bytes());
        let object = Object::read(&oam, 0);
        assert_eq!((object.y, object.x), (-96, -16));
        assert!(object.affine && object.double_size && object.eight_bit);
        assert!(matches!(object.shape, Some(Shape::Tall)));
        assert_eq!(object.size(), (32, 64));
        assert_eq!(object.bounding_box(), (64, 128));
        assert_eq!((object.affine_group, object.tile, object.priority, object.palette_bank), (3, 0x123, 2, 7));
        assert!(!object.is_disabled());
    }
}
