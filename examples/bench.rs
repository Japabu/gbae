use gbae::system::gba::Gba;
use std::env;
use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: bench <rom> <frames>");
        std::process::exit(1);
    }
    let rom = fs::read(&args[1]).expect("Failed to read ROM");
    let frames: u64 = args[2].parse().expect("Failed to parse frame count");

    let mut gba = Gba::new(rom);
    let mut instructions = 0u64;
    let mut halted_steps = 0u64;
    let start = Instant::now();
    while gba.frame_count() < frames {
        if gba.mem.io().halted {
            halted_steps += 1;
        } else {
            instructions += 1;
        }
        gba.step();
    }
    let elapsed = start.elapsed();
    let seconds = elapsed.as_secs_f64();
    println!("frames: {}", frames);
    println!("wall time: {:.3}s", seconds);
    println!("emulated seconds: {:.2}", frames as f64 / 59.7275);
    println!("speed: {:.1}x realtime", frames as f64 / 59.7275 / seconds);
    println!("instructions: {}", instructions);
    println!("halted scanline skips: {}", halted_steps);
    println!("instructions per second: {:.1}M", instructions as f64 / seconds / 1e6);
}
