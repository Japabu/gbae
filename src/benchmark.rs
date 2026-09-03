use crate::system::instructions::asm::{registers::*, *};
use crate::system::instructions::Condition;

const ROM: u32 = 0x0800_0000;
const EWRAM: u32 = 0x0200_0000;
const IWRAM: u32 = 0x0300_0000;
const IO: u32 = 0x0400_0000;
const PALETTE: u32 = 0x0500_0000;
const VRAM: u32 = 0x0600_0000;
const OBJ_TILES: u32 = VRAM + 0x1_0000;
const SCREEN_MAP: u32 = VRAM + 0x4000;
const OAM: u32 = 0x0700_0000;
const IRQ_HANDLER: u32 = 0x0300_7FFC;
const IRQ_FLAGS: u32 = 0x0300_7FF8;

const HEADER_TITLE: usize = 0xA0;
const HEADER_END: usize = 0xC0;
const TITLE: &[u8; 12] = b"GBAE BENCH  ";

const WORKER: u32 = IWRAM;
const ARRAY_END: u32 = EWRAM + 0x4000;
const DATA_END: u32 = EWRAM + 0x1_0000;
const SAMPLES: u32 = EWRAM + 0x1_0000;
const SAMPLES_END: u32 = SAMPLES + 0x400;
const SPRITES: u32 = 32;
const WORK_ITERATIONS: u32 = 2048;

const VBLANK_INTR_WAIT: u32 = 0x05;
const CPU_FAST_SET: u32 = 0x0C;

pub fn rom() -> Vec<u8> {
    let worker = worker();
    let mut asm = Assembler::new(ROM);
    let start = asm.label();
    asm.b(start).pad_to(HEADER_TITLE);
    for chunk in TITLE.chunks(4) {
        asm.word(u32::from_le_bytes(chunk.try_into().unwrap()));
    }
    asm.pad_to(HEADER_END);
    let worker_source = asm.address();
    for chunk in worker.chunks(4) {
        asm.word(u32::from_le_bytes(chunk.try_into().unwrap()));
    }
    let handler = asm.address();
    irq_handler(&mut asm);
    asm.place(start);
    asm.ldr_literal(R0, IO + 0x204).ldr_literal(R1, 0x4317).emit(strh(R1, at(R0)));
    fill_palettes(&mut asm);
    fill_tiles(&mut asm);
    fill_screen_map(&mut asm);
    fill_sprites(&mut asm);
    fill_data(&mut asm);
    fill_samples(&mut asm);
    asm.ldr_literal(R0, worker_source)
        .ldr_literal(R1, WORKER)
        .emit(mov(R2, imm(worker.len() as u32 / 4)))
        .emit(swi(CPU_FAST_SET << 16));
    start_display(&mut asm);
    start_sound(&mut asm);
    asm.ldr_literal(R0, IRQ_HANDLER).ldr_literal(R1, handler).emit(str(R1, at(R0)));
    asm.ldr_literal(R0, IO + 0x200).emit(mov(R1, imm(1))).emit(strh(R1, at(R0))).emit(strh(R1, offset_address(R0, 8)));
    asm.emit(mov(R8, imm(0)));

    let frame = asm.here();
    asm.emit(add(R8, R8, imm(1)));
    scroll_and_cycle(&mut asm);
    move_sprites(&mut asm);
    animate_tiles(&mut asm);
    asm.ldr_literal(R0, WORKER | 1).emit(mov(LR, PC)).emit(bx(R0));
    arm_work(&mut asm);
    restart_sound_dma(&mut asm);
    asm.emit(swi(VBLANK_INTR_WAIT << 16)).b(frame);
    asm.pool();
    asm.finish()
}

fn worker() -> Vec<u8> {
    let mut asm = Assembler::new(WORKER);
    asm.thumb();
    asm.ldr_literal(R1, EWRAM).ldr_literal(R2, ARRAY_END).emit(movs(R0, imm(0)));
    let sum = asm.here();
    asm.emit(ldr(R3, at(R1)))
        .emit(muls(R3, R3, R3))
        .emit(adds(R0, R0, R3))
        .emit(adds(R1, R1, imm(4)))
        .emit(cmp(R1, R2))
        .b_if(Condition::NE, sum)
        .emit(bx(LR))
        .pool();
    let mut code = asm.finish();
    code.resize(code.len().next_multiple_of(32), 0);
    code
}

fn irq_handler(asm: &mut Assembler) {
    asm.ldr_literal(R0, IO + 0x202)
        .emit(ldrh(R1, at(R0)))
        .emit(strh(R1, at(R0)))
        .ldr_literal(R2, IRQ_FLAGS)
        .emit(ldrh(R3, at(R2)))
        .emit(orr(R3, R3, R1))
        .emit(strh(R3, at(R2)))
        .emit(bx(LR))
        .pool();
}

fn fill_palettes(asm: &mut Assembler) {
    asm.ldr_literal(R0, PALETTE).ldr_literal(R3, PALETTE + 0x400).emit(mov(R1, imm(0))).ldr_literal(R2, 0x0421);
    let entry = asm.here();
    asm.emit(strh(R1, post_increment(R0, 2))).emit(add(R1, R1, R2)).emit(cmp(R0, R3)).b_if(Condition::NE, entry);
}

fn fill_tiles(asm: &mut Assembler) {
    asm.ldr_literal(R0, VRAM).ldr_literal(R2, 0x1111_1110).emit(mov(R1, imm(0))).emit(mov(R5, imm(0)));
    let tile = asm.here();
    asm.emit(and(R3, R1, imm(0xF))).emit(mul(R4, R3, R2)).emit(str(R5, post_increment(R0, 4)));
    for _ in 1..8 {
        asm.emit(str(R4, post_increment(R0, 4)));
    }
    asm.emit(add(R1, R1, imm(1))).emit(cmp(R1, imm(512))).b_if(Condition::NE, tile);

    asm.ldr_literal(R0, OBJ_TILES).ldr_literal(R2, 0x1111_1111).emit(mov(R1, imm(0)));
    let sprite_tile = asm.here();
    asm.emit(and(R3, R1, imm(0xF))).emit(mul(R4, R3, R2));
    for _ in 0..8 {
        asm.emit(str(R4, post_increment(R0, 4)));
    }
    asm.emit(add(R1, R1, imm(1))).emit(cmp(R1, imm(256))).b_if(Condition::NE, sprite_tile);
}

fn fill_screen_map(asm: &mut Assembler) {
    asm.ldr_literal(R0, SCREEN_MAP).emit(mov(R1, imm(0)));
    let entry = asm.here();
    asm.emit(bic(R2, R1, imm(0xFE00)))
        .emit(mov(R3, lsr(R1, 5)))
        .emit(and(R3, R3, imm(0xF)))
        .emit(orr(R2, R2, lsl(R3, 12)))
        .emit(strh(R2, post_increment(R0, 2)))
        .emit(add(R1, R1, imm(1)))
        .emit(cmp(R1, imm(0x400)))
        .b_if(Condition::NE, entry);
}

fn fill_sprites(asm: &mut Assembler) {
    asm.ldr_literal(R0, OAM).emit(mov(R1, imm(0x200))).emit(mov(R2, imm(128)));
    let hide = asm.here();
    asm.emit(strh(R1, post_increment(R0, 8))).emit(subs(R2, R2, imm(1))).b_if(Condition::NE, hide);
    asm.ldr_literal(R0, OAM).emit(mov(R1, imm(0)));
    let sprite = asm.here();
    asm.emit(add(R2, R1, lsl(R1, 2)))
        .emit(and(R2, R2, imm(0xFF)))
        .emit(strh(R2, at(R0)))
        .emit(rsb(R2, R1, lsl(R1, 3)))
        .emit(bic(R2, R2, imm(0xFE00)))
        .emit(orr(R2, R2, imm(0x4000)))
        .emit(strh(R2, offset_address(R0, 2)))
        .emit(mov(R2, lsl(R1, 2)))
        .emit(and(R3, R1, imm(0xF)))
        .emit(orr(R2, R2, lsl(R3, 12)))
        .emit(strh(R2, offset_address(R0, 4)))
        .emit(add(R0, R0, imm(8)))
        .emit(add(R1, R1, imm(1)))
        .emit(cmp(R1, imm(SPRITES)))
        .b_if(Condition::NE, sprite);
}

fn fill_data(asm: &mut Assembler) {
    asm.ldr_literal(R0, EWRAM).ldr_literal(R3, DATA_END).emit(mov(R1, imm(1)));
    let word = asm.here();
    asm.emit(str(R1, post_increment(R0, 4)))
        .emit(add(R1, R1, lsl(R1, 3)))
        .emit(add(R1, R1, imm(1)))
        .emit(cmp(R0, R3))
        .b_if(Condition::NE, word);
}

fn fill_samples(asm: &mut Assembler) {
    asm.ldr_literal(R0, SAMPLES).ldr_literal(R3, SAMPLES_END).emit(mov(R1, imm(0)));
    let byte = asm.here();
    asm.emit(strb(R1, post_increment(R0, 1))).emit(add(R1, R1, imm(4))).emit(cmp(R0, R3)).b_if(Condition::NE, byte);
}

fn start_display(asm: &mut Assembler) {
    asm.ldr_literal(R0, IO)
        .ldr_literal(R1, 0x0800)
        .emit(strh(R1, offset_address(R0, 0x08)))
        .ldr_literal(R1, 0x1140)
        .emit(strh(R1, at(R0)))
        .emit(mov(R1, imm(0x08)))
        .emit(strh(R1, offset_address(R0, 0x04)));
}

fn start_sound(asm: &mut Assembler) {
    asm.ldr_literal(R0, IO)
        .emit(mov(R1, imm(0x80)))
        .emit(strh(R1, offset_address(R0, 0x84)))
        .ldr_literal(R1, 0xFF77)
        .emit(strh(R1, offset_address(R0, 0x80)))
        .ldr_literal(R1, 0x0B06)
        .emit(strh(R1, offset_address(R0, 0x82)))
        .ldr_literal(R1, 0xF080)
        .emit(strh(R1, offset_address(R0, 0x62)))
        .ldr_literal(R1, 0x8700)
        .emit(strh(R1, offset_address(R0, 0x64)))
        .ldr_literal(R1, SAMPLES)
        .emit(str(R1, offset_address(R0, 0xBC)))
        .ldr_literal(R1, IO + 0xA0)
        .emit(str(R1, offset_address(R0, 0xC0)))
        .ldr_literal(R0, IO + 0x100)
        .ldr_literal(R1, 0xFB1A)
        .emit(strh(R1, at(R0)))
        .emit(mov(R1, imm(0x80)))
        .emit(strh(R1, offset_address(R0, 2)));
    restart_sound_dma(asm);
}

fn restart_sound_dma(asm: &mut Assembler) {
    asm.ldr_literal(R0, IO)
        .emit(mov(R1, imm(0)))
        .emit(strh(R1, offset_address(R0, 0xC6)))
        .ldr_literal(R1, 0xB640)
        .emit(strh(R1, offset_address(R0, 0xC6)));
}

fn scroll_and_cycle(asm: &mut Assembler) {
    asm.ldr_literal(R0, IO)
        .emit(strh(R8, offset_address(R0, 0x10)))
        .emit(mov(R1, lsr(R8, 1)))
        .emit(strh(R1, offset_address(R0, 0x12)))
        .ldr_literal(R0, PALETTE)
        .ldr_literal(R2, 0x0421)
        .emit(mul(R1, R8, R2))
        .emit(strh(R1, offset_address(R0, 2)));
}

fn move_sprites(asm: &mut Assembler) {
    asm.ldr_literal(R0, OAM).emit(mov(R1, imm(SPRITES)));
    let sprite = asm.here();
    asm.emit(ldrh(R2, at(R0)))
        .emit(add(R2, R2, imm(1)))
        .emit(and(R2, R2, imm(0xFF)))
        .emit(strh(R2, post_increment(R0, 2)))
        .emit(ldrh(R2, at(R0)))
        .emit(add(R2, R2, imm(1)))
        .emit(bic(R2, R2, imm(0xFE00)))
        .emit(orr(R2, R2, imm(0x4000)))
        .emit(strh(R2, post_increment(R0, 6)))
        .emit(subs(R1, R1, imm(1)))
        .b_if(Condition::NE, sprite);
}

fn animate_tiles(asm: &mut Assembler) {
    asm.ldr_literal(R0, IO)
        .emit(and(R1, R8, imm(0x3F)))
        .ldr_literal(R2, EWRAM)
        .emit(add(R1, R2, lsl(R1, 10)))
        .emit(str(R1, offset_address(R0, 0xD4)))
        .ldr_literal(R1, VRAM)
        .emit(str(R1, offset_address(R0, 0xD8)))
        .emit(mov(R1, imm(0x100)))
        .emit(strh(R1, offset_address(R0, 0xDC)))
        .ldr_literal(R1, 0x8400)
        .emit(strh(R1, offset_address(R0, 0xDE)));
}

fn arm_work(asm: &mut Assembler) {
    asm.emit(mov(R1, imm(WORK_ITERATIONS)));
    let work = asm.here();
    asm.emit(push(registers([R4, R5, R6, R7])))
        .emit(add(R4, R4, lsl(R5, 2)))
        .emit(mul(R6, R7, R4))
        .emit(pop(registers([R4, R5, R6, R7])))
        .emit(subs(R1, R1, imm(1)))
        .b_if(Condition::NE, work);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::CartridgeInfo;

    #[test]
    fn test_rom_has_a_title_and_fits_the_header() {
        let rom = rom();
        assert_eq!(CartridgeInfo::parse(&rom).unwrap().title.trim(), "GBAE BENCH");
        assert!(rom.len() < 0x1000);
    }
}
