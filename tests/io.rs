mod common;

use common::*;

const IE: u32 = 0x0400_0200;
const IF: u32 = 0x0400_0202;
const TM0CNT_L: u32 = 0x0400_0100;
const TM0CNT_H: u32 = 0x0400_0102;

fn raise_timer0_irq(gba: &mut gbae::system::gba::Gba) {
    gba.mem.write_u16(TM0CNT_L, 0xFFFE);
    gba.mem.write_u16(TM0CNT_H, 0xC0);
    gba.step();
    assert_eq!(gba.mem.read_u16(IF), 1 << 3);
}

#[test]
fn word_read_of_ie_returns_ie_and_if() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(IE, 0x0009);
    raise_timer0_irq(&mut gba);
    assert_eq!(gba.mem.read_u32(IE), 0x0008_0009);
}

#[test]
fn word_write_covers_two_registers() {
    let mut gba = gba_without_rom();
    gba.mem.write_u32(BG0CNT, 0x0002_0001);
    assert_eq!(gba.mem.read_u16(BG0CNT), 0x0001);
    assert_eq!(gba.mem.read_u16(BG1CNT), 0x0002);
}

#[test]
fn byte_access_selects_half_of_register() {
    let mut gba = gba_without_rom();
    gba.mem.write_u16(DISPCNT, 0x1234);
    assert_eq!(gba.mem.read_u8(DISPCNT), 0x34);
    assert_eq!(gba.mem.read_u8(DISPCNT + 1), 0x12);
    gba.mem.write_u8(DISPCNT + 1, 0x56);
    assert_eq!(gba.mem.read_u16(DISPCNT), 0x5634);
}

#[test]
fn byte_write_to_if_acknowledges_only_that_byte() {
    let mut gba = gba_without_rom();
    raise_timer0_irq(&mut gba);
    gba.mem.write_u8(IF + 1, 0xFF);
    assert_eq!(gba.mem.read_u16(IF), 1 << 3);
    gba.mem.write_u8(IF, 1 << 3);
    assert_eq!(gba.mem.read_u16(IF), 0);
}

#[test]
fn keyinput_reads_all_buttons_released() {
    let gba = gba_without_rom();
    assert_eq!(gba.mem.read_u16(0x0400_0130), 0x03FF);
}
