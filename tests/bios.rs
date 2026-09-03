mod common;

use common::*;
use gbae::system::bios::Bios;
use gbae::system::cpu::{Mode, Register};
use gbae::system::gba::Gba;
use gbae::system::instructions::asm::{registers::*, *};
use gbae::system::instructions::Condition;

const EWRAM: u32 = 0x0200_0000;
const IWRAM: u32 = 0x0300_0000;
const DONE: u32 = IWRAM + 0xF0;
const COUNTER: u32 = IWRAM + 0xF4;
const RESULTS: u32 = IWRAM + 0x100;
const IRQ_HANDLER: u32 = IWRAM + 0x7FFC;
const BIOS_INTERRUPT_FLAGS: u32 = IWRAM + 0x7FF8;
const DISPSTAT: u32 = 0x0400_0004;
const SOUNDBIAS: u32 = 0x0400_0088;
const IE: u32 = 0x0400_0200;
const IF: u32 = 0x0400_0202;
const IME: u32 = 0x0400_0208;

fn call(function: u32) -> gbae::system::instructions::Instruction {
    swi(function << 16)
}

fn program(build: impl FnOnce(&mut Assembler)) -> Gba {
    let mut asm = Assembler::new(ROM);
    build(&mut asm);
    asm.ldr_literal(R0, DONE).emit(mov(R1, imm(1))).emit(str(R1, at(R0)));
    let end = asm.here();
    asm.b(end);
    let mut rom = asm.finish();
    rom.resize(rom.len().max(0x100), 0);
    Gba::new(Bios::Builtin, rom)
}

fn run_until_done(gba: &mut Gba) {
    assert!(gba.run_until(|gba| gba.mem.read_u32(DONE) == 1, 5_000_000), "the program did not finish");
}

fn results(gba: &Gba, count: u32) -> Vec<u32> {
    (0..count).map(|index| gba.mem.read_u32(RESULTS + index * 4)).collect()
}

#[test]
fn boot_enters_the_rom_in_system_mode() {
    let mut gba = program(|_| {});
    run_until_done(&mut gba);
    assert_eq!(gba.cpu.mode(), Mode::System);
    assert!(!gba.cpu.cpsr().irq_disabled());
    assert_eq!(gba.cpu.r(Register::SP), 0x0300_7F00);
    assert_eq!(gba.cpu.r_in_mode(Register::SP, Mode::Irq), 0x0300_7FA0);
    assert_eq!(gba.cpu.r_in_mode(Register::SP, Mode::Supervisor), 0x0300_7FE0);
    assert_eq!(gba.mem.read_u16(SOUNDBIAS), 0x200);
    assert!(gba.cpu.pc() >= ROM);
}

#[test]
fn division_square_root_and_arc_tangent() {
    let mut gba = program(|asm| {
        asm.ldr_literal(R4, RESULTS);
        asm.ldr_literal(R0, 100u32.wrapping_neg()).emit(mov(R1, imm(7))).emit(call(0x06));
        asm.emit(stmia(R4, true, registers([R0, R1, R3])));
        asm.emit(mov(R1, imm(100))).emit(mov(R0, imm(9))).emit(call(0x07)).emit(str(R0, post_increment(R4, 4)));
        asm.ldr_literal(R0, 1_000_000).emit(call(0x08)).emit(str(R0, post_increment(R4, 4)));
        asm.ldr_literal(R0, 0x4000).emit(call(0x09)).emit(str(R0, post_increment(R4, 4)));
        asm.emit(mov(R0, imm(100))).emit(mov(R1, imm(100))).emit(call(0x0A)).emit(str(R0, post_increment(R4, 4)));
        asm.emit(call(0x0D)).emit(str(R0, post_increment(R4, 4)));
    });
    run_until_done(&mut gba);
    assert_eq!(results(&gba, 8), [(-14i32) as u32, (-2i32) as u32, 14, 11, 1000, 0x2000, 0x2000, 0xBAAE_187F]);
}

#[test]
fn cpu_set_copies_and_fills() {
    let mut gba = program(|asm| {
        asm.ldr_literal(R0, EWRAM).ldr_literal(R1, EWRAM + 0x1000).ldr_literal(R2, 16 | 1 << 26).emit(call(0x0B));
        asm.ldr_literal(R0, EWRAM + 0x40).ldr_literal(R1, EWRAM + 0x2000).ldr_literal(R2, 8 | 1 << 24).emit(call(0x0B));
        asm.ldr_literal(R0, EWRAM).ldr_literal(R1, EWRAM + 0x3000).ldr_literal(R2, 3).emit(call(0x0C));
    });
    for index in 0..16u32 {
        gba.mem.write_u32(EWRAM + index * 4, 0x1000 + index);
    }
    gba.mem.write_u16(EWRAM + 0x40, 0xBEEF);
    run_until_done(&mut gba);
    for index in 0..16u32 {
        assert_eq!(gba.mem.read_u32(EWRAM + 0x1000 + index * 4), 0x1000 + index);
    }
    for index in 0..8u32 {
        assert_eq!(gba.mem.read_u16(EWRAM + 0x2000 + index * 2), 0xBEEF);
    }
    for index in 0..8u32 {
        assert_eq!(gba.mem.read_u32(EWRAM + 0x3000 + index * 4), 0x1000 + index);
    }
    assert_eq!(gba.mem.read_u32(EWRAM + 0x3000 + 8 * 4), 0);
}

fn write_bytes(gba: &mut Gba, address: u32, bytes: &[u8]) {
    for (index, byte) in bytes.iter().enumerate() {
        gba.mem.write_u8(address + index as u32, *byte);
    }
}

fn read_bytes(gba: &Gba, address: u32, length: u32) -> Vec<u8> {
    (0..length).map(|index| gba.mem.read_u8(address + index)).collect()
}

#[test]
fn lz77_decompresses_to_wram_and_vram() {
    let source = EWRAM + 0x100;
    let mut gba = program(|asm| {
        asm.ldr_literal(R0, source).ldr_literal(R1, EWRAM + 0x200).emit(call(0x11));
        asm.ldr_literal(R0, source).ldr_literal(R1, VRAM).emit(call(0x12));
    });
    let mut compressed = (0x10u32 | 16 << 8).to_le_bytes().to_vec();
    compressed.push(0b0000_0000);
    compressed.extend_from_slice(b"ABCDEFGH");
    compressed.push(0b1000_0000);
    compressed.extend_from_slice(&[0x50, 0x07]);
    write_bytes(&mut gba, source, &compressed);
    run_until_done(&mut gba);
    assert_eq!(read_bytes(&gba, EWRAM + 0x200, 16), b"ABCDEFGHABCDEFGH");
    assert_eq!(read_bytes(&gba, VRAM, 16), b"ABCDEFGHABCDEFGH");
}

#[test]
fn run_length_and_difference_filters() {
    let source = EWRAM + 0x100;
    let mut gba = program(|asm| {
        asm.ldr_literal(R0, source).ldr_literal(R1, EWRAM + 0x200).emit(call(0x14));
        asm.ldr_literal(R0, source + 0x20).ldr_literal(R1, EWRAM + 0x300).emit(call(0x16));
        asm.ldr_literal(R0, source + 0x40).ldr_literal(R1, EWRAM + 0x400).emit(call(0x18));
    });
    let mut run_length = (0x30u32 | 10 << 8).to_le_bytes().to_vec();
    run_length.extend_from_slice(&[0x82, b'x', 0x04, b'1', b'2', b'3', b'4', b'5']);
    write_bytes(&mut gba, source, &run_length);
    let mut difference_8 = (0x81u32 | 4 << 8).to_le_bytes().to_vec();
    difference_8.extend_from_slice(&[1, 1, 1, 1]);
    write_bytes(&mut gba, source + 0x20, &difference_8);
    let mut difference_16 = (0x82u32 | 4 << 8).to_le_bytes().to_vec();
    difference_16.extend_from_slice(&[0x00, 0x01, 0x00, 0x01]);
    write_bytes(&mut gba, source + 0x40, &difference_16);
    run_until_done(&mut gba);
    assert_eq!(read_bytes(&gba, EWRAM + 0x200, 10), b"xxxxx12345");
    assert_eq!(read_bytes(&gba, EWRAM + 0x300, 4), [1, 2, 3, 4]);
    assert_eq!(gba.mem.read_u16(EWRAM + 0x400), 0x100);
    assert_eq!(gba.mem.read_u16(EWRAM + 0x402), 0x200);
}

#[test]
fn huffman_decompresses_with_a_two_symbol_tree() {
    let source = EWRAM + 0x100;
    let mut gba = program(|asm| {
        asm.ldr_literal(R0, source).ldr_literal(R1, EWRAM + 0x200).emit(call(0x13));
    });
    let mut compressed = (0x28u32 | 4 << 8).to_le_bytes().to_vec();
    compressed.extend_from_slice(&[1, 0xC0, b'A', b'B']);
    compressed.extend_from_slice(&0x6000_0000u32.to_le_bytes());
    write_bytes(&mut gba, source, &compressed);
    run_until_done(&mut gba);
    assert_eq!(read_bytes(&gba, EWRAM + 0x200, 4), b"ABBA");
}

#[test]
fn bit_unpack_widens_one_bit_pixels_to_nibbles() {
    let source = EWRAM + 0x100;
    let info = EWRAM + 0x180;
    let mut gba = program(|asm| {
        asm.ldr_literal(R0, source).ldr_literal(R1, EWRAM + 0x200).ldr_literal(R2, info).emit(call(0x10));
    });
    gba.mem.write_u8(source, 0b1010_1010);
    gba.mem.write_u16(info, 1);
    gba.mem.write_u8(info + 2, 1);
    gba.mem.write_u8(info + 3, 4);
    gba.mem.write_u32(info + 4, 0);
    run_until_done(&mut gba);
    assert_eq!(gba.mem.read_u32(EWRAM + 0x200), 0x1010_1010);
}

fn install_vblank_handler(asm: &mut Assembler, count_bios_flags: bool) {
    let handler = asm.label();
    let main = asm.label();
    asm.b(main);
    asm.place(handler);
    asm.ldr_literal(R0, IF).emit(mov(R1, imm(1))).emit(strh(R1, at(R0)));
    if count_bios_flags {
        asm.ldr_literal(R0, BIOS_INTERRUPT_FLAGS).emit(ldrh(R2, at(R0))).emit(orr(R2, R2, imm(1))).emit(strh(R2, at(R0)));
    }
    asm.ldr_literal(R0, COUNTER).emit(ldr(R1, at(R0))).emit(add(R1, R1, imm(1))).emit(str(R1, at(R0)));
    asm.emit(bx(LR));
    asm.place(main);
    asm.ldr_literal(R0, IRQ_HANDLER).ldr_literal(R1, ROM + 4).emit(str(R1, at(R0)));
    asm.ldr_literal(R0, DISPSTAT).emit(mov(R1, imm(8))).emit(strh(R1, at(R0)));
    asm.ldr_literal(R0, IE).emit(mov(R1, imm(1))).emit(strh(R1, at(R0)));
    asm.ldr_literal(R0, IME).emit(str(R1, at(R0)));
}

#[test]
fn irq_dispatches_through_the_bios_to_the_installed_handler() {
    let mut gba = program(|asm| {
        install_vblank_handler(asm, false);
        let idle = asm.here();
        asm.b(idle);
    });
    for _ in 0..3 {
        gba.run_frame();
    }
    assert_eq!(gba.mem.read_u32(COUNTER), 3);
    assert_eq!(gba.cpu.mode(), Mode::System);
}

#[test]
fn vblank_intr_wait_returns_once_per_frame() {
    let mut gba = program(|asm| {
        install_vblank_handler(asm, true);
        let wait = asm.here();
        asm.emit(call(0x05));
        asm.ldr_literal(R0, RESULTS).emit(ldr(R1, at(R0))).emit(add(R1, R1, imm(1))).emit(str(R1, at(R0)));
        asm.b(wait);
    });
    for _ in 0..4 {
        gba.run_frame();
    }
    assert_eq!(gba.mem.read_u32(COUNTER), 4);
    assert_eq!(gba.mem.read_u32(RESULTS), 4);
    assert_eq!(gba.mem.read_u16(BIOS_INTERRUPT_FLAGS), 0);
}

#[test]
fn register_ram_reset_clears_selected_regions() {
    let mut gba = program(|asm| {
        asm.emit(mov(R0, imm(0b0101))).emit(call(0x01));
    });
    gba.mem.write_u32(EWRAM + 0x100, 0x1234_5678);
    gba.mem.write_u32(IWRAM + 0x100, 0x1234_5678);
    gba.mem.write_u16(PALETTE, 0x7FFF);
    run_until_done(&mut gba);
    assert_eq!(gba.mem.read_u32(EWRAM + 0x100), 0);
    assert_eq!(gba.mem.read_u32(IWRAM + 0x100), 0x1234_5678);
    assert_eq!(gba.mem.read_u16(PALETTE), 0);
}

#[test]
fn soft_reset_restarts_the_rom() {
    let mut gba = program(|asm| {
        asm.ldr_literal(R0, COUNTER).emit(ldr(R1, at(R0))).emit(add(R1, R1, imm(1))).emit(str(R1, at(R0)));
        asm.emit(cmp(R1, imm(1))).emit_if(Condition::EQ, call(0x00));
    });
    run_until_done(&mut gba);
    assert_eq!(gba.mem.read_u32(COUNTER), 2);
    assert_eq!(gba.cpu.mode(), Mode::System);
    assert_eq!(gba.cpu.r(Register::SP), 0x0300_7F00);
}
