mod common;

use common::*;
use gbae::system::save::SaveType;

const SRAM: u32 = 0x0E00_0000;
const EEPROM: u32 = 0x0D00_0000;
const EWRAM: u32 = 0x0200_0000;
const DMA3SAD: u32 = 0x0400_00D4;
const DMA3DAD: u32 = 0x0400_00D8;
const DMA3CNT_L: u32 = 0x0400_00DC;
const DMA3CNT_H: u32 = 0x0400_00DE;
const DMA_ENABLE: u16 = 1 << 15;

#[test]
fn save_type_comes_from_the_rom() {
    assert_eq!(gba_without_rom().save_type(), SaveType::None);
    assert_eq!(gba_with_save_marker("SRAM_V").save_type(), SaveType::Sram);
    assert_eq!(gba_with_save_marker("FLASH_V").save_type(), SaveType::Flash64K);
    assert_eq!(gba_with_save_marker("FLASH1M_V").save_type(), SaveType::Flash128K);
    assert_eq!(gba_with_save_marker("EEPROM_V").save_type(), SaveType::Eeprom);
}

#[test]
fn no_save_type_reads_ff() {
    let mut gba = gba_without_rom();
    gba.mem.write_u8(SRAM, 0x12);
    assert_eq!(gba.mem.read_u8(SRAM), 0xFF);
    assert_eq!(gba.mem.read_u8(SRAM + 0x0100_0000), 0xFF);
    assert!(gba.save_data().is_empty());
}

#[test]
fn sram_is_32k_and_mirrored() {
    let mut gba = gba_with_save_marker("SRAM_V");
    assert_eq!(gba.mem.read_u8(SRAM), 0xFF);
    gba.mem.write_u8(SRAM + 0x10, 0x12);
    assert_eq!(gba.mem.read_u8(SRAM + 0x8010), 0x12);
    assert_eq!(gba.mem.read_u8(SRAM + 0x1_0010), 0x12);
    assert_eq!(gba.mem.read_u8(SRAM + 0x0100_0010), 0x12);
    assert_eq!(gba.mem.read_u16(SRAM + 0x10), 0x1212);
    assert_eq!(gba.mem.read_u32(SRAM + 0x10), 0x1212_1212);
    assert!(gba.take_save_dirty());
    assert!(!gba.take_save_dirty());
}

#[test]
fn wide_stores_write_only_the_addressed_byte() {
    let mut gba = gba_with_save_marker("SRAM_V");
    gba.mem.write_u16(SRAM + 0x21, 0xAABB);
    assert_eq!(gba.mem.read_u8(SRAM + 0x20), 0xFF);
    assert_eq!(gba.mem.read_u8(SRAM + 0x21), 0xAA);
    gba.mem.write_u32(SRAM + 0x32, 0xAABB_CCDD);
    assert_eq!(gba.mem.read_u8(SRAM + 0x32), 0xBB);
    assert_eq!(gba.mem.read_u8(SRAM + 0x33), 0xFF);
}

#[test]
fn flash_64k_reports_panasonic_id() {
    let mut gba = gba_with_save_marker("FLASH_V");
    gba.mem.write_u8(SRAM + 0x5555, 0xAA);
    gba.mem.write_u8(SRAM + 0x2AAA, 0x55);
    gba.mem.write_u8(SRAM + 0x5555, 0x90);
    assert_eq!(gba.mem.read_u8(SRAM), 0x32);
    assert_eq!(gba.mem.read_u8(SRAM + 1), 0x1B);
    assert_eq!(gba.save_data().len(), 0x1_0000);
}

#[test]
fn save_data_round_trips() {
    let mut gba = gba_with_save_marker("SRAM_V");
    gba.mem.write_u8(SRAM + 5, 0x42);
    let data = gba.save_data().to_vec();
    assert_eq!(data.len(), 0x8000);
    assert_eq!(data[5], 0x42);

    let mut restored = gba_with_save_marker("SRAM_V");
    restored.load_save_data(&data);
    assert_eq!(restored.mem.read_u8(SRAM + 5), 0x42);
}

fn dma_bits(gba: &mut gbae::system::gba::Gba, bits: &[u16], destination: u32) {
    for (i, bit) in bits.iter().enumerate() {
        gba.mem.write_u16(EWRAM + i as u32 * 2, *bit);
    }
    gba.mem.write_u32(DMA3SAD, EWRAM);
    gba.mem.write_u32(DMA3DAD, destination);
    gba.mem.write_u16(DMA3CNT_L, bits.len() as u16);
    gba.mem.write_u16(DMA3CNT_H, DMA_ENABLE);
}

fn dma_read(gba: &mut gbae::system::gba::Gba, count: u16) -> Vec<u16> {
    gba.mem.write_u32(DMA3SAD, EEPROM);
    gba.mem.write_u32(DMA3DAD, EWRAM + 0x1000);
    gba.mem.write_u16(DMA3CNT_L, count);
    gba.mem.write_u16(DMA3CNT_H, DMA_ENABLE);
    (0..count as u32).map(|i| gba.mem.read_u16(EWRAM + 0x1000 + i * 2)).collect()
}

fn request_bits(prefix: [u16; 2], address: u32, address_bits: u32, data: Option<u64>) -> Vec<u16> {
    let mut bits = prefix.to_vec();
    bits.extend((0..address_bits).rev().map(|i| (address >> i & 1) as u16));
    if let Some(data) = data {
        bits.extend((0..64).rev().map(|i| (data >> i & 1) as u16));
    }
    bits.push(0);
    bits
}

#[test]
fn eeprom_is_written_and_read_through_dma() {
    let mut gba = gba_with_save_marker("EEPROM_V");
    let value = 0x1122_3344_5566_7788u64;
    dma_bits(&mut gba, &request_bits([1, 0], 0x2A, 14, Some(value)), EEPROM);
    assert_eq!(&gba.save_data()[0x2A * 8..0x2A * 8 + 8], &value.to_be_bytes());
    assert!(gba.take_save_dirty());

    dma_bits(&mut gba, &request_bits([1, 1], 0x2A, 14, None), EEPROM);
    let bits = dma_read(&mut gba, 68);
    let read = bits[4..].iter().fold(0u64, |acc, bit| acc << 1 | *bit as u64);
    assert_eq!(read, value);
    assert_eq!(gba.mem.read_u16(EEPROM), 1);
}

#[test]
fn eeprom_with_six_bit_addresses() {
    let mut gba = gba_with_save_marker("EEPROM_V");
    dma_bits(&mut gba, &request_bits([1, 0], 0x3F, 6, Some(0xDEAD_BEEF_0000_0001)), EEPROM);
    assert_eq!(&gba.save_data()[0x3F * 8..0x3F * 8 + 8], &0xDEAD_BEEF_0000_0001u64.to_be_bytes());
}
