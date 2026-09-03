mod common;

use common::*;
use gbae::system::gba::Gba;

#[test]
fn benchmark_rom_renders_and_sounds() {
    let mut gba = Gba::new(gbae::benchmark::rom());
    for _ in 0..120 {
        gba.run_frame();
    }
    let samples = gba.take_audio_samples();
    assert!(samples.iter().any(|sample| *sample != 0));
    assert_matches_golden(gba.framebuffer(), "benchmark_frame120");
}

#[test]
fn benchmark_rom_waits_for_vblank_every_frame() {
    let mut gba = Gba::new(gbae::benchmark::rom());
    let mut halted_frames = 0;
    for _ in 0..30 {
        assert!(gba.run_until(|gba| gba.scanline() % 228 == 150, 1_000_000));
        halted_frames += u32::from(gba.mem.io().halted);
        assert!(gba.run_until(|gba| gba.scanline() % 228 == 0, 1_000_000));
    }
    assert!(halted_frames >= 25, "halted before VBlank in {} frames of 30", halted_frames);
}
