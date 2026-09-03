mod common;

use common::*;
use gbae::system::gba::Gba;

const SCREEN_BASE_1: u32 = VRAM + 0x800;
const SCREEN_BASE_2: u32 = VRAM + 0x1000;
const OAM: u32 = 0x0700_0000;
const OBJ_PALETTE: u32 = PALETTE + 0x200;
const OBJ_TILES: u32 = VRAM + 0x1_0000;
const BG2CNT: u32 = 0x0400_000C;
const BG0HOFS: u32 = 0x0400_0010;
const BG0VOFS: u32 = 0x0400_0012;
const WIN0H: u32 = 0x0400_0040;
const WIN0V: u32 = 0x0400_0044;
const WININ: u32 = 0x0400_0048;
const WINOUT: u32 = 0x0400_004A;
const BLDCNT: u32 = 0x0400_0050;
const BLDALPHA: u32 = 0x0400_0052;
const BLDY: u32 = 0x0400_0054;
const GREENSWAP: u32 = 0x0400_0002;
const MOSAIC: u32 = 0x0400_004C;

fn fill_tile(gba: &mut Gba, tile: u32, left: u16, right: u16) {
    for row in 0..8 {
        gba.mem.write_u16(VRAM + tile * 32 + row * 4, left * 0x1111);
        gba.mem.write_u16(VRAM + tile * 32 + row * 4 + 2, right * 0x1111);
    }
}

fn setup_mode0(gba: &mut Gba, bg_cnt: u32, bg_enable: u16) {
    gba.mem.write_u16(PALETTE, rgb555(0, 0, 0));
    gba.mem.write_u16(PALETTE + 2, rgb555(31, 0, 0));
    gba.mem.write_u16(PALETTE + 4, rgb555(0, 31, 0));
    fill_tile(gba, 1, 1, 1);
    fill_tile(gba, 2, 1, 2);
    gba.mem.write_u16(bg_cnt, 1 << 8);
    gba.mem.write_u16(DISPCNT, bg_enable);
}

#[test]
fn mode0_draws_4bpp_tile_through_palette() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][0], rgb(31, 0, 0));
    assert_eq!(fb[7][7], rgb(31, 0, 0));
    assert_eq!(fb[0][8], rgb(0, 0, 0));
    assert_eq!(fb[8][0], rgb(0, 0, 0));
}

#[test]
fn mode0_horizontal_flip_mirrors_tile() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8);
    gba.mem.write_u16(SCREEN_BASE_1, 2);
    gba.mem.write_u16(SCREEN_BASE_1 + 2, 2 | 1 << 10);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][0], rgb(31, 0, 0));
    assert_eq!(fb[0][7], rgb(0, 31, 0));
    assert_eq!(fb[0][8], rgb(0, 31, 0));
    assert_eq!(fb[0][15], rgb(31, 0, 0));
}

#[test]
fn mode0_draws_bg1_when_bg0_is_disabled() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG1CNT, 1 << 9);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(31, 0, 0));
}

#[test]
fn mode3_draws_16bit_pixels_from_vram() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPCNT, 3 | 1 << 10);
    gba.mem.write_u16(VRAM + (3 * 240 + 5) * 2, rgb555(0, 0, 31));
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[3][5], rgb(0, 0, 31));
    assert_eq!(fb[3][6], rgb(0, 0, 0));
}

#[test]
fn mode4_draws_8bit_palette_indices_from_vram() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(PALETTE + 2, rgb555(31, 0, 0));
    gba.mem.write_u16(DISPCNT, 4 | 1 << 10);
    gba.mem.write_u16(VRAM + 3 * 240 + 4, 0x0101);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[3][4], rgb(31, 0, 0));
    assert_eq!(fb[3][5], rgb(31, 0, 0));
    assert_eq!(fb[3][6], rgb(0, 0, 0));
}

#[test]
fn forced_blank_draws_white() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPCNT, 1 << 7);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], [255, 255, 255]);
}

#[test]
fn mode0_checkerboard_matches_golden() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8);
    for tile_y in 0..20u32 {
        for tile_x in 0..30u32 {
            let tile = if (tile_x + tile_y) % 2 == 0 { 1 } else { 2 };
            gba.mem.write_u16(SCREEN_BASE_1 + (tile_y * 32 + tile_x) * 2, tile);
        }
    }
    gba.run_frame();
    assert_matches_golden(gba.framebuffer(), "mode0_checkerboard");
}

#[test]
fn no_background_enabled_draws_backdrop_color() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(PALETTE, rgb555(31, 31, 0));
    gba.mem.write_u16(DISPCNT, 0);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(31, 31, 0));
    assert_eq!(gba.framebuffer()[159][239], rgb(31, 31, 0));
}

#[test]
fn mode0_scrolls_with_bg_offsets() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.mem.write_u16(BG0HOFS, 4);
    gba.mem.write_u16(BG0VOFS, 3);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][3], rgb(31, 0, 0));
    assert_eq!(fb[0][4], rgb(0, 0, 0));
    assert_eq!(fb[4][0], rgb(31, 0, 0));
    assert_eq!(fb[5][0], rgb(0, 0, 0));
}

#[test]
fn background_with_lower_priority_value_is_drawn_on_top() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8 | 1 << 9);
    fill_tile(&mut gba, 3, 2, 2);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.mem.write_u16(SCREEN_BASE_2, 3);
    gba.mem.write_u16(BG0CNT, 1 << 8 | 1);
    gba.mem.write_u16(BG1CNT, 2 << 8 | 0);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(0, 31, 0));

    gba.mem.write_u16(BG0CNT, 1 << 8 | 0);
    gba.mem.write_u16(BG1CNT, 2 << 8 | 1);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(31, 0, 0));
}

#[test]
fn sprite_is_drawn_over_the_backdrop() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(OBJ_PALETTE + 2, rgb555(0, 0, 31));
    for row in 0..8 {
        gba.mem.write_u32(OBJ_TILES + 32 + row * 4, 0x1111_1111);
    }
    gba.mem.write_u16(OAM, 20);
    gba.mem.write_u16(OAM + 2, 10);
    gba.mem.write_u16(OAM + 4, 1);
    gba.mem.write_u16(DISPCNT, 1 << 12 | 1 << 6);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[20][10], rgb(0, 0, 31));
    assert_eq!(fb[27][17], rgb(0, 0, 31));
    assert_eq!(fb[20][18], rgb(0, 0, 0));
    assert_eq!(fb[19][10], rgb(0, 0, 0));
}

#[test]
fn sprite_priority_decides_against_background() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8 | 1 << 12 | 1 << 6);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.mem.write_u16(BG0CNT, 1 << 8 | 1);
    gba.mem.write_u16(OBJ_PALETTE + 2, rgb555(0, 0, 31));
    for row in 0..8 {
        gba.mem.write_u32(OBJ_TILES + 32 + row * 4, 0x1111_1111);
    }
    gba.mem.write_u16(OAM, 0);
    gba.mem.write_u16(OAM + 2, 0);
    gba.mem.write_u16(OAM + 4, 1 | 2 << 10);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(31, 0, 0));

    gba.mem.write_u16(OAM + 4, 1 | 1 << 10);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(0, 0, 31));
}

#[test]
fn alpha_blending_mixes_first_and_second_target() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8 | 1 << 9);
    fill_tile(&mut gba, 3, 2, 2);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.mem.write_u16(SCREEN_BASE_2, 3);
    gba.mem.write_u16(BG1CNT, 2 << 8 | 1);
    gba.mem.write_u16(BLDCNT, 1 << 6 | 1 << 0 | 1 << 9);
    gba.mem.write_u16(BLDALPHA, 8 | 8 << 8);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(15, 15, 0));
}

#[test]
fn brightness_decrease_darkens_first_target() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8);
    gba.mem.write_u16(SCREEN_BASE_1, 1);
    gba.mem.write_u16(BLDCNT, 3 << 6 | 1 << 0);
    gba.mem.write_u16(BLDY, 8);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[0][0], rgb(16, 0, 0));
}

#[test]
fn window0_limits_background_to_its_rectangle() {
    let mut gba = gba_without_rom();
    setup_mode0(&mut gba, BG0CNT, 1 << 8 | 1 << 13);
    for tile_x in 0..30u32 {
        gba.mem.write_u16(SCREEN_BASE_1 + tile_x * 2, 1);
    }
    gba.mem.write_u16(WIN0H, 8 << 8 | 16);
    gba.mem.write_u16(WIN0V, 0 << 8 | 160);
    gba.mem.write_u16(WININ, 1);
    gba.mem.write_u16(WINOUT, 0);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][7], rgb(0, 0, 0));
    assert_eq!(fb[0][8], rgb(31, 0, 0));
    assert_eq!(fb[0][15], rgb(31, 0, 0));
    assert_eq!(fb[0][16], rgb(0, 0, 0));
}

#[test]
fn mode2_draws_affine_background_from_8bit_tiles() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(PALETTE + 2, rgb555(31, 0, 0));
    for row in 0..8 {
        gba.mem.write_u32(VRAM + 64 + row * 8, 0x0101_0101);
        gba.mem.write_u32(VRAM + 64 + row * 8 + 4, 0x0101_0101);
    }
    gba.mem.write_u16(VRAM + 0x4000, 0x0001);
    gba.mem.write_u16(BG2CNT, 8 << 8);
    gba.mem.write_u16(DISPCNT, 2 | 1 << 10);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][0], rgb(31, 0, 0));
    assert_eq!(fb[7][7], rgb(31, 0, 0));
    assert_eq!(fb[0][8], rgb(0, 0, 0));
    assert_eq!(fb[8][0], rgb(0, 0, 0));
}

fn place_square_sprite(gba: &mut Gba, index: u32, x: u16, y: u16, mosaic: bool) {
    gba.mem.write_u16(OAM + index * 8, y | (mosaic as u16) << 12);
    gba.mem.write_u16(OAM + index * 8 + 2, x);
    gba.mem.write_u16(OAM + index * 8 + 4, 1);
}

fn sprite_tile_with_gradient(gba: &mut Gba) {
    for column in 0..8u32 {
        gba.mem.write_u16(OBJ_PALETTE + 2 + column * 2, rgb555(column as u8 * 4, 0, 0));
    }
    for row in 0..8 {
        gba.mem.write_u32(OBJ_TILES + 32 + row * 4, 0x7654_3210 + 0x1111_1111);
    }
}

#[test]
fn sprite_horizontal_mosaic_repeats_block_start_pixels() {
    let mut gba = gba_without_rom();
    sprite_tile_with_gradient(&mut gba);
    place_square_sprite(&mut gba, 0, 0, 0, true);
    gba.mem.write_u16(MOSAIC, 3 << 8);
    gba.mem.write_u16(DISPCNT, 1 << 12 | 1 << 6);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][0], rgb(0, 0, 0));
    assert_eq!(fb[0][3], rgb(0, 0, 0));
    assert_eq!(fb[0][4], rgb(16, 0, 0));
    assert_eq!(fb[0][7], rgb(16, 0, 0));
}

#[test]
fn sprite_cycle_budget_drops_sprites_beyond_the_limit() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(OBJ_PALETTE + 2, rgb555(0, 0, 31));
    for row in 0..8 {
        gba.mem.write_u32(OBJ_TILES + 32 + row * 4, 0x1111_1111);
    }
    let big_sprites = |gba: &mut Gba, count: u32| {
        for index in 0..20u32 {
            gba.mem.write_u16(OAM + index * 8, if index < count { 0 } else { 160 });
            gba.mem.write_u16(OAM + index * 8 + 2, 100 | 3 << 14);
            gba.mem.write_u16(OAM + index * 8 + 4, 1);
        }
    };
    gba.mem.write_u16(OAM + 20 * 8, 8);
    gba.mem.write_u16(OAM + 20 * 8 + 2, 0);
    gba.mem.write_u16(OAM + 20 * 8 + 4, 1);
    gba.mem.write_u16(DISPCNT, 1 << 12 | 1 << 6);

    big_sprites(&mut gba, 10);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[8][0], rgb(0, 0, 31));

    big_sprites(&mut gba, 20);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[8][0], rgb(0, 0, 0));
}

#[test]
fn green_swap_exchanges_green_between_pixel_pairs() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPCNT, 3 | 1 << 10);
    gba.mem.write_u16(VRAM, rgb555(31, 31, 0));
    gba.mem.write_u16(VRAM + 2, rgb555(0, 0, 31));
    gba.mem.write_u16(GREENSWAP, 1);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[0][0], rgb(31, 0, 0));
    assert_eq!(fb[0][1], rgb(0, 31, 31));
}

#[test]
fn affine_background_mosaic_holds_reference_for_block_rows() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(PALETTE + 2, rgb555(31, 0, 0));
    gba.mem.write_u16(PALETTE + 4, rgb555(0, 31, 0));
    for row in 0..8 {
        let value = if row < 4 { 0x0101_0101 } else { 0x0202_0202 };
        gba.mem.write_u32(VRAM + 64 + row * 8, value);
        gba.mem.write_u32(VRAM + 64 + row * 8 + 4, value);
    }
    gba.mem.write_u16(VRAM + 0x4000, 0x0101);
    gba.mem.write_u16(VRAM + 0x4010, 0x0101);
    gba.mem.write_u16(BG2CNT, 8 << 8 | 1 << 6);
    gba.mem.write_u16(MOSAIC, 5 << 4);
    gba.mem.write_u16(DISPCNT, 2 | 1 << 10);
    gba.run_frame();
    let fb = gba.framebuffer();
    assert_eq!(fb[3][0], rgb(31, 0, 0));
    assert_eq!(fb[5][0], rgb(31, 0, 0));
    assert_eq!(fb[6][0], rgb(0, 31, 0));
    assert_eq!(fb[11][0], rgb(0, 31, 0));
    assert_eq!(fb[12][0], rgb(0, 31, 0));
}
