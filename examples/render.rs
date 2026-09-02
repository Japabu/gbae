use gbae::system::gba::Gba;
use gbae::system::ppu::{FRAMEBUFFER_HEIGHT, FRAMEBUFFER_WIDTH};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 {
        eprintln!("usage: render <bios> <rom> <frames> <out.png>");
        std::process::exit(1);
    }
    let bios = fs::read(&args[1]).expect("Failed to read BIOS");
    let rom = fs::read(&args[2]).expect("Failed to read ROM");
    let frames: u64 = args[3].parse().expect("Failed to parse frame count");

    let mut gba = Gba::new(bios, rom);
    for _ in 0..frames {
        gba.run_frame();
    }

    let mut image = image::RgbImage::new(FRAMEBUFFER_WIDTH as u32, FRAMEBUFFER_HEIGHT as u32);
    for (y, row) in gba.framebuffer().iter().enumerate() {
        for (x, pixel) in row.iter().enumerate() {
            image.put_pixel(x as u32, y as u32, image::Rgb(*pixel));
        }
    }
    image.save(&args[4]).expect("Failed to write image");
}
