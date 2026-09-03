mod common;

use common::*;

const DISPSTAT: u32 = 0x0400_0004;
const VCOUNT: u32 = 0x0400_0006;
const IF: u32 = 0x0400_0202;

#[test]
fn vblank_flag_is_set_during_vblank_lines() {
    let mut gba = gba_without_rom();
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 159, 1_000_000));
    assert_eq!(gba.mem.read_u16(DISPSTAT) & 1, 0);
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 160, 1_000_000));
    assert_eq!(gba.mem.read_u16(DISPSTAT) & 1, 1);
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 227, 1_000_000));
    assert_eq!(gba.mem.read_u16(DISPSTAT) & 1, 0);
}

#[test]
fn vblank_irq_requires_dispstat_enable() {
    let mut gba = gba_without_rom();
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 160, 1_000_000));
    assert_eq!(gba.mem.read_u16(IF) & 1, 0);

    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPSTAT, 1 << 3);
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 160, 1_000_000));
    assert_eq!(gba.mem.read_u16(IF) & 1, 1);
}

#[test]
fn hblank_flag_toggles_within_a_scanline() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPSTAT, 1 << 4);
    assert!(gba.run_until(|gba| gba.mem.read_u16(DISPSTAT) & 2 != 0, 10_000));
    assert_eq!(gba.mem.read_u16(VCOUNT), 0);
    assert_eq!(gba.mem.read_u16(IF) & 2, 2);
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 1, 10_000));
    assert_eq!(gba.mem.read_u16(DISPSTAT) & 2, 0);
}

#[test]
fn vcount_match_sets_flag_and_irq() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPSTAT, 42 << 8 | 1 << 5);
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 42, 1_000_000));
    assert_eq!(gba.mem.read_u16(DISPSTAT) & 4, 4);
    assert_eq!(gba.mem.read_u16(IF) & 4, 4);
    assert!(gba.run_until(|gba| gba.mem.read_u16(VCOUNT) == 43, 1_000_000));
    assert_eq!(gba.mem.read_u16(DISPSTAT) & 4, 0);
}

#[test]
fn frames_end_on_the_frame_boundary() {
    let mut gba = gba_without_rom();
    for _ in 0..5 {
        gba.run_scanline();
    }
    gba.run_frame();
    assert_eq!(gba.scanline() % 228, 0);
    assert_eq!(gba.frame_count(), 1);
}

#[test]
fn the_picture_changes_only_at_vblank() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(PALETTE, rgb555(31, 0, 0));
    gba.mem.write_u16(DISPCNT, 0);
    gba.run_frame();
    assert_eq!(gba.framebuffer()[100][100], rgb(31, 0, 0));
    gba.mem.write_u16(PALETTE, rgb555(0, 31, 0));
    for _ in 0..159 {
        gba.run_scanline();
    }
    assert_eq!(gba.framebuffer()[100][100], rgb(31, 0, 0));
    gba.run_scanline();
    assert_eq!(gba.framebuffer()[100][100], rgb(0, 31, 0));
}
