mod common;

use common::*;

const SOUNDCNT_L: u32 = 0x0400_0080;
const SOUNDCNT_H: u32 = 0x0400_0082;
const SOUNDCNT_X: u32 = 0x0400_0084;
const SOUND1CNT_H: u32 = 0x0400_0062;
const SOUND1CNT_X: u32 = 0x0400_0064;
const FIFO_A: u32 = 0x0400_00A0;
const TM0CNT_L: u32 = 0x0400_0100;
const TM0CNT_H: u32 = 0x0400_0102;
const DMA1SAD: u32 = 0x0400_00BC;
const DMA1DAD: u32 = 0x0400_00C0;
const DMA1CNT_H: u32 = 0x0400_00C6;
const EWRAM: u32 = 0x0200_0000;

#[test]
fn a_frame_produces_the_expected_number_of_samples() {
    let mut gba = gba_without_rom();
    gba.run_frame();
    let samples = gba.take_audio_samples();
    let per_frame = 48_000.0 / 59.7275;
    assert!((samples.len() as f64 / 2.0 - per_frame).abs() < 2.0, "{} samples", samples.len() / 2);
    assert!(samples.iter().all(|sample| *sample == 0));
}

#[test]
fn square_channel_is_audible_after_enabling_sound() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(SOUNDCNT_X, 0x80);
    gba.mem.write_u16(SOUNDCNT_L, 0x7777);
    gba.mem.write_u16(SOUNDCNT_H, 0x2);
    gba.mem.write_u16(SOUND1CNT_H, 0xF000 | 2 << 6);
    gba.mem.write_u16(SOUND1CNT_X, 0x8000 | 1792);
    gba.run_frame();
    let samples = gba.take_audio_samples();
    assert!(samples.iter().any(|sample| *sample > 0));
    assert_eq!(gba.mem.read_u16(SOUNDCNT_X) & 1, 1);
}

#[test]
fn direct_sound_streams_fifo_through_timer_and_dma() {
    let mut gba = gba_without_rom();
    for i in 0..64u32 {
        gba.mem.write_u8(EWRAM + i, (i as i32 * 4 - 128) as i8 as u8);
    }
    gba.mem.write_u16(SOUNDCNT_X, 0x80);
    gba.mem.write_u16(SOUNDCNT_H, 0x2 | 0x4 | 0x100 | 0x200 | 0x800);
    gba.mem.write_u32(DMA1SAD, EWRAM);
    gba.mem.write_u32(DMA1DAD, FIFO_A);
    gba.mem.write_u16(DMA1CNT_H, 1 << 15 | 1 << 10 | 1 << 9 | 3 << 12 | 2 << 5);
    gba.mem.write_u16(TM0CNT_L, 0xFE00);
    gba.mem.write_u16(TM0CNT_H, 0x80);
    gba.run_frame();
    let samples = gba.take_audio_samples();
    let distinct: std::collections::BTreeSet<i16> = samples.iter().copied().collect();
    assert!(distinct.len() > 8, "only {} distinct sample values", distinct.len());
    assert!(samples.iter().any(|sample| *sample < 0) && samples.iter().any(|sample| *sample > 0));
}
