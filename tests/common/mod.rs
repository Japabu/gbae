#![allow(dead_code)]

use gbae::system::gba::Gba;
use gbae::system::ppu::{Framebuffer, FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use image::RgbImage;
use std::path::PathBuf;

pub const BIOS_LEN: usize = 0x4000;
pub const VRAM: u32 = 0x0600_0000;
pub const PALETTE: u32 = 0x0500_0000;
pub const DISPCNT: u32 = 0x0400_0000;
pub const BG0CNT: u32 = 0x0400_0008;
pub const BG1CNT: u32 = 0x0400_000A;

pub fn gba_without_rom() -> Gba {
    let mut bios = vec![0; BIOS_LEN];
    bios[..4].copy_from_slice(&0xEAFF_FFFEu32.to_le_bytes());
    Gba::new(bios, vec![0; 0x100])
}

pub fn gba_from_files() -> Option<Gba> {
    Some(Gba::new(read_project_file("gba_bios.bin")?, read_project_file("rom.gba")?))
}

pub fn read_project_file(name: &str) -> Option<Vec<u8>> {
    std::fs::read(project_dir().join(name)).ok()
}

pub fn project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn rgb555(r: u8, g: u8, b: u8) -> u16 {
    r as u16 | (g as u16) << 5 | (b as u16) << 10
}

pub fn rgb(r: u8, g: u8, b: u8) -> [u8; 3] {
    [r << 3, g << 3, b << 3]
}

pub fn to_image(framebuffer: &Framebuffer) -> RgbImage {
    let mut image = RgbImage::new(FRAMEBUFFER_WIDTH as u32, FRAMEBUFFER_HEIGHT as u32);
    for (y, row) in framebuffer.iter().enumerate() {
        for (x, pixel) in row.iter().enumerate() {
            image.put_pixel(x as u32, y as u32, image::Rgb(*pixel));
        }
    }
    image
}

pub fn frame_hash(framebuffer: &Framebuffer) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for row in framebuffer.iter() {
        for pixel in row.iter() {
            for byte in pixel {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }
    hash
}

pub fn assert_matches_golden(framebuffer: &Framebuffer, name: &str) {
    let actual = to_image(framebuffer);
    let golden_path = project_dir().join("tests").join("golden").join(format!("{}.png", name));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        actual.save(&golden_path).unwrap();
        return;
    }
    if !golden_path.exists() {
        panic!("missing golden image {}, run with UPDATE_GOLDEN=1 to create it", golden_path.display());
    }
    let expected = image::open(&golden_path).unwrap().to_rgb8();
    if expected.as_raw() == actual.as_raw() {
        return;
    }
    let actual_path = project_dir().join("target").join("golden_actual").join(format!("{}.png", name));
    std::fs::create_dir_all(actual_path.parent().unwrap()).unwrap();
    actual.save(&actual_path).unwrap();
    for y in 0..FRAMEBUFFER_HEIGHT as u32 {
        for x in 0..FRAMEBUFFER_WIDTH as u32 {
            if expected.get_pixel(x, y) != actual.get_pixel(x, y) {
                panic!(
                    "frame differs from {} at ({}, {}): expected {:?}, got {:?}, actual frame written to {}",
                    golden_path.display(),
                    x,
                    y,
                    expected.get_pixel(x, y).0,
                    actual.get_pixel(x, y).0,
                    actual_path.display()
                );
            }
        }
    }
}
