mod common;

use common::*;

const DMA3SAD: u32 = 0x0400_00D4;
const DMA3DAD: u32 = 0x0400_00D8;
const DMA3CNT_L: u32 = 0x0400_00DC;
const DMA3CNT_H: u32 = 0x0400_00DE;
const EWRAM: u32 = 0x0200_0000;
const IWRAM: u32 = 0x0300_0000;
const ENABLE: u16 = 1 << 15;
const WORD: u16 = 1 << 10;
const SOURCE_FIXED: u16 = 2 << 7;
const TIMING_VBLANK: u16 = 1 << 12;
const TIMING_HBLANK: u16 = 2 << 12;
const REPEAT: u16 = 1 << 9;

#[test]
fn immediate_dma3_copies_words() {
    let mut gba = gba_without_rom();
    for i in 0..4 {
        gba.mem.write_u32(EWRAM + i * 4, 0x1000_0000 + i);
    }
    gba.mem.write_u32(DMA3SAD, EWRAM);
    gba.mem.write_u32(DMA3DAD, IWRAM);
    gba.mem.write_u16(DMA3CNT_L, 4);
    gba.mem.write_u16(DMA3CNT_H, ENABLE | WORD);
    for i in 0..4 {
        assert_eq!(gba.mem.read_u32(IWRAM + i * 4), 0x1000_0000 + i);
    }
    assert_eq!(gba.mem.read_u32(IWRAM + 16), 0);
    assert_eq!(gba.mem.read_u16(DMA3CNT_H) & ENABLE, 0);
}

#[test]
fn immediate_dma3_copies_halfwords_from_fixed_source() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(EWRAM, 0x1234);
    gba.mem.write_u32(DMA3SAD, EWRAM);
    gba.mem.write_u32(DMA3DAD, IWRAM);
    gba.mem.write_u16(DMA3CNT_L, 3);
    gba.mem.write_u16(DMA3CNT_H, ENABLE | SOURCE_FIXED);
    assert_eq!(gba.mem.read_u16(IWRAM), 0x1234);
    assert_eq!(gba.mem.read_u16(IWRAM + 2), 0x1234);
    assert_eq!(gba.mem.read_u16(IWRAM + 4), 0x1234);
    assert_eq!(gba.mem.read_u16(IWRAM + 6), 0);
}

#[test]
fn vblank_dma3_waits_for_vblank() {
    let mut gba = gba_without_rom();
    gba.mem.write_u32(EWRAM, 0xCAFE_F00D);
    gba.mem.write_u32(DMA3SAD, EWRAM);
    gba.mem.write_u32(DMA3DAD, IWRAM);
    gba.mem.write_u16(DMA3CNT_L, 1);
    gba.mem.write_u16(DMA3CNT_H, ENABLE | WORD | TIMING_VBLANK);
    assert_eq!(gba.mem.read_u32(IWRAM), 0);
    assert!(gba.run_until(|gba| gba.mem.read_u16(0x0400_0006) == 160, 1_000_000));
    assert_eq!(gba.mem.read_u32(IWRAM), 0xCAFE_F00D);
}

#[test]
fn immediate_dma3_into_io_registers_does_not_retrigger() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(EWRAM, 0x0001);
    gba.mem.write_u16(EWRAM + 2, 0x0002);
    gba.mem.write_u32(DMA3SAD, EWRAM);
    gba.mem.write_u32(DMA3DAD, BG0CNT);
    gba.mem.write_u16(DMA3CNT_L, 2);
    gba.mem.write_u16(DMA3CNT_H, ENABLE);
    assert_eq!(gba.mem.read_u16(BG0CNT), 0x0001);
    assert_eq!(gba.mem.read_u16(BG1CNT), 0x0002);
    assert_eq!(gba.mem.read_u16(DMA3CNT_H) & ENABLE, 0);
}

#[test]
fn hblank_dma3_repeats_on_every_visible_line() {
    let mut gba = gba_without_rom();
    gba.mem.write_u32(EWRAM, 0x1234_5678);
    gba.mem.write_u32(DMA3SAD, EWRAM);
    gba.mem.write_u32(DMA3DAD, IWRAM);
    gba.mem.write_u16(DMA3CNT_L, 1);
    gba.mem.write_u16(DMA3CNT_H, ENABLE | WORD | REPEAT | SOURCE_FIXED | TIMING_HBLANK);
    assert!(gba.run_until(|gba| gba.mem.read_u16(0x0400_0006) == 3, 100_000));
    assert_eq!(gba.mem.read_u32(IWRAM), 0x1234_5678);
    assert_eq!(gba.mem.read_u32(IWRAM + 4), 0x1234_5678);
    assert_eq!(gba.mem.read_u32(IWRAM + 8), 0x1234_5678);
    assert_eq!(gba.mem.read_u32(IWRAM + 12), 0);
    assert_eq!(gba.mem.read_u16(DMA3CNT_H) & ENABLE, ENABLE);
}
