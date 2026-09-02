mod common;

use common::*;
use gbae::system::gba::Gba;

const IWRAM: u32 = 0x0300_0000;
const EWRAM: u32 = 0x0200_0000;

fn test_rom(directory: &str, name: &str) -> Option<Gba> {
    let bios = read_project_file("gba_bios.bin")?;
    let rom = std::fs::read(project_dir().join("tests").join("roms").join(directory).join(format!("{}.gba", name))).ok()?;
    Some(Gba::new(bios, rom))
}

fn jsmolka(name: &str) {
    let Some(mut gba) = test_rom("jsmolka", name) else {
        eprintln!("tests/roms/jsmolka/{}.gba or gba_bios.bin not found, skipping", name);
        return;
    };
    for _ in 0..400 {
        gba.run_frame();
    }
    let failed_test = gba.mem.read_u32(IWRAM) * 100 + gba.mem.read_u32(IWRAM + 4) * 10 + gba.mem.read_u32(IWRAM + 8);
    eprintln!("{}: IWRAM digits say test {} (only meaningful when the screen shows a failure)", name, failed_test);
    assert_matches_golden_as(gba.framebuffer(), "jsmolka_passed", &format!("jsmolka_{}", name));
}

fn fuzzarm(name: &str) {
    let Some(mut gba) = test_rom("fuzzarm", name) else {
        eprintln!("tests/roms/fuzzarm/{}.gba or gba_bios.bin not found, skipping", name);
        return;
    };
    let mut previous_pc = u32::MAX;
    let mut stalled_steps = 0;
    assert!(
        gba.run_until(
            |gba| {
                stalled_steps = if gba.cpu.pc() == previous_pc && !gba.mem.get_io_registers().halted { stalled_steps + 1 } else { 0 };
                previous_pc = gba.cpu.pc();
                stalled_steps == 1000
            },
            2_000_000_000
        ),
        "{} did not finish",
        name
    );
    gba.run_frame();
    let state = gba.mem.read_u32(EWRAM);
    if state == 0x4141_4141 || state == 0x5454_5454 {
        let mut dump = String::new();
        for word in 0..16 {
            dump.push_str(&format!("{:08X} ", gba.mem.read_u32(EWRAM + word * 4)));
        }
        let opcode: Vec<u8> = (4..12).map(|i| gba.mem.read_u8(EWRAM + i)).collect();
        panic!("{} failed: {} {}", name, String::from_utf8_lossy(&opcode), dump);
    }
    assert_matches_golden_as(gba.framebuffer(), "fuzzarm_passed", &format!("fuzzarm_{}", name));
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_arm() {
    jsmolka("arm");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_thumb() {
    jsmolka("thumb");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_memory() {
    jsmolka("memory");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_bios() {
    jsmolka("bios");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_nes() {
    jsmolka("nes");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_unsafe() {
    jsmolka("unsafe");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn fuzzarm_arm_data_processing() {
    fuzzarm("ARM_DataProcessing");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn fuzzarm_arm_any() {
    fuzzarm("ARM_Any");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn fuzzarm_thumb_data_processing() {
    fuzzarm("THUMB_DataProcessing");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn fuzzarm_thumb_any() {
    fuzzarm("THUMB_Any");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn fuzzarm_all() {
    fuzzarm("FuzzARM");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_flash64() {
    jsmolka("flash64");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_flash128() {
    jsmolka("flash128");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_sram() {
    jsmolka("sram");
}

#[test]
#[ignore = "needs tests/roms, run with cargo test --release --test roms -- --ignored"]
fn jsmolka_none() {
    jsmolka("none");
}
