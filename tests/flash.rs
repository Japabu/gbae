mod common;

use common::*;
use gbae::system::gba::Gba;

const FLASH: u32 = 0x0E00_0000;
const COMMAND: u32 = FLASH + 0x5555;
const COMMAND2: u32 = FLASH + 0x2AAA;

fn command(gba: &mut Gba, value: u8) {
    gba.mem.write_u8(COMMAND, 0xAA);
    gba.mem.write_u8(COMMAND2, 0x55);
    gba.mem.write_u8(COMMAND, value);
}

fn program(gba: &mut Gba, address: u32, value: u8) {
    command(gba, 0xA0);
    gba.mem.write_u8(address, value);
}

#[test]
fn id_command_returns_macronix_128k_id() {
    let mut gba = gba_with_save_marker("FLASH1M_V");
    command(&mut gba, 0x90);
    assert_eq!(gba.mem.read_u8(FLASH), 0xC2);
    assert_eq!(gba.mem.read_u8(FLASH + 1), 0x09);
    command(&mut gba, 0xF0);
    assert_eq!(gba.mem.read_u8(FLASH), 0xFF);
}

#[test]
fn plain_write_does_not_change_flash() {
    let mut gba = gba_with_save_marker("FLASH1M_V");
    gba.mem.write_u8(FLASH + 0x10, 0x12);
    assert_eq!(gba.mem.read_u8(FLASH + 0x10), 0xFF);
}

#[test]
fn program_and_sector_erase() {
    let mut gba = gba_with_save_marker("FLASH1M_V");
    program(&mut gba, FLASH + 0x1234, 0x12);
    assert_eq!(gba.mem.read_u8(FLASH + 0x1234), 0x12);
    assert_eq!(gba.mem.read_u16(FLASH + 0x1234), 0x1212);
    command(&mut gba, 0x80);
    gba.mem.write_u8(COMMAND, 0xAA);
    gba.mem.write_u8(COMMAND2, 0x55);
    gba.mem.write_u8(FLASH + 0x1000, 0x30);
    assert_eq!(gba.mem.read_u8(FLASH + 0x1234), 0xFF);
}

#[test]
fn chip_erase_clears_everything() {
    let mut gba = gba_with_save_marker("FLASH1M_V");
    program(&mut gba, FLASH + 0x10, 0x00);
    command(&mut gba, 0x80);
    command(&mut gba, 0x10);
    assert_eq!(gba.mem.read_u8(FLASH + 0x10), 0xFF);
}

#[test]
fn bank_switch_selects_second_64k() {
    let mut gba = gba_with_save_marker("FLASH1M_V");
    program(&mut gba, FLASH + 0x10, 0x12);
    command(&mut gba, 0xB0);
    gba.mem.write_u8(FLASH, 1);
    assert_eq!(gba.mem.read_u8(FLASH + 0x10), 0xFF);
    program(&mut gba, FLASH + 0x10, 0x34);
    command(&mut gba, 0xB0);
    gba.mem.write_u8(FLASH, 0);
    assert_eq!(gba.mem.read_u8(FLASH + 0x10), 0x12);
}
