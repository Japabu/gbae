mod common;

use common::*;
use gbae::system::bios::Bios;
use gbae::system::cpu::Register;
use gbae::system::gba::Gba;
use gbae::system::instructions::asm::{registers::*, *};
use gbae::system::instructions::Instruction;

const IWRAM_CODE: u32 = 0x0300_0100;
const IWRAM_DATA: u32 = 0x0300_0200;
const EWRAM_CODE: u32 = 0x0200_0000;
const ROM_CODE: u32 = 0x0800_0000;
const WAITCNT: u32 = 0x0400_0204;
const DMA3SAD: u32 = 0x0400_00D4;
const DMA3DAD: u32 = 0x0400_00D8;
const DMA3CNT_L: u32 = 0x0400_00DC;
const DMA3CNT_H: u32 = 0x0400_00DE;

fn nop() -> Instruction {
    mov(R0, R0)
}

fn thumb_nop() -> Instruction {
    mov(R8, R8)
}

fn assemble(base: u32, thumb: bool, build: impl FnOnce(&mut Assembler)) -> Vec<u8> {
    let mut asm = Assembler::new(base & !1);
    if thumb {
        asm.thumb();
    }
    build(&mut asm);
    asm.finish()
}

fn nops(asm: &mut Assembler, instruction: Instruction, count: usize) {
    for _ in 0..count {
        asm.emit(instruction);
    }
}

fn machine(entry: u32, rom: Vec<u8>) -> Gba {
    let mut asm = Assembler::new(0);
    asm.ldr_literal(R0, entry).emit(bx(R0)).pool().pad_to(BIOS_LEN);
    Gba::new(Bios::Image(asm.finish()), rom)
}

fn boot(entry: u32, rom: Vec<u8>) -> Gba {
    let mut gba = machine(entry, rom);
    run_to(&mut gba, entry);
    gba
}

fn boot_in_ram(entry: u32, code: &[u8]) -> Gba {
    let mut gba = machine(entry, vec![0; 0x100]);
    for (offset, byte) in code.iter().enumerate() {
        gba.mem.write_u8((entry & !1) + offset as u32, *byte);
    }
    run_to(&mut gba, entry);
    gba
}

fn boot_with_arm(entry: u32, build: impl FnOnce(&mut Assembler)) -> Gba {
    boot_in_ram(entry, &assemble(entry, false, build))
}

fn boot_with_thumb(entry: u32, build: impl FnOnce(&mut Assembler)) -> Gba {
    boot_in_ram(entry, &assemble(entry, true, build))
}

fn run_to(gba: &mut Gba, entry: u32) {
    assert!(gba.run_until(|gba| gba.cpu.pc() == entry & !1, 100));
    gba.mem.take_cycles();
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
    let mut gba = boot_with_arm(IWRAM_CODE, |asm| nops(asm, nop(), 8));
    assert_eq!(cycles_per_step(&mut gba, 4), [1, 1, 1, 1]);
}

#[test]
fn register_shift_adds_an_internal_cycle() {
    let mut gba = boot_with_arm(IWRAM_CODE, |asm| {
        asm.emit(mov(R0, lsl_by(R1, R2)));
        nops(asm, nop(), 2);
    });
    assert_eq!(cycles_per_step(&mut gba, 2), [2, 1]);
}

#[test]
fn loads_and_stores_in_iwram() {
    let mut gba = boot_with_arm(IWRAM_CODE, |asm| {
        asm.emit(mov(R1, imm(0x0300_0000)))
            .emit(add(R1, R1, imm(0x200)))
            .emit(ldr(R0, at(R1)))
            .emit(str(R0, at(R1)))
            .emit(ldmia(R1, false, registers([R2, R3, R4, R5])))
            .emit(stmia(R1, false, registers([R2, R3, R4, R5])));
        nops(asm, nop(), 2);
    });
    assert_eq!(cycles_per_step(&mut gba, 7), [1, 1, 3, 2, 6, 5, 1]);
    assert_eq!(gba.cpu.r(Register::R1), IWRAM_DATA);
}

#[test]
fn branch_refills_the_pipeline() {
    let mut gba = boot_with_arm(IWRAM_CODE, |asm| {
        let target = asm.label();
        asm.b(target).emit(nop()).place(target);
        nops(asm, nop(), 2);
    });
    assert_eq!(cycles_per_step(&mut gba, 2), [3, 1]);
}

#[test]
fn multiply_cycles_depend_on_the_multiplier() {
    let mut gba = boot_with_arm(IWRAM_CODE, |asm| {
        for multiplier in [mov(R2, imm(0x12)), mvn(R2, imm(0)), mov(R2, imm(0x0048_0000)), mov(R2, imm(0x1200_0000))] {
            asm.emit(multiplier).emit(mul(R0, R1, R2));
        }
        asm.emit(nop());
    });
    assert_eq!(cycles_per_step(&mut gba, 8), [1, 2, 1, 2, 1, 4, 1, 5]);
}

#[test]
fn thumb_code_in_ewram_pays_the_16_bit_bus() {
    let mut gba = boot_with_thumb(EWRAM_CODE | 1, |asm| nops(asm, thumb_nop(), 8));
    assert_eq!(cycles_per_step(&mut gba, 3), [3, 3, 3]);
}

#[test]
fn arm_code_in_rom_uses_wait_state_0() {
    let rom = assemble(ROM_CODE, false, |asm| {
        let target = asm.label();
        asm.b(target).emit(nop()).place(target);
        nops(asm, nop(), 3);
    });
    let mut gba = boot(ROM_CODE, rom);
    assert_eq!(cycles_per_step(&mut gba, 3), [20, 6, 6]);
}

#[test]
fn waitcnt_changes_rom_timing() {
    let mut gba = boot(ROM_CODE, assemble(ROM_CODE, false, |asm| nops(asm, nop(), 8)));
    gba.mem.write_u16(WAITCNT, 0x0014);
    gba.mem.take_cycles();
    assert_eq!(cycles_per_step(&mut gba, 2), [4, 4]);
}

#[test]
fn prefetch_buffer_fills_during_internal_cycles() {
    let rom = || {
        assemble(ROM_CODE, true, |asm| {
            asm.emit(movs(R2, imm(3))).emit(muls(R0, R2, R0));
            nops(asm, thumb_nop(), 8);
        })
    };
    let mut gba = boot(ROM_CODE | 1, rom());
    assert_eq!(cycles_per_step(&mut gba, 4), [3, 7, 3, 3]);

    let mut gba = boot(ROM_CODE | 1, rom());
    gba.mem.write_u16(WAITCNT, 0x4000);
    gba.mem.take_cycles();
    assert_eq!(cycles_per_step(&mut gba, 5), [5, 7, 1, 1, 3]);
}

#[test]
fn dma_stalls_the_cpu() {
    let mut gba = boot_with_arm(IWRAM_CODE, |asm| nops(asm, nop(), 4));
    gba.mem.write_u32(DMA3SAD, IWRAM_DATA);
    gba.mem.write_u32(DMA3DAD, IWRAM_DATA + 0x100);
    gba.mem.write_u16(DMA3CNT_L, 16);
    gba.mem.write_u16(DMA3CNT_H, 1 << 15 | 1 << 10);
    assert_eq!(step_cycles(&mut gba), 1 + 2 + 2 + 30);
}
