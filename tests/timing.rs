mod common;

use gbae::system::bios::Bios;
use gbae::system::cpu::Register;

use common::*;
use gbae::system::gba::Gba;

const IWRAM_CODE: u32 = 0x0300_0100;
const IWRAM_DATA: u32 = 0x0300_0200;
const EWRAM_CODE: u32 = 0x0200_0000;
const ROM_CODE: u32 = 0x0800_0000;
const WAITCNT: u32 = 0x0400_0204;
const DMA3SAD: u32 = 0x0400_00D4;
const DMA3DAD: u32 = 0x0400_00D8;
const DMA3CNT_L: u32 = 0x0400_00DC;
const DMA3CNT_H: u32 = 0x0400_00DE;

const ARM_NOP: u32 = 0xE1A0_0000;
const THUMB_NOP: u16 = 0x46C0;

fn machine(entry: u32, rom: Vec<u8>) -> Gba {
    let mut bios = vec![0; BIOS_LEN];
    bios[..4].copy_from_slice(&0xE59F_0000u32.to_le_bytes());
    bios[4..8].copy_from_slice(&0xE12F_FF10u32.to_le_bytes());
    bios[8..12].copy_from_slice(&entry.to_le_bytes());
    Gba::new(Bios::Image(bios), rom)
}

fn boot(entry: u32, rom: Vec<u8>) -> Gba {
    let mut gba = machine(entry, rom);
    run_to(&mut gba, entry);
    gba
}

fn boot_with_arm(entry: u32, code: &[u32]) -> Gba {
    let mut gba = machine(entry, vec![0; 0x100]);
    write_arm(&mut gba, entry, code);
    run_to(&mut gba, entry);
    gba
}

fn boot_with_thumb(entry: u32, code: &[u16]) -> Gba {
    let mut gba = machine(entry, vec![0; 0x100]);
    write_thumb(&mut gba, entry & !1, code);
    run_to(&mut gba, entry);
    gba
}

fn run_to(gba: &mut Gba, entry: u32) {
    assert!(gba.run_until(|gba| gba.cpu.pc() == entry & !1, 100));
    gba.mem.take_cycles();
}

fn write_arm(gba: &mut Gba, address: u32, code: &[u32]) {
    for (i, word) in code.iter().enumerate() {
        gba.mem.write_u32(address + i as u32 * 4, *word);
    }
}

fn write_thumb(gba: &mut Gba, address: u32, code: &[u16]) {
    for (i, halfword) in code.iter().enumerate() {
        gba.mem.write_u16(address + i as u32 * 2, *halfword);
    }
}

fn rom_with_arm(code: &[u32]) -> Vec<u8> {
    code.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn rom_with_thumb(code: &[u16]) -> Vec<u8> {
    code.iter().flat_map(|halfword| halfword.to_le_bytes()).collect()
}

fn step_cycles(gba: &mut Gba) -> u64 {
    let before = gba.cpu.cycles();
    gba.step();
    gba.cpu.cycles() - before
}

fn cycles_per_step(gba: &mut Gba, steps: usize) -> Vec<u64> {
    (0..steps).map(|_| step_cycles(gba)).collect()
}

#[test]
fn data_processing_in_iwram_takes_one_cycle() {
    let mut gba = boot_with_arm(IWRAM_CODE, &[ARM_NOP; 8]);
    assert_eq!(cycles_per_step(&mut gba, 4), [1, 1, 1, 1]);
}

#[test]
fn register_shift_adds_an_internal_cycle() {
    let mut gba = boot_with_arm(IWRAM_CODE, &[0xE1A0_0211, ARM_NOP, ARM_NOP]);
    assert_eq!(cycles_per_step(&mut gba, 2), [2, 1]);
}

#[test]
fn loads_and_stores_in_iwram() {
    let mut gba = boot_with_arm(IWRAM_CODE, &[0xE3A0_1403, 0xE281_1F80, 0xE591_0000, 0xE581_0000, 0xE891_003C, 0xE881_003C, ARM_NOP, ARM_NOP]);
    assert_eq!(cycles_per_step(&mut gba, 7), [1, 1, 3, 2, 6, 5, 1]);
    assert_eq!(gba.cpu.r(Register::R1), IWRAM_DATA);
}

#[test]
fn branch_refills_the_pipeline() {
    let mut gba = boot_with_arm(IWRAM_CODE, &[0xEA00_0000, ARM_NOP, ARM_NOP, ARM_NOP]);
    assert_eq!(cycles_per_step(&mut gba, 2), [3, 1]);
}

#[test]
fn multiply_cycles_depend_on_the_multiplier() {
    let mut gba = boot_with_arm(
        IWRAM_CODE,
        &[0xE3A0_2012, 0xE000_0291, 0xE3E0_2000, 0xE000_0291, 0xE3A0_2712, 0xE000_0291, 0xE3A0_2412, 0xE000_0291, ARM_NOP],
    );
    assert_eq!(cycles_per_step(&mut gba, 8), [1, 2, 1, 2, 1, 4, 1, 5]);
}

#[test]
fn thumb_code_in_ewram_pays_the_16_bit_bus() {
    let mut gba = boot_with_thumb(EWRAM_CODE | 1, &[THUMB_NOP; 8]);
    assert_eq!(cycles_per_step(&mut gba, 3), [3, 3, 3]);
}

#[test]
fn arm_code_in_rom_uses_wait_state_0() {
    let mut gba = boot(ROM_CODE, rom_with_arm(&[0xEA00_0000, ARM_NOP, ARM_NOP, ARM_NOP, ARM_NOP]));
    assert_eq!(cycles_per_step(&mut gba, 3), [20, 6, 6]);
}

#[test]
fn waitcnt_changes_rom_timing() {
    let mut gba = boot(ROM_CODE, rom_with_arm(&[ARM_NOP; 8]));
    gba.mem.write_u16(WAITCNT, 0x0014);
    gba.mem.take_cycles();
    assert_eq!(cycles_per_step(&mut gba, 2), [4, 4]);
}

#[test]
fn prefetch_buffer_fills_during_internal_cycles() {
    let code = [0x2203, 0x4350, THUMB_NOP, THUMB_NOP, THUMB_NOP, THUMB_NOP, THUMB_NOP, THUMB_NOP, THUMB_NOP, THUMB_NOP];
    let mut gba = boot(ROM_CODE | 1, rom_with_thumb(&code));
    assert_eq!(cycles_per_step(&mut gba, 4), [3, 7, 3, 3]);

    let mut gba = boot(ROM_CODE | 1, rom_with_thumb(&code));
    gba.mem.write_u16(WAITCNT, 0x4000);
    gba.mem.take_cycles();
    assert_eq!(cycles_per_step(&mut gba, 5), [5, 7, 1, 1, 3]);
}

#[test]
fn dma_stalls_the_cpu() {
    let mut gba = boot_with_arm(IWRAM_CODE, &[ARM_NOP; 4]);
    gba.mem.write_u32(DMA3SAD, IWRAM_DATA);
    gba.mem.write_u32(DMA3DAD, IWRAM_DATA + 0x100);
    gba.mem.write_u16(DMA3CNT_L, 16);
    gba.mem.write_u16(DMA3CNT_H, 1 << 15 | 1 << 10);
    assert_eq!(step_cycles(&mut gba), 1 + 2 + 2 + 30);
}
