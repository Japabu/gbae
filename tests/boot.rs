mod common;

use common::*;

const EMERALD_COPYRIGHT_FRAME_HASH: u64 = 0x7263_a4a5_0a43_3d4d;

#[test]
fn bios_boot_runs_30_frames() {
    let Some(mut gba) = gba_from_files() else {
        eprintln!("gba_bios.bin or rom.gba not found, skipping");
        return;
    };
    for _ in 0..30 {
        gba.run_frame();
    }
    assert_eq!(gba.frame_count(), 30);
}

#[test]
fn normmatt_bios_logo_matches_golden() {
    let (Some(bios), Some(rom)) = (read_project_file("gba_bios_normatt.bin"), read_project_file("rom.gba")) else {
        eprintln!("gba_bios_normatt.bin or rom.gba not found, skipping");
        return;
    };
    let mut gba = gbae::system::gba::Gba::new(bios, rom);
    for _ in 0..60 {
        gba.run_frame();
    }
    assert_matches_golden(gba.framebuffer(), "normmatt_logo_frame60");
}

#[test]
#[ignore = "runs about 56M instructions, use cargo test --release --test boot -- --ignored"]
fn emerald_copyright_screen_matches_recorded_hash() {
    let Some(mut gba) = gba_from_files() else {
        eprintln!("gba_bios.bin or rom.gba not found, skipping");
        return;
    };
    for _ in 0..400 {
        gba.run_frame();
    }
    assert_eq!(frame_hash(gba.framebuffer()), EMERALD_COPYRIGHT_FRAME_HASH, "copyright screen changed, frame hash {:#018x}", frame_hash(gba.framebuffer()));
}
