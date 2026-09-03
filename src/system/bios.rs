use std::f64::consts::TAU;

use crate::bits::Bits;

use super::{
    cpu::{Mode, Psr, Register, CPU},
    instructions::asm::{registers::*, *},
    memory::{Memory, BIOS_LEN, OAM_LEN, PALETTE_RAM_LEN, VRAM_LEN, WRAM1_LEN, WRAM2_LEN},
};

const ROM_ENTRY: u32 = 0x0800_0000;
const EWRAM_ENTRY: u32 = 0x0200_0000;
const EWRAM: u32 = 0x0200_0000;
const IWRAM: u32 = 0x0300_0000;
const IO: u32 = 0x0400_0000;
const PALETTE: u32 = 0x0500_0000;
const VRAM: u32 = 0x0600_0000;
const OAM: u32 = 0x0700_0000;
const STACK_SUPERVISOR: u32 = 0x0300_7FE0;
const STACK_IRQ: u32 = 0x0300_7FA0;
const STACK_USER: u32 = 0x0300_7F00;
const RESET_FLAG: u32 = 0x0300_7FFA;
const INTERRUPT_FLAGS: u32 = 0x0300_7FF8;
const RESERVED_IWRAM: u32 = 0x200;
const DISPCNT: u32 = IO;
const SOUNDBIAS: u32 = IO + 0x88;
const IME: u32 = IO + 0x208;
const CHECKSUM: u32 = 0xBAAE_187F;
const SINE_ONE: i32 = 0x4000;

const CALL_CYCLES: u32 = 40;
const DIVISION_CYCLES: u32 = 90;
const SQUARE_ROOT_CYCLES: u32 = 100;
const ARC_TANGENT_CYCLES: u32 = 60;
const COPY_UNIT_CYCLES: u32 = 4;
const DECOMPRESS_BYTE_CYCLES: u32 = 6;
const AFFINE_ENTRY_CYCLES: u32 = 80;
const CLEAR_WORD_CYCLES: u32 = 2;

pub fn image() -> Vec<u8> {
    let mut asm = Assembler::new(0);
    let reset = asm.label();
    let hang = asm.label();
    let irq = asm.label();
    asm.b(reset).b(hang).b(hang).b(hang).b(hang).b(hang).b(irq).b(hang);
    asm.place(hang).b(hang);

    asm.place(irq)
        .emit(push(registers([R0, R1, R2, R3, R12, LR])))
        .emit(mov(R0, imm(IO)))
        .emit(add(LR, PC, imm(0)))
        .emit(ldr(PC, offset_address(R0, 4u32.wrapping_neg())))
        .emit(pop(registers([R0, R1, R2, R3, R12, LR])))
        .emit(subs_pc_lr(4));

    asm.place(reset);
    for (mode, stack) in [(Mode::Irq, STACK_IRQ), (Mode::Supervisor, STACK_SUPERVISOR), (Mode::System, STACK_USER)] {
        let irqs_disabled = mode != Mode::System;
        let control = Psr::from(0).with_mode(mode).with_irq_disabled(irqs_disabled).with_fiq_disabled(irqs_disabled).bits();
        asm.emit(mov(R0, imm(control))).emit(msr_cpsr_control(R0)).ldr_literal(SP, stack);
    }
    asm.emit(mov(R0, imm(IO))).emit(mov(R1, imm(0x200))).emit(strh(R1, offset_address(R0, SOUNDBIAS - IO)));
    for register in Register::all().filter(|register| *register != SP && *register != PC) {
        asm.emit(mov(register, imm(0)));
    }
    asm.emit(mov(PC, imm(ROM_ENTRY)));
    asm.pool().pad_to(BIOS_LEN);
    asm.finish()
}

pub fn call(function: u32, cpu: &mut CPU, mem: &mut Memory) {
    mem.idle(CALL_CYCLES);
    let argument = |register: Register| cpu.r(register);
    match function {
        0x00 => soft_reset(cpu, mem),
        0x01 => register_ram_reset(argument(R0), mem),
        0x02 => mem.io_mut().halted = true,
        0x03 | 0x27 => mem.io_mut().halted = true,
        0x04 => intr_wait(argument(R0) != 0, argument(R1), cpu, mem),
        0x05 => intr_wait(true, 1, cpu, mem),
        0x06 => divide(argument(R0), argument(R1), cpu, mem),
        0x07 => divide(argument(R1), argument(R0), cpu, mem),
        0x08 => {
            mem.idle(SQUARE_ROOT_CYCLES);
            cpu.set_r(R0, square_root(argument(R0)));
        }
        0x09 => {
            mem.idle(ARC_TANGENT_CYCLES);
            cpu.set_r(R0, arc_tangent(argument(R0)));
        }
        0x0A => {
            mem.idle(ARC_TANGENT_CYCLES);
            cpu.set_r(R0, arc_tangent_2(argument(R0), argument(R1)));
        }
        0x0B => cpu_set(argument(R0), argument(R1), argument(R2), mem),
        0x0C => cpu_fast_set(argument(R0), argument(R1), argument(R2), mem),
        0x0D => cpu.set_r(R0, CHECKSUM),
        0x0E => bg_affine_set(argument(R0), argument(R1), argument(R2), mem),
        0x0F => obj_affine_set(argument(R0), argument(R1), argument(R2), argument(R3), mem),
        0x10 => bit_unpack(argument(R0), argument(R1), argument(R2), mem),
        0x11 => decompress(argument(R0), argument(R1), Destination::Wram, lz77, mem),
        0x12 => decompress(argument(R0), argument(R1), Destination::Vram, lz77, mem),
        0x13 => decompress(argument(R0), argument(R1), Destination::Wram, huffman, mem),
        0x14 => decompress(argument(R0), argument(R1), Destination::Wram, run_length, mem),
        0x15 => decompress(argument(R0), argument(R1), Destination::Vram, run_length, mem),
        0x16 => decompress(argument(R0), argument(R1), Destination::Wram, difference_8, mem),
        0x17 => decompress(argument(R0), argument(R1), Destination::Vram, difference_8, mem),
        0x18 => decompress(argument(R0), argument(R1), Destination::Vram, difference_16, mem),
        0x19 => mem.write_u16(SOUNDBIAS, if argument(R0) == 0 { 0 } else { 0x200 }),
        0x1F => cpu.set_r(R0, midi_key_to_frequency(argument(R0), argument(R1), argument(R2), mem)),
        0x25 => cpu.set_r(R0, 1),
        0x26 => {
            mem.write_u8(RESET_FLAG, 0);
            soft_reset(cpu, mem);
        }
        _ => panic!("BIOS function {:#04X} is not implemented (called from {:#010X})", function, cpu.pc()),
    }
}

fn soft_reset(cpu: &mut CPU, mem: &mut Memory) {
    let entry = if mem.read_u8(RESET_FLAG) == 0 { ROM_ENTRY } else { EWRAM_ENTRY };
    clear(IWRAM + WRAM2_LEN as u32 - RESERVED_IWRAM, RESERVED_IWRAM, mem);
    for register in Register::all().filter(|register| *register != PC) {
        cpu.set_r(register, 0);
    }
    for (mode, stack) in [(Mode::Irq, STACK_IRQ), (Mode::Supervisor, STACK_SUPERVISOR), (Mode::System, STACK_USER)] {
        cpu.set_r_in_mode(SP, mode, stack);
        cpu.set_r_in_mode(LR, mode, 0);
    }
    cpu.set_cpsr(Psr::from(0).with_mode(Mode::System));
    cpu.set_r(PC, entry);
}

fn register_ram_reset(flags: u32, mem: &mut Memory) {
    if flags.bit(0) {
        clear(EWRAM, WRAM1_LEN as u32, mem);
    }
    if flags.bit(1) {
        clear(IWRAM, WRAM2_LEN as u32 - RESERVED_IWRAM, mem);
    }
    if flags.bit(2) {
        clear(PALETTE, PALETTE_RAM_LEN as u32, mem);
    }
    if flags.bit(3) {
        clear(VRAM, VRAM_LEN as u32, mem);
    }
    if flags.bit(4) {
        clear(OAM, OAM_LEN as u32, mem);
    }
    if flags.bit(5) {
        clear(IO + 0x120, 0x10, mem);
        clear(IO + 0x134, 0x10, mem);
    }
    if flags.bit(6) {
        mem.write_u16(IO + 0x84, 0);
        clear(IO + 0x60, 0x48, mem);
    }
    if flags.bit(7) {
        clear(IO, 0x60, mem);
        clear(IO + 0xB0, 0x60, mem);
        mem.write_u16(IO + 0x200, 0);
        mem.write_u16(IME, 0);
        mem.write_u16(DISPCNT, 0x80);
    }
}

fn clear(start: u32, length: u32, mem: &mut Memory) {
    for address in (start..start + length).step_by(4) {
        mem.write_u32(address, 0);
    }
    mem.idle(length / 4 * CLEAR_WORD_CYCLES);
}

fn intr_wait(discard_old: bool, mask: u32, cpu: &mut CPU, mem: &mut Memory) {
    let mask = mask as u16;
    mem.write_u16(IME, 1);
    if !mem.intr_waiting() && discard_old {
        let flags = mem.read_u16(INTERRUPT_FLAGS);
        mem.write_u16(INTERRUPT_FLAGS, flags & !mask);
    }
    let flags = mem.read_u16(INTERRUPT_FLAGS);
    if flags & mask != 0 {
        mem.write_u16(INTERRUPT_FLAGS, flags & !mask);
        mem.set_intr_waiting(false);
    } else {
        mem.set_intr_waiting(true);
        mem.io_mut().halted = true;
        cpu.set_r(PC, cpu.pc());
    }
}

fn divide(numerator: u32, denominator: u32, cpu: &mut CPU, mem: &mut Memory) {
    mem.idle(DIVISION_CYCLES);
    let (numerator, denominator) = (numerator as i32, denominator as i32);
    let (quotient, remainder) = if denominator == 0 {
        (numerator.signum(), numerator)
    } else {
        (numerator.wrapping_div(denominator), numerator.wrapping_rem(denominator))
    };
    cpu.set_r(R0, quotient as u32);
    cpu.set_r(R1, remainder as u32);
    cpu.set_r(R3, quotient.wrapping_abs() as u32);
}

fn square_root(value: u32) -> u32 {
    let mut root = (f64::from(value).sqrt()) as u32;
    while u64::from(root) * u64::from(root) > u64::from(value) {
        root -= 1;
    }
    while u64::from(root + 1) * u64::from(root + 1) <= u64::from(value) {
        root += 1;
    }
    root
}

fn arc_tangent(tangent: u32) -> u32 {
    let tangent = tangent.sign_extended(16) as i32;
    let square = -((tangent * tangent) >> 14);
    let series = [0xA9, 0x390, 0x91C, 0xFB6, 0x16AA, 0x2081, 0x3651, 0xA2F9];
    let polynomial = series[1..].iter().fold(series[0], |acc, term| ((acc * square) >> 14) + term);
    ((tangent * polynomial) >> 16) as u32 & 0xFFFF
}

fn arc_tangent_2(x: u32, y: u32) -> u32 {
    let (x, y) = (x.sign_extended(16) as i32, y.sign_extended(16) as i32);
    let angle = match (x, y) {
        (_, 0) if x >= 0 => 0,
        (_, 0) => 0x8000,
        (0, _) if y >= 0 => 0x4000,
        (0, _) => 0xC000,
        _ if y.abs() <= x.abs() => {
            let tangent = arc_tangent(((y << 14) / x) as u32).sign_extended(16) as i32;
            if x < 0 {
                tangent + 0x8000
            } else {
                tangent
            }
        }
        _ => {
            let tangent = arc_tangent(((x << 14) / y) as u32).sign_extended(16) as i32;
            if y < 0 {
                0xC000 - tangent
            } else {
                0x4000 - tangent
            }
        }
    };
    angle as u32 & 0xFFFF
}

fn cpu_set(source: u32, destination: u32, control: u32, mem: &mut Memory) {
    let count = control.bits(0..21);
    let fill = control.bit(24);
    let words = control.bit(26);
    mem.idle(count * COPY_UNIT_CYCLES);
    for index in 0..count {
        if words {
            let value = mem.read_u32(if fill { source } else { source + index * 4 });
            mem.write_u32(destination + index * 4, value);
        } else {
            let value = mem.read_u16(if fill { source } else { source + index * 2 });
            mem.write_u16(destination + index * 2, value);
        }
    }
}

fn cpu_fast_set(source: u32, destination: u32, control: u32, mem: &mut Memory) {
    let count = control.bits(0..21).div_ceil(8) * 8;
    let fill = control.bit(24);
    mem.idle(count * COPY_UNIT_CYCLES / 2);
    for index in 0..count {
        let value = mem.read_u32(if fill { source } else { source + index * 4 });
        mem.write_u32(destination + index * 4, value);
    }
}

fn sine(angle: u32) -> (i32, i32) {
    let radians = f64::from(angle) * TAU / 256.0;
    let scaled = |value: f64| (value * f64::from(SINE_ONE)).round() as i32;
    (scaled(radians.sin()), scaled(radians.cos()))
}

fn halfword(value: i32) -> u16 {
    value as u16
}

fn signed_halfword(mem: &Memory, address: u32) -> i32 {
    u32::from(mem.read_u16(address)).sign_extended(16) as i32
}

fn bg_affine_set(source: u32, destination: u32, count: u32, mem: &mut Memory) {
    mem.idle(count * AFFINE_ENTRY_CYCLES);
    for index in 0..count {
        let source = source + index * 20;
        let destination = destination + index * 16;
        let origin_x = mem.read_u32(source) as i32;
        let origin_y = mem.read_u32(source + 4) as i32;
        let display_x = signed_halfword(mem, source + 8);
        let display_y = signed_halfword(mem, source + 10);
        let scale_x = signed_halfword(mem, source + 12);
        let scale_y = signed_halfword(mem, source + 14);
        let (sin, cos) = sine(u32::from(mem.read_u16(source + 16)).bits(8..16));
        let pa = (scale_x * cos) >> 14;
        let pb = -((scale_x * sin) >> 14);
        let pc = (scale_y * sin) >> 14;
        let pd = (scale_y * cos) >> 14;
        mem.write_u16(destination, halfword(pa));
        mem.write_u16(destination + 2, halfword(pb));
        mem.write_u16(destination + 4, halfword(pc));
        mem.write_u16(destination + 6, halfword(pd));
        mem.write_u32(destination + 8, origin_x.wrapping_sub(pa * display_x + pb * display_y) as u32);
        mem.write_u32(destination + 12, origin_y.wrapping_sub(pc * display_x + pd * display_y) as u32);
    }
}

fn obj_affine_set(source: u32, destination: u32, count: u32, stride: u32, mem: &mut Memory) {
    mem.idle(count * AFFINE_ENTRY_CYCLES / 2);
    for index in 0..count {
        let source = source + index * 8;
        let destination = destination + index * stride * 4;
        let scale_x = signed_halfword(mem, source);
        let scale_y = signed_halfword(mem, source + 2);
        let (sin, cos) = sine(u32::from(mem.read_u16(source + 4)).bits(8..16));
        let parameters = [(scale_x * cos) >> 14, -((scale_x * sin) >> 14), (scale_y * sin) >> 14, (scale_y * cos) >> 14];
        for (slot, parameter) in parameters.into_iter().enumerate() {
            mem.write_u16(destination + slot as u32 * stride, halfword(parameter));
        }
    }
}

fn bit_unpack(source: u32, destination: u32, info: u32, mem: &mut Memory) {
    let length = u32::from(mem.read_u16(info));
    let source_width = u32::from(mem.read_u8(info + 2));
    let destination_width = u32::from(mem.read_u8(info + 3));
    let offset_word = mem.read_u32(info + 4);
    let offset = offset_word.bits(0..31);
    let offset_zeros = offset_word.bit(31);
    mem.idle(length * DECOMPRESS_BYTE_CYCLES);
    let mut output = 0u32;
    let mut output_bits = 0;
    let mut written = destination;
    for byte_index in 0..length {
        let byte = u32::from(mem.read_u8(source + byte_index));
        for unit in 0..8 / source_width {
            let mut value = byte >> (unit * source_width) & ((1 << source_width) - 1);
            if value != 0 || offset_zeros {
                value = value.wrapping_add(offset);
            }
            output |= value << output_bits;
            output_bits += destination_width;
            if output_bits >= 32 {
                mem.write_u32(written, output);
                written += 4;
                output = 0;
                output_bits = 0;
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Wram,
    Vram,
}

struct Reader<'a> {
    mem: &'a Memory,
    address: u32,
}

impl Reader<'_> {
    fn byte(&mut self) -> u8 {
        let byte = self.mem.read_u8(self.address);
        self.address += 1;
        byte
    }

    fn word(&mut self) -> u32 {
        let word = self.mem.read_u32(self.address);
        self.address += 4;
        word
    }
}

fn decompress(source: u32, destination: u32, target: Destination, algorithm: fn(&mut Reader, usize) -> Vec<u8>, mem: &mut Memory) {
    let header = mem.read_u32(source);
    let length = header.bits(8..32) as usize;
    let mut reader = Reader { mem, address: source };
    let output = algorithm(&mut reader, length);
    mem.idle(length as u32 * DECOMPRESS_BYTE_CYCLES);
    match target {
        Destination::Wram => {
            for (index, byte) in output.iter().enumerate() {
                mem.write_u8(destination + index as u32, *byte);
            }
        }
        Destination::Vram => {
            for (index, pair) in output.chunks(2).enumerate() {
                let low = pair[0];
                let high = pair.get(1).copied().unwrap_or_else(|| mem.read_u8(destination + index as u32 * 2 + 1));
                mem.write_u16(destination + index as u32 * 2, u16::from_le_bytes([low, high]));
            }
        }
    }
}

fn lz77(reader: &mut Reader, length: usize) -> Vec<u8> {
    reader.word();
    let mut output = Vec::with_capacity(length);
    while output.len() < length {
        let flags = reader.byte();
        for block in (0..8).rev() {
            if output.len() >= length {
                break;
            }
            if flags.bit(block) {
                let first = reader.byte();
                let second = reader.byte();
                let run = usize::from(first >> 4) + 3;
                let displacement = (usize::from(first & 0xF) << 8 | usize::from(second)) + 1;
                for _ in 0..run {
                    let byte = output[output.len() - displacement];
                    output.push(byte);
                }
            } else {
                output.push(reader.byte());
            }
        }
    }
    output.truncate(length);
    output
}

fn run_length(reader: &mut Reader, length: usize) -> Vec<u8> {
    reader.word();
    let mut output = Vec::with_capacity(length);
    while output.len() < length {
        let flag = reader.byte();
        if flag.bit(7) {
            let byte = reader.byte();
            output.extend(std::iter::repeat_n(byte, usize::from(flag & 0x7F) + 3));
        } else {
            output.extend((0..=flag & 0x7F).map(|_| reader.byte()));
        }
    }
    output.truncate(length);
    output
}

fn huffman(reader: &mut Reader, length: usize) -> Vec<u8> {
    let header = reader.word();
    let symbol_bits = header.bits(0..4) as usize;
    let tree_start = reader.address;
    let tree_size = (usize::from(reader.byte()) + 1) * 2;
    reader.address = tree_start + tree_size as u32;
    let mut symbols = Vec::with_capacity(length * 8 / symbol_bits);
    let mut node = tree_start + 1;
    while symbols.len() * symbol_bits < length * 8 {
        let word = reader.word();
        for bit in (0..32).rev() {
            let entry = reader.mem.read_u8(node);
            let branch = word.bit(bit);
            let offset = u32::from(entry & 0x3F);
            let leaf = entry.bit(if branch { 6 } else { 7 });
            node = (node & !1) + offset * 2 + 2 + u32::from(branch);
            if leaf {
                symbols.push(reader.mem.read_u8(node));
                node = tree_start + 1;
                if symbols.len() * symbol_bits >= length * 8 {
                    break;
                }
            }
        }
    }
    let mut output = Vec::with_capacity(length);
    if symbol_bits == 8 {
        output.extend(symbols);
    } else {
        for pair in symbols.chunks(2) {
            output.push(pair[0] | pair.get(1).copied().unwrap_or(0) << 4);
        }
    }
    output.truncate(length);
    output
}

fn difference_8(reader: &mut Reader, length: usize) -> Vec<u8> {
    reader.word();
    let mut output = Vec::with_capacity(length);
    let mut sum = 0u8;
    for _ in 0..length {
        sum = sum.wrapping_add(reader.byte());
        output.push(sum);
    }
    output
}

fn difference_16(reader: &mut Reader, length: usize) -> Vec<u8> {
    reader.word();
    let mut output = Vec::with_capacity(length);
    let mut sum = 0u16;
    for _ in 0..length / 2 {
        sum = sum.wrapping_add(u16::from_le_bytes([reader.byte(), reader.byte()]));
        output.extend_from_slice(&sum.to_le_bytes());
    }
    output
}

fn midi_key_to_frequency(wave: u32, key: u32, fine: u32, mem: &Memory) -> u32 {
    let base = f64::from(mem.read_u32(wave + 4));
    let semitones = f64::from(key as i32 - 60) + f64::from(fine) / 256.0;
    (base * (semitones / 12.0).exp2()) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::instructions::{Condition, Instruction};

    #[test]
    fn test_image_has_vectors_and_size() {
        let image = image();
        assert_eq!(image.len(), BIOS_LEN);
        let word = |offset: usize| u32::from_le_bytes(image[offset..offset + 4].try_into().unwrap());
        for vector in [0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18, 0x1C] {
            assert!(matches!(Instruction::decode_arm(word(vector)), Instruction::Branch(_)), "vector {:02X}", vector);
        }
        let irq_target = Instruction::decode_arm(word(0x18)).disassemble(Condition::AL, 0x18);
        assert_eq!(irq_target, "B #00000024");
        assert_eq!(Instruction::decode_arm(word(0x24)).disassemble(Condition::AL, 0x24), "STMDB R13!, {R0, R1, R2, R3, R12, R14}");
        assert_eq!(Instruction::decode_arm(word(0x30)).disassemble(Condition::AL, 0x30), "LDR R15, [R0, #-0x4]");
        assert_eq!(Instruction::decode_arm(word(0x38)).disassemble(Condition::AL, 0x38), "SUBS R15, R14, #0x4");
    }

    #[test]
    fn test_square_root() {
        assert_eq!(square_root(0), 0);
        assert_eq!(square_root(1), 1);
        assert_eq!(square_root(15), 3);
        assert_eq!(square_root(16), 4);
        assert_eq!(square_root(1_000_000), 1000);
        assert_eq!(square_root(u32::MAX), 65535);
    }

    #[test]
    fn test_arc_tangent_matches_the_hardware_polynomial() {
        assert_eq!(arc_tangent(0), 0);
        assert_eq!(arc_tangent(0x4000), 0x2000);
        assert_eq!(arc_tangent(0xC000), 0xE000);
        assert_eq!(arc_tangent(0x2000), 0x12E4);
    }

    #[test]
    fn test_arc_tangent_2_covers_the_circle() {
        assert_eq!(arc_tangent_2(1, 0), 0);
        assert_eq!(arc_tangent_2(0, 1), 0x4000);
        assert_eq!(arc_tangent_2(0xFFFF, 0), 0x8000);
        assert_eq!(arc_tangent_2(0, 0xFFFF), 0xC000);
        assert_eq!(arc_tangent_2(100, 100), 0x2000);
        assert_eq!(arc_tangent_2(0xFF9C, 100), 0x6000);
        assert_eq!(arc_tangent_2(100, 0xFF9C), 0xE000);
        assert_eq!(arc_tangent_2(0xFF9C, 0xFF9C), 0xA000);
        assert_eq!(arc_tangent_2(30, 100), 0x3421);
    }

    #[test]
    fn test_sine_table_endpoints() {
        assert_eq!(sine(0), (0, SINE_ONE));
        assert_eq!(sine(64), (SINE_ONE, 0));
        assert_eq!(sine(128), (0, -SINE_ONE));
    }
}
