/*
GBA Memory Map
General Internal Memory
  00_000_000-00_003_FFF   BIOS - System ROM         (极 KBytes)
  00_004_000-01_FFF_FFF   Not used
  02_000_000-02_03F_FFF   WRAM - On-board Work RAM  (256 KBytes) 2 Wait
  02_040_000-02_FFF_FFF   Not used
  03_000_000-03_007_FFF   WRAM - On-chip Work RAM   (32 KBytes)
  03_008_000-03_FFF_FFF   Not used
  04_000_000-04_000_3FE   I/O Registers
  04_000_400-04_FFF_FFF   Not used
Internal Display Memory
  05_000_000-05_000_3FF   BG/OBJ Palette RAM        (1 Kbyte)
  05_000_400-05_FFF_FFF   Not used
  06_000_000-06_017_FFF   VRAM - Video RAM          (96 KBytes)
  06_018_000-06_FFF_FFF   Not used
  07_000_000-07极_000_3FF   OAM - OBJ Attributes      (1 Kbyte)
  07_000_400-07_FFF_FFF   Not used
External Memory (Game Pak)
  08_000_000-09_FFF_FFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 0
  0A_000_000-0B_FFF_FFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 1
  0C_000_000-0D_FFF_FFF   Game Pak ROM/FlashROM (max 32MB) - Wait State 2
  0E_000_000-0E_00F_FFF   Game Pak SRAM    (max 64 KBytes) - 8bit Bus width
  0E_010_000-0F_FFF_FFF   Not used
Unused Memory Area
  10_000_000-FF_FFF_FFF   Not used (upper 4bits of address bus unused)
*/

use std::fmt::Display;

pub trait Region {
    fn read_u8(&self, offset: u32) -> u8;
    fn read_u16(&self, offset: u32) -> u16;
    fn read_u32(&self, offset: u32) -> u32;

    fn write_u8(&mut self, offset: u32, value: u8);
    fn write_u16(&mut self, offset: u32, value: u16);
    fn write_u32(&mut self, offset: u32, value: u32);
}

type WrapOffsetFn = fn(u32, usize) -> usize;

pub struct DataRegion {
    data: Vec<u8>,
    writable: bool,
    wrap_offset_logic: WrapOffsetFn,
}

impl DataRegion {
    pub fn new(data: Vec<u8>, writable: bool, wrap_offset_logic: WrapOffsetFn) -> Self {
        Self { data, writable, wrap_offset_logic }
    }
}

// Directly implement the Region trait for DataRegion
impl Region for DataRegion {
    fn read_u8(&self, offset: u32) -> u8 {
        let wrapped_offset = (self.wrap_offset_logic)(offset, self.data.len());
        self.data[wrapped_offset]
    }

    fn read_u16(&self, offset: u32) -> u16 {
        let wrapped_offset = (self.wrap_offset_logic)(offset, self.data.len());
        u16::from_le_bytes(self.data[wrapped_offset..wrapped_offset + 2].try_into().unwrap())
    }

    fn read_u32(&self, offset: u32) -> u32 {
        let wrapped_offset = (self.wrap_offset_logic)(offset, self.data.len());
        u32::from_le_bytes(self.data[wrapped_offset..wrapped_offset + 4].try_into().unwrap())
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        if !self.writable {
            panic!("Write to read-only region at offset {:#08X}", offset);
        }
        let wrapped_offset = (self.wrap_offset_logic)(offset, self.data.len());
        self.data[wrapped_offset] = value;
    }

    fn write_u16(&mut self, offset: u32, value: u16) {
        if !self.writable {
            panic!("Write to read-only region at offset {:#08X}", offset);
        }
        let wrapped_offset = (self.wrap_offset_logic)(offset, self.data.len());
        self.data[wrapped_offset..wrapped_offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(&mut self, offset: u32, value: u32) {
        if !self.writable {
            panic!("Write to read-only region at offset {:#08X}", offset);
        }
        let wrapped_offset = (self.wrap_offset_logic)(offset, self.data.len());
        self.data[wrapped_offset..wrapped_offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}

#[derive(Debug)]
enum RegisterKey {
    DispCnt,
    DispStat,
    VCount,
    Bg0Cnt,
    Bg1Cnt,
    Bg2Cnt,
    Bg3Cnt,
    Bg0HOffset,
    Bg0VOffset,
    Bg1HOffset,
    Bg1VOffset,
    Bg2HOffset,
    Bg2VOffset,
    Bg3HOffset,
    Bg3VOffset,
    Bg2Pa,
    Bg2Pb,
    Bg2Pc,
    Bg2Pd,
    Bg2RefX,
    Bg2RefY,
    Bg3Pa,
    Bg3Pb,
    Bg3Pc,
    Bg3Pd,
    Bg3RefX,
    Bg3RefY,
    Win0H,
    Win1H,
    Win0V,
    Win1V,
    WinIn,
    WinOut,
    Mosaic,
    BlendCnt,
    BlendAlpha,
    BlendY,
    Sound3CntL,
    SoundCntL,
    SoundCntH,
    SoundCntX,
    WaveRam,
    FifoA,
    FifoB,
    Dma0Sad,
    Dma0Dad,
    Dma0CntL,
    Dma0CntH,
    Dma1Sad,
    Dma1Dad,
    Dma1CntL,
    Dma1CntH,
    Dma2Sad,
    Dma2Dad,
    Dma2CntL,
    Dma2CntH,
    Dma3Sad,
    Dma3Dad,
    Dma3CntL,
    Dma3CntH,
    Tm0CntL,
    Tm0CntH,
    Tm1CntL,
    Tm1CntH,
    Tm2CntL,
    Tm2CntH,
    Tm3CntL,
    Tm3CntH,
    SioData32,
    SioMulti0,
    SioMulti1,
    SioMulti2,
    SioMulti3,
    SioCnt,
    SioData8,
    Rcnt,
    JoyCnt,
    JoyRecv,
    JoyTrans,
    JoyStat,
    Ie,
    Irf,
    WaitCnt,
    Ime,
    PostFlg,
    HaltCnt,
    SoundBias,
    Unused,
}

pub struct IoRegisters {
    pub disp_cnt: u16,  // 0x04000000 - Display Control
    disp_stat: u16, // 0x04000004 - General LCD Status (STAT,LYC)
    pub v_count: u16,   // 0x04000006 - Vertical Counter (LY)
    pub bg0_cnt: u16,   // 0x04000008 - BG0 Control
    pub bg1_cnt: u16,   // 0x0400000A - BG1 Control
    pub bg2_cnt: u16,   // 0x0400000C - BG2 Control
    pub bg3_cnt: u16,   // 0x0400000E - BG3 Control
    bg0_h_offset: u16, // 0x04000010 - BG0 X-Offset
    bg0_v_offset: u16, // 0x04000012 - BG0 Y-Offset
    bg1_h_offset: u16, // 0x04000014 - BG1 X-Offset
    bg1_v_offset: u16, // 0x04000016 - BG1 Y-Offset
    bg2_h_offset: u16, // 0x04000018 - BG2 X-Offset
    bg2_v_offset: u16, // 0x0400001A - BG2 Y-Offset
    bg3_h_offset: u16, // 0x0400001C - BG3 X-Offset
    bg3_v_offset: u16, // 0x0400001E - BG3 Y-Offset
    bg2_pa: u16,    // 0x04000020 - BG2 Rotation/Scaling Parameter A (dx)
    bg2_pb: u16,    // 0x04000022 - BG2 Rotation/Scaling Parameter B (dmx)
    bg2_pc: u16,    // 0x04000024 - BG2 Rotation/Scaling Parameter C (dy)
    bg2_pd: u16,    // 0x04000026 - BG2 Rotation/Scaling Parameter D (dmy)
    bg2_ref_x: u32, // 0x04000028 - BG2 Reference Point X
    bg2_ref_y: u32, // 0x0400002C - BG2 Reference Point Y
    bg3_pa: u16,    // 0x04000030 - BG3 Rotation/Scaling Parameter A (dx)
    bg3_pb: u16,    // 0x04000032 - BG3 Rotation/Scaling Parameter B (dmx)
    bg3_pc: u16,    // 0x04000034 - BG3 Rotation/Scaling Parameter C (dy)
    bg3_pd: u16,    // 0x04000036 - BG3 Rotation/Scaling Parameter D (dmy)
    bg3_ref_x: u32, // 0x04000038 - BG3 Reference Point X
    bg3_ref_y: u32, // 0x0400003C - BG3 Reference Point Y
    win0h: u16,     // 0x04000040 - Window 0 Horizontal
    win1h: u16,     // 0x04000042 - Window 1 Horizontal
    win0v: u16,     // 0x04000044 - Window 0 Vertical
    win1v: u16,     // 0x04000046 - Window 1 Vertical
    win_in: u16,    // 0x04000048 - Window Inside Control
    win_out: u16,   // 0x0400004A - Window Outside Control
    mosaic: u16,    // 0x0400004C - Mosaic
    blend_cnt: u16, // 0x04000050 - Blend Control
    blend_alpha: u16, // 0x04000052 - Blend Alpha
    blend_y: u16,   // 0x04000054 - Blend Brightness
    sound3_cnt_l: u16, // 0x04000070 - Sound 3 Control L
    sound_cnt_l: u16,  // 0x04000080 - Sound Control L
    sound_cnt_h: u16,  // 0x04000082 - Sound Control H
    sound_cnt_x: u16,  // 0x04000084 - Sound Control X
    soundbias: u16, // 0x04000088 - Sound PWM Control
    wave_ram: [u8; 16], // 0x04000090 - Wave RAM
    fifo_a: u32,    // 0x040000A0 - FIFO A
    fifo_b: u32,    // 0x040000A4 - FIFO B
    dma0_sad: u32,  // 0x040000B0 - DMA 0 Source Address
    dma0_dad: u32,  // 0x040000B4 - DMA 0 Destination Address
    dma0_cnt_l: u16, // 0x040000B8 - DMA 0 Word Count
    dma0_cnt_h: u16, // 0x040000BA - DMA 0 Control
    dma1_sad: u32,  // 0x040000BC - DMA 1 Source Address
    dma1_dad: u32,  // 0x040000C0 - DMA 1 Destination Address
    dma1_cnt_l: u16, // 0x040000C4 - DMA 1 Word Count
    dma1_cnt_h: u16, // 0x040000C6 - DMA 1 Control
    dma2_sad: u32,  // 0x040000C8 - DMA 2 Source Address
    dma2_dad: u32,  // 0x040000CC - DMA 2 Destination Address
    dma2_cnt_l: u16, // 0x040000D0 - DMA 2 Word Count
    dma2_cnt_h: u16, // 0x040000D2 - DMA 2 Control
    dma3_sad: u32,  // 0x040000D4 - DMA 3 Source Address
    dma3_dad: u32,  // 0x040000D8 - DMA 3 Destination Address
    dma3_cnt_l: u16, // 0x040000DC - DMA 3 Word Count
    dma3_cnt_h: u16, // 0x040000DE - DMA 3 Control
    tm0_cnt_l: u16, // 0x04000100 - Timer 0 Counter/Reload
    tm0_cnt_h: u16, // 0x04000102 - Timer 0 Control
    tm1_cnt_l: u16, // 0x04000104 - Timer 1 Counter/Reload
    tm1_cnt_h: u16, // 0x04000106 - Timer 1 Control
    tm2_cnt_l: u16, // 0x04000108 - Timer 2 Counter/Reload
    tm2_cnt_h: u16, // 0x0400010A - Timer 2 Control
    tm3_cnt_l: u16, // 0x0400010C - Timer 3 Counter/Reload
    tm3_cnt_h: u16, // 0x0400010E - Timer 3 Control
    sio_data32: u32, // 0x04000120 - Serial Data 32
    sio_multi0: u16, // 0x04000120 - Serial Multi 0
    sio_multi1: u16, // 0x04000122 - Serial Multi 1
    sio_multi2: u16, // 0x04000124 - Serial Multi 2
    sio_multi3: u16, // 0x04000126 - Serial Multi 3
    sio_cnt: u16,    // 0x04000128 - Serial Control
    sio_data8: u16,  // 0x0400012A - Serial Data 8
    rcnt: u16,       // 0x04000134 - Mode Select
    joy_cnt: u16,    // 0x04000140 - JOY Bus Control
    joy_recv: u32,   // 0x04000150 - JOY Bus Receive
    joy_trans: u32,  // 0x04000154 - JOY Bus Transmit
    joy_stat: u16,   // 0x04000158 - JOY Bus Status
    pub ie: u16,        // 0x04000200 - Interrupt Enable
    pub irf: u16,       // 0x04000202 - Interrupt Request Flags / IRQ Acknowledge
    wait_cnt: u16,  // 0x04000204 - Waitstate Control
    pub ime: bool,      // 0x04000208 - Interrupt Master Enable
    halt_cnt: u16,  // 0x04000300 - Undocumented - Post Boot / Debug Control
    post_flg: bool, // 0x04000300 - Post-Interrupt Flag
}

impl IoRegisters {
    pub fn new() -> Self {
        Self {
            disp_cnt: 0,
            disp_stat: 0,
            v_count: 0,
            bg0_cnt: 0,
            bg1_cnt: 0,
            bg2_cnt: 0,
            bg3_cnt: 0,
            bg0_h_offset: 0,
            bg0_v_offset: 0,
            bg1_h_offset: 0,
            bg1_v_offset: 0,
            bg2_h_offset: 0,
            bg2_v_offset: 0,
            bg3_h_offset: 0,
            bg3_v_offset: 0,
            bg2_pa: 0,
            bg2_pb: 0,
            bg2_pc: 0,
            bg2_pd: 0,
            bg2_ref_x: 0,
            bg2_ref_y: 0,
            bg3_pa: 0,
            bg3_pb: 0,
            bg3_pc: 0,
            bg3_pd: 0,
            bg3_ref_x: 0,
            bg3_ref_y: 0,
            win0h: 0,
            win1h: 0,
            win0v: 0,
            win1v: 0,
            win_in: 0,
            win_out: 0,
            mosaic: 0,
            blend_cnt: 0,
            blend_alpha: 0,
            blend_y: 0,
            sound3_cnt_l: 0,
            sound_cnt_l: 0,
            sound_cnt_h: 0,
            sound_cnt_x: 0,
            soundbias: 0x0200,
            wave_ram: [0; 16],
            fifo_a: 0,
            fifo_b: 0,
            dma0_sad: 0,
            dma0_dad: 0,
            dma0_cnt_l: 0,
            dma0_cnt_h: 0,
            dma1_sad: 0,
            dma1_dad: 0,
            dma1_cnt_l: 0,
            dma1_cnt_h: 0,
            dma2_sad: 0,
            dma2_dad: 0,
            dma2_cnt_l: 0,
            dma2_cnt_h: 0,
            dma3_sad: 0,
            dma3_dad: 0,
            dma3_cnt_l: 0,
            dma3_cnt_h: 0,
            tm0_cnt_l: 0,
            tm0_cnt_h: 0,
            tm1_cnt_l: 0,
            tm1_cnt_h: 0,
            tm2_cnt_l: 0,
            tm2_cnt_h: 0,
            tm3_cnt_l: 0,
            tm3_cnt_h: 0,
            sio_data32: 0,
            sio_multi0: 0,
            sio_multi1: 0,
            sio_multi2: 0,
            sio_multi3: 0,
            sio_cnt: 0,
            sio_data8: 0,
            rcnt: 0,
            joy_cnt: 0,
            joy_recv: 0,
            joy_trans: 0,
            joy_stat: 0,
            ie: 0,
            irf: 0,
            wait_cnt: 0,
            ime: false,
            halt_cnt: 0,
            post_flg: false,
        }
    }

    fn decode_offset(&self, offset: u32) -> RegisterKey {
        match offset {
            0x000..=0x001 => RegisterKey::DispCnt,
            0x002..=0x003 => RegisterKey::Unused,
            0x004..=0x005 => RegisterKey::DispStat,
            0x006..=0x007 => RegisterKey::VCount,
            0x008..=0x009 => RegisterKey::Bg0Cnt,
            0x00A..=0x00B => RegisterKey::Bg1Cnt,
            0x00C..=0x00D => RegisterKey::Bg2Cnt,
            0x00E..=0x00F => RegisterKey::Bg3Cnt,
            0x010..=0x011 => RegisterKey::Bg0HOffset,
            0x012..=0x013 => RegisterKey::Bg0VOffset,
            0x014..=0x015 => RegisterKey::Bg1HOffset,
            0x016..=0x017 => RegisterKey::Bg1VOffset,
            0x018..=0x019 => RegisterKey::Bg2HOffset,
            0x01A..=0x01B => RegisterKey::Bg2VOffset,
            0x01C..=0x01D => RegisterKey::Bg3HOffset,
            0x01E..=0x01F => RegisterKey::Bg3VOffset,
            0x020..=0x021 => RegisterKey::Bg2Pa,
            0x022..=0x023 => RegisterKey::Bg2Pb,
            0x024..=0x025 => RegisterKey::Bg2Pc,
            0x026..=0x027 => RegisterKey::Bg2Pd,
            0x028..=0x02B => RegisterKey::Bg2RefX,
            0x02C..=0x02F => RegisterKey::Bg2RefY,
            0x030..=0x031 => RegisterKey::Bg3Pa,
            0x032..=0x033 => RegisterKey::Bg3Pb,
            0x034..=0x035 => RegisterKey::Bg3Pc,
            0x036..=0x037 => RegisterKey::Bg3Pd,
            0x038..=0x03B => RegisterKey::Bg3RefX,
            0x03C..=0x03F => RegisterKey::Bg3RefY,
            0x040..=0x041 => RegisterKey::Win0H,
            0x042..=0x043 => RegisterKey::Win1H,
            0x044..=0x045 => RegisterKey::Win0V,
            0x046..=0x047 => RegisterKey::Win1V,
            0x048..=0x049 => RegisterKey::WinIn,
            0x04A..=0x04B => RegisterKey::WinOut,
            0x04C..=0x04D => RegisterKey::Mosaic,
            0x04E..=0x04F => RegisterKey::Unused,
            0x050..=0x051 => RegisterKey::BlendCnt,
            0x052..=0x053 => RegisterKey::BlendAlpha,
            0x054..=0x055 => RegisterKey::BlendY,
            0x056..=0x05F => RegisterKey::Unused,
            0x060..=0x06F => RegisterKey::Unused, // Sound channel 1-2
            0x070..=0x071 => RegisterKey::Sound3CntL,
            0x072..=0x07F => RegisterKey::Unused,
            0x080..=0x081 => RegisterKey::SoundCntL,
            0x082..=0x083 => RegisterKey::SoundCntH,
            0x084..=0x085 => RegisterKey::SoundCntX,
            0x086..=0x087 => RegisterKey::Unused,
            0x088..=0x089 => RegisterKey::SoundBias,
            0x08A..=0x08F => RegisterKey::Unused,
            0x090..=0x09F => RegisterKey::WaveRam,
            0x0A0..=0x0A3 => RegisterKey::FifoA,
            0x0A4..=0x0A7 => RegisterKey::FifoB,
            0x0A8..=0x0AF => RegisterKey::Unused,
            0x0B0..=0x0B3 => RegisterKey::Dma0Sad,
            0x0B4..=0x0B7 => RegisterKey::Dma0Dad,
            0x0B8..=0x0B9 => RegisterKey::Dma0CntL,
            0x0BA..=0x0BB => RegisterKey::Dma0CntH,
            0x0BC..=0x0BF => RegisterKey::Dma1Sad,
            0x0C0..=0x0C3 => RegisterKey::Dma1Dad,
            0x0C4..=0x0C5 => RegisterKey::Dma1CntL,
            0x0C6..=0x0C7 => RegisterKey::Dma1CntH,
            0x0C8..=0x0CB => RegisterKey::Dma2Sad,
            0x0CC..=0x0CF => RegisterKey::Dma2Dad,
            0x0D0..=0x0D1 => RegisterKey::Dma2CntL,
            0x0D2..=0x0D3 => RegisterKey::Dma2CntH,
            0x0D4..=0x0D7 => RegisterKey::Dma3Sad,
            0x0D8..=0x0DB => RegisterKey::Dma3Dad,
            0x0DC..=0x0DD => RegisterKey::Dma3CntL,
            0x0DE..=0x0DF => RegisterKey::Dma3CntH,
            0x0E0..=0x0FF => RegisterKey::Unused,
            0x100..=0x101 => RegisterKey::Tm0CntL,
            0x102..=0x103 => RegisterKey::Tm0CntH,
            0x104..=0x105 => RegisterKey::Tm1CntL,
            0x106..=0x107 => RegisterKey::Tm1CntH,
            0x108..=0x109 => RegisterKey::Tm2CntL,
            0x10A..=0x10B => RegisterKey::Tm2CntH,
            0x10C..=0x10D => RegisterKey::Tm3CntL,
            0x10E..=0x10F => RegisterKey::Tm3CntH,
            0x110..=0x11F => RegisterKey::Unused, // Serial IO (some overlap with below)
            0x120..=0x123 => RegisterKey::SioData32,
            0x124..=0x125 => RegisterKey::SioMulti2,
            0x126..=0x127 => RegisterKey::SioMulti3,
            0x128..=0x129 => RegisterKey::SioCnt,
            0x12A..=0x12B => RegisterKey::SioData8,
            0x12C..=0x133 => RegisterKey::Unused,
            0x134..=0x135 => RegisterKey::Rcnt,
            0x136..=0x13F => RegisterKey::Unused,
            0x140..=0x141 => RegisterKey::JoyCnt,
            0x142..=0x14F => RegisterKey::Unused,
            0x150..=0x153 => RegisterKey::JoyRecv,
            0x154..=0x157 => RegisterKey::JoyTrans,
            0x158..=0x159 => RegisterKey::JoyStat,
            0x15A..=0x15F => RegisterKey::Unused,
            0x160..=0x1FF => RegisterKey::Unused,
            0x200..=0x201 => RegisterKey::Ie,
            0x202..=0x203 => RegisterKey::Irf,
            0x204..=0x205 => RegisterKey::WaitCnt,
            0x206..=0x207 => RegisterKey::Unused,
            0x208..=0x209 => RegisterKey::Ime,
            0x20A..=0x20B => RegisterKey::Unused,
            0x20C..=0x21F => RegisterKey::HaltCnt, // Undocumented registers
            0x220..=0x2FF => RegisterKey::Unused,
            0x300..=0x301 => RegisterKey::PostFlg,
            0x302..=0x3FF => RegisterKey::Unused,
            _ => panic!("Unsupported I/O register access at offset {:#08X}", offset),
        }
    }
}
impl Display for IoRegisters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cnt = self.disp_cnt;
        let stat = self.disp_stat;

        writeln!(
            f,
            "DISPCNT  = {:#06X}\n\
             MODE CGB FRM HBLK VRAM FBLK BG0 BG1 BG2 BG3 OBJ W0 W1 OW\n\
             {:>4} {:>3} {:>3}  {:>4}  {:>4} {:>4}  {:>3} {:>3} {:>3} {:>3} {:>3} {:>2} {:>2} {:>2}",
            cnt,
            cnt & 0b111,                                     // MODE
            (cnt >> 3) & 1,                                  // CGB
            (cnt >> 4) & 1,                                  // FRM
            (cnt >> 5) & 1,                                  // HBLK
            if ((cnt >> 6) & 1) == 1 { "1D" } else { "2D" }, // VRAM
            (cnt >> 7) & 1,                                  // FBLK
            (cnt >> 8) & 1,                                  // BG0
            (cnt >> 9) & 1,                                  // BG1
            (cnt >> 10) & 1,                                 // BG2
            (cnt >> 11) & 1,                                 // BG3
            (cnt >> 12) & 1,                                 // OBJ
            (cnt >> 13) & 1,                                 // W0
            (cnt >> 14) & 1,                                 // W1
            (cnt >> 15) & 1                                  // OBJ Window
        )?;

        writeln!(
            f,
            "DISPSTAT = {:#06X}\n\
             VB HB VC VBI HBI VCI MSB LYC\n\
             {:>2} {:>2} {:>2}  {:>2}  {:>2}  {:>2}  {:>3} {:>3}",
            stat,
            (stat >> 0) & 1,    // VBlank flag
            (stat >> 1) & 1,    // HBlank flag
            (stat >> 2) & 1,    // VCounter flag
            (stat >> 3) & 1,    // VBlank IRQ enable
            (stat >> 4) & 1,    // HBlank IRQ enable
            (stat >> 5) & 1,    // VCounter IRQ enable
            (stat >> 7) & 1,    // LYC MSB
            (stat >> 8) & 0xFF  // LYC
        )?;

        writeln!(f, "V_COUNT = {v_count}", v_count=self.v_count)
    }
}

impl Region for IoRegisters {
    fn read_u8(&self, offset: u32) -> u8 {
        let r = self.decode_offset(offset);
        match r {
            _ => self.read_u16(offset) as u8, // Fallback to 16-bit read
        }
    }

    fn read_u16(&self, offset: u32) -> u16 {
        let r = self.decode_offset(offset);
        match r {
            RegisterKey::DispCnt => self.disp_cnt,
            RegisterKey::DispStat => self.disp_stat,
            RegisterKey::VCount => self.v_count,
            RegisterKey::Bg0Cnt => self.bg0_cnt,
            RegisterKey::Bg1Cnt => self.bg1_cnt,
            RegisterKey::Bg2Cnt => self.bg2_cnt,
            RegisterKey::Bg3Cnt => self.bg3_cnt,
            RegisterKey::Bg0HOffset => self.bg0_h_offset,
            RegisterKey::Bg0VOffset => self.bg0_v_offset,
            RegisterKey::Bg1HOffset => self.bg1_h_offset,
            RegisterKey::Bg1VOffset => self.bg1_v_offset,
            RegisterKey::Bg2HOffset => self.bg2_h_offset,
            RegisterKey::Bg2VOffset => self.bg2_v_offset,
            RegisterKey::Bg3HOffset => self.bg3_h_offset,
            RegisterKey::Bg3VOffset => self.bg3_v_offset,
            RegisterKey::Bg2Pa => self.bg2_pa,
            RegisterKey::Bg2Pb => self.bg2_pb,
            RegisterKey::Bg2Pc => self.bg2_pc,
            RegisterKey::Bg2Pd => self.bg2_pd,
            RegisterKey::Bg3Pa => self.bg3_pa,
            RegisterKey::Bg3Pb => self.bg3_pb,
            RegisterKey::Bg3Pc => self.bg3_pc,
            RegisterKey::Bg3Pd => self.bg3_pd,
            RegisterKey::Win0H => self.win0h,
            RegisterKey::Win1H => self.win1h,
            RegisterKey::Win0V => self.win0v,
            RegisterKey::Win1V => self.win1v,
            RegisterKey::WinIn => self.win_in,
            RegisterKey::WinOut => self.win_out,
            RegisterKey::Mosaic => self.mosaic,
            RegisterKey::BlendCnt => self.blend_cnt,
            RegisterKey::BlendAlpha => self.blend_alpha,
            RegisterKey::BlendY => self.blend_y,
            RegisterKey::Sound3CntL => self.sound3_cnt_l,
            RegisterKey::SoundCntL => self.sound_cnt_l,
            RegisterKey::SoundCntH => self.sound_cnt_h,
            RegisterKey::SoundCntX => self.sound_cnt_x,
            RegisterKey::SoundBias => self.soundbias,
            RegisterKey::Dma0CntL => self.dma0_cnt_l,
            RegisterKey::Dma0CntH => self.dma0_cnt_h,
            RegisterKey::Dma1CntL => self.dma1_cnt_l,
            RegisterKey::Dma1CntH => self.dma1_cnt_h,
            RegisterKey::Dma2CntL => self.dma2_cnt_l,
            RegisterKey::Dma2CntH => self.dma2_cnt_h,
            RegisterKey::Dma3CntL => self.dma3_cnt_l,
            RegisterKey::Dma3CntH => self.dma3_cnt_h,
            RegisterKey::Tm0CntL => self.tm0_cnt_l,
            RegisterKey::Tm0CntH => self.tm0_cnt_h,
            RegisterKey::Tm1CntL => self.tm1_cnt_l,
            RegisterKey::Tm1CntH => self.tm1_cnt_h,
            RegisterKey::Tm2CntL => self.tm2_cnt_l,
            RegisterKey::Tm2CntH => self.tm2_cnt_h,
            RegisterKey::Tm3CntL => self.tm3_cnt_l,
            RegisterKey::Tm3CntH => self.tm3_cnt_h,
            RegisterKey::SioMulti0 => self.sio_multi0,
            RegisterKey::SioMulti1 => self.sio_multi1,
            RegisterKey::SioMulti2 => self.sio_multi2,
            RegisterKey::SioMulti3 => self.sio_multi3,
            RegisterKey::SioCnt => self.sio_cnt,
            RegisterKey::SioData8 => self.sio_data8,
            RegisterKey::Rcnt => self.rcnt,
            RegisterKey::JoyCnt => self.joy_cnt,
            RegisterKey::JoyStat => self.joy_stat,
            RegisterKey::Ie => self.ie,
            RegisterKey::Irf => self.irf,
            RegisterKey::WaitCnt => self.wait_cnt,
            RegisterKey::Ime => if self.ime { 1 } else { 0 },
            RegisterKey::HaltCnt => self.halt_cnt,
            RegisterKey::PostFlg => if self.post_flg { 1 } else { 0 },
            RegisterKey::Bg2RefX | RegisterKey::Bg2RefY | RegisterKey::Bg3RefX | RegisterKey::Bg3RefY |
            RegisterKey::WaveRam | RegisterKey::FifoA | RegisterKey::FifoB |
            RegisterKey::Dma0Sad | RegisterKey::Dma0Dad | RegisterKey::Dma1Sad | RegisterKey::Dma1Dad |
            RegisterKey::Dma2Sad | RegisterKey::Dma2Dad | RegisterKey::Dma3Sad | RegisterKey::Dma3Dad |
            RegisterKey::SioData32 | RegisterKey::JoyRecv | RegisterKey::JoyTrans => {
                // These are 32-bit registers or special, handle via read_u32
                0
            }
            RegisterKey::Unused => {
                eprintln!("WARNING: Read from unused I/O register at offset {:#08X}", offset);
                0
            }
        }
    }

    fn read_u32(&self, offset: u32) -> u32 {
        let r = self.decode_offset(offset);
        match r {
            RegisterKey::Bg2RefX => self.bg2_ref_x,
            RegisterKey::Bg2RefY => self.bg2_ref_y,
            RegisterKey::Bg3RefX => self.bg3_ref_x,
            RegisterKey::Bg3RefY => self.bg3_ref_y,
            RegisterKey::FifoA => self.fifo_a,
            RegisterKey::FifoB => self.fifo_b,
            RegisterKey::Dma0Sad => self.dma0_sad,
            RegisterKey::Dma0Dad => self.dma0_dad,
            RegisterKey::Dma1Sad => self.dma1_sad,
            RegisterKey::Dma1Dad => self.dma1_dad,
            RegisterKey::Dma2Sad => self.dma2_sad,
            RegisterKey::Dma2Dad => self.dma2_dad,
            RegisterKey::Dma3Sad => self.dma3_sad,
            RegisterKey::Dma3Dad => self.dma3_dad,
            RegisterKey::SioData32 => self.sio_data32,
            RegisterKey::JoyRecv => self.joy_recv,
            RegisterKey::JoyTrans => self.joy_trans,
            RegisterKey::WaveRam => {
                // Read 4 bytes from wave RAM at this offset
                let wave_offset = (offset & 0x0F) as usize;
                u32::from_le_bytes([
                    self.wave_ram[wave_offset],
                    self.wave_ram[wave_offset + 1],
                    self.wave_ram[wave_offset + 2],
                    self.wave_ram[wave_offset + 3],
                ])
            }
            _ => self.read_u16(offset) as u32, // Fallback to 16-bit read
        }
    }

    fn write_u8(&mut self, offset: u32, value: u8) {
        let r = self.decode_offset(offset);
        match r {
            _ => self.write_u16(offset, value as u16), // Fallback to 16-bit write
        }
    }

    fn write_u16(&mut self, offset: u32, value: u16) {
        let r = self.decode_offset(offset);
        match r {
            RegisterKey::DispCnt => self.disp_cnt = value,
            RegisterKey::DispStat => self.disp_stat = value,
            RegisterKey::VCount => self.v_count = value,
            RegisterKey::Bg0Cnt => self.bg0_cnt = value,
            RegisterKey::Bg1Cnt => self.bg1_cnt = value,
            RegisterKey::Bg2Cnt => {
                if value != 0 {
                    println!("BG2CNT set to: {:#06X} (CharBase: {}, ScreenBase: {}, Size: {})",
                        value,
                        ((value >> 2) & 0x3) * 0x4000,
                        ((value >> 8) & 0x1F) * 0x800,
                        (value >> 14) & 0x3
                    );
                }
                self.bg2_cnt = value;
            },
            RegisterKey::Bg3Cnt => self.bg3_cnt = value,
            RegisterKey::Bg0HOffset => self.bg0_h_offset = value,
            RegisterKey::Bg0VOffset => self.bg0_v_offset = value,
            RegisterKey::Bg1HOffset => self.bg1_h_offset = value,
            RegisterKey::Bg1VOffset => self.bg1_v_offset = value,
            RegisterKey::Bg2HOffset => self.bg2_h_offset = value,
            RegisterKey::Bg2VOffset => self.bg2_v_offset = value,
            RegisterKey::Bg3HOffset => self.bg3_h_offset = value,
            RegisterKey::Bg3VOffset => self.bg3_v_offset = value,
            RegisterKey::Bg2Pa => self.bg2_pa = value,
            RegisterKey::Bg2Pb => self.bg2_pb = value,
            RegisterKey::Bg2Pc => self.bg2_pc = value,
            RegisterKey::Bg2Pd => self.bg2_pd = value,
            RegisterKey::Bg3Pa => self.bg3_pa = value,
            RegisterKey::Bg3Pb => self.bg3_pb = value,
            RegisterKey::Bg3Pc => self.bg3_pc = value,
            RegisterKey::Bg3Pd => self.bg3_pd = value,
            RegisterKey::Win0H => self.win0h = value,
            RegisterKey::Win1H => self.win1h = value,
            RegisterKey::Win0V => self.win0v = value,
            RegisterKey::Win1V => self.win1v = value,
            RegisterKey::WinIn => self.win_in = value,
            RegisterKey::WinOut => self.win_out = value,
            RegisterKey::Mosaic => self.mosaic = value,
            RegisterKey::BlendCnt => self.blend_cnt = value,
            RegisterKey::BlendAlpha => self.blend_alpha = value,
            RegisterKey::BlendY => self.blend_y = value,
            RegisterKey::Sound3CntL => self.sound3_cnt_l = value,
            RegisterKey::SoundCntL => self.sound_cnt_l = value,
            RegisterKey::SoundCntH => self.sound_cnt_h = value,
            RegisterKey::SoundCntX => self.sound_cnt_x = value,
            RegisterKey::SoundBias => self.soundbias = value,
            RegisterKey::Dma0CntL => self.dma0_cnt_l = value,
            RegisterKey::Dma0CntH => {
                self.dma0_cnt_h = value;
                // Trigger DMA immediately if enable bit is set
                if (value & 0x8000) != 0 {
                    // DMA will be performed in a moment
                }
            },
            RegisterKey::Dma1CntL => self.dma1_cnt_l = value,
            RegisterKey::Dma1CntH => {
                self.dma1_cnt_h = value;
                if (value & 0x8000) != 0 {
                    // DMA will be performed in a moment
                }
            },
            RegisterKey::Dma2CntL => self.dma2_cnt_l = value,
            RegisterKey::Dma2CntH => {
                self.dma2_cnt_h = value;
                if (value & 0x8000) != 0 {
                    // DMA will be performed in a moment
                }
            },
            RegisterKey::Dma3CntL => self.dma3_cnt_l = value,
            RegisterKey::Dma3CntH => {
                self.dma3_cnt_h = value;
                if (value & 0x8000) != 0 {
                    // DMA will be performed in a moment
                }
            },
            RegisterKey::Tm0CntL => self.tm0_cnt_l = value,
            RegisterKey::Tm0CntH => self.tm0_cnt_h = value,
            RegisterKey::Tm1CntL => self.tm1_cnt_l = value,
            RegisterKey::Tm1CntH => self.tm1_cnt_h = value,
            RegisterKey::Tm2CntL => self.tm2_cnt_l = value,
            RegisterKey::Tm2CntH => self.tm2_cnt_h = value,
            RegisterKey::Tm3CntL => self.tm3_cnt_l = value,
            RegisterKey::Tm3CntH => self.tm3_cnt_h = value,
            RegisterKey::SioMulti0 => self.sio_multi0 = value,
            RegisterKey::SioMulti1 => self.sio_multi1 = value,
            RegisterKey::SioMulti2 => self.sio_multi2 = value,
            RegisterKey::SioMulti3 => self.sio_multi3 = value,
            RegisterKey::SioCnt => self.sio_cnt = value,
            RegisterKey::SioData8 => self.sio_data8 = value,
            RegisterKey::Rcnt => self.rcnt = value,
            RegisterKey::JoyCnt => self.joy_cnt = value,
            RegisterKey::JoyStat => self.joy_stat = value,
            RegisterKey::Ie => self.ie = value,
            RegisterKey::Irf => self.irf = value,
            RegisterKey::WaitCnt => self.wait_cnt = value,
            RegisterKey::Ime => self.ime = value & 1 != 0,
            RegisterKey::HaltCnt => self.halt_cnt = value,
            RegisterKey::PostFlg => self.post_flg = value & 1 != 0,
            RegisterKey::Bg2RefX | RegisterKey::Bg2RefY | RegisterKey::Bg3RefX | RegisterKey::Bg3RefY |
            RegisterKey::WaveRam | RegisterKey::FifoA | RegisterKey::FifoB |
            RegisterKey::Dma0Sad | RegisterKey::Dma0Dad | RegisterKey::Dma1Sad | RegisterKey::Dma1Dad |
            RegisterKey::Dma2Sad | RegisterKey::Dma2Dad | RegisterKey::Dma3Sad | RegisterKey::Dma3Dad |
            RegisterKey::SioData32 | RegisterKey::JoyRecv | RegisterKey::JoyTrans => {
                // These are 32-bit registers or special, handle via write_u32
            }
            RegisterKey::Unused => {
                // Silently ignore writes to unused registers (real hardware behavior)
                // Don't print warnings here as they spam the output
            }
        }
    }

    fn write_u32(&mut self, offset: u32, value: u32) {
        let r = self.decode_offset(offset);
        match r {
            RegisterKey::Bg2RefX => self.bg2_ref_x = value,
            RegisterKey::Bg2RefY => self.bg2_ref_y = value,
            RegisterKey::Bg3RefX => self.bg3_ref_x = value,
            RegisterKey::Bg3RefY => self.bg3_ref_y = value,
            RegisterKey::FifoA => self.fifo_a = value,
            RegisterKey::FifoB => self.fifo_b = value,
            RegisterKey::Dma0Sad => self.dma0_sad = value,
            RegisterKey::Dma0Dad => self.dma0_dad = value,
            RegisterKey::Dma1Sad => self.dma1_sad = value,
            RegisterKey::Dma1Dad => self.dma1_dad = value,
            RegisterKey::Dma2Sad => self.dma2_sad = value,
            RegisterKey::Dma2Dad => self.dma2_dad = value,
            RegisterKey::Dma3Sad => self.dma3_sad = value,
            RegisterKey::Dma3Dad => self.dma3_dad = value,
            RegisterKey::SioData32 => self.sio_data32 = value,
            RegisterKey::JoyRecv => self.joy_recv = value,
            RegisterKey::JoyTrans => self.joy_trans = value,
            RegisterKey::WaveRam => {
                // Write 4 bytes to wave RAM at this offset
                let wave_offset = (offset & 0x0F) as usize;
                let bytes = value.to_le_bytes();
                self.wave_ram[wave_offset] = bytes[0];
                self.wave_ram[wave_offset + 1] = bytes[1];
                self.wave_ram[wave_offset + 2] = bytes[2];
                self.wave_ram[wave_offset + 3] = bytes[3];
            }
            _ => self.write_u16(offset, value as u16), // Fallback to 16-bit write
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum RegionKey {
    Bios(u32),
    Wram1(u32),
    Wram2(u32),
    IoRegisters(u32),
    IoUnused(u32),
    PaletteRam(u32),
    Vram(u32),
    Oam(u32),
    GamePak(u32),
}

fn decode_address(address: u32) -> RegionKey {
    const BIOS_START: u32 = 0x0000_0000;
    const WRAM1_START: u32 = 0x0200_0000;
    const WRAM2_START: u32 = 0x0300_0000;
    const IO_REGISTERS_START: u32 = 0x0400_0000;
    const IO_UNUSED_START: u32 = 0x0400_0410;
    const PALETTE_RAM_START: u32 = 0x0500_0000;
    const VRAM_START: u32 = 0x0600_0000;
    const OAM_START: u32 = 0x0700_0000;
    const GAME_PAK_START: u32 = 0x0800_0000;

    match address {
        BIOS_START..=0x0000_3FFF => RegionKey::Bios(address - BIOS_START),
        WRAM1_START..=0x02FF_FFFF => RegionKey::Wram1(address - WRAM1_START),
        WRAM2_START..=0x03FF_FFFF => RegionKey::Wram2(address - WRAM2_START),
        IO_REGISTERS_START..=0x0400_03FE => RegionKey::IoRegisters(address - IO_REGISTERS_START),
        IO_UNUSED_START..=0x0400_0410 => RegionKey::IoUnused(address - IO_UNUSED_START),
        PALETTE_RAM_START..=0x05FF_FFFF => RegionKey::PaletteRam(address - PALETTE_RAM_START),
        VRAM_START..=0x06FF_FFFF => RegionKey::Vram(address - VRAM_START),
        OAM_START..=0x07FF_FFFF => RegionKey::Oam(address - OAM_START),
        GAME_PAK_START..=0x09FF_FFFF => RegionKey::GamePak(address - GAME_PAK_START),
        _ => panic!("Unmapped address: {:#08X}", address),
    }
}

pub struct Memory {
    bios: Box<dyn Region>,
    wram1: Box<dyn Region>,
    wram2: Box<dyn Region>,
    io_registers: Box<IoRegisters>,
    io_unused: Box<dyn Region>,
    palette_ram: Box<dyn Region>,
    vram: Box<dyn Region>,
    oam: Box<dyn Region>,
    game_pak: Box<dyn Region>,
}

macro_rules! implement_memory_access {
    ($read_func:ident, $write_func:ident, $type:ty) => {
        pub fn $read_func(&self, address: u32) -> $type {
            match decode_address(address) {
                RegionKey::Bios(offset) => self.bios.$read_func(offset),
                RegionKey::Wram1(offset) => self.wram1.$read_func(offset),
                RegionKey::Wram2(offset) => self.wram2.$read_func(offset),
                RegionKey::IoRegisters(offset) => self.io_registers.$read_func(offset),
                RegionKey::IoUnused(offset) => self.io_unused.$read_func(offset),
                RegionKey::PaletteRam(offset) => self.palette_ram.$read_func(offset),
                RegionKey::Vram(offset) => self.vram.$read_func(offset),
                RegionKey::Oam(offset) => self.oam.$read_func(offset),
                RegionKey::GamePak(offset) => self.game_pak.$read_func(offset),
            }
        }

        pub fn $write_func(&mut self, address: u32, value: $type) {
            match decode_address(address) {
                RegionKey::Bios(offset) => self.bios.$write_func(offset, value),
                RegionKey::Wram1(offset) => self.wram1.$write_func(offset, value),
                RegionKey::Wram2(offset) => self.wram2.$write_func(offset, value),
                RegionKey::IoRegisters(offset) => {
                    self.io_registers.$write_func(offset, value);
                    // Check for immediate DMA transfers after I/O register writes
                    if stringify!($write_func) == "write_u16" || stringify!($write_func) == "write_u32" {
                        self.check_immediate_dma();
                    }
                },
                RegionKey::IoUnused(offset) => self.io_unused.$write_func(offset, value),
                RegionKey::PaletteRam(offset) => self.palette_ram.$write_func(offset, value),
                RegionKey::Vram(offset) => self.vram.$write_func(offset, value),
                RegionKey::Oam(offset) => self.oam.$write_func(offset, value),
                RegionKey::GamePak(offset) => self.game_pak.$write_func(offset, value),
            }
        }
    };
}

impl Memory {
    pub fn new(bios: Vec<u8>, game_pak: Vec<u8>) -> Self {
        const WRAM1_LEN: usize = 0x40_000;
        const WRAM2_LEN: usize = 0x800;
        const IO_UNUSED_LEN: usize = 0x1;
        const PALETTE_RAM_LEN: usize = 0x400;
        const VRAM_LEN: usize = 0x18_000;
        const OAM_LEN: usize = 0x400;

        let simple_wrap_offset = |offset: u32, data_len: usize| (offset % data_len as u32) as usize;

        let vram_wrap_offset = |offset: u32, data_len: usize| {
            let mut off = offset % 0x20_000;
            if off >= data_len as u32 {
                off -= data_len as u32;
            }
            off as usize
        };

        Self {
            bios: Box::new(DataRegion::new(bios, false, simple_wrap_offset)),
            wram1: Box::new(DataRegion::new(vec![0; WRAM1_LEN], true, simple_wrap_offset)),
            wram2: Box::new(DataRegion::new(vec![0; WRAM2_LEN], true, simple_wrap_offset)),
            io_registers: Box::new(IoRegisters::new()),
            io_unused: Box::new(DataRegion::new(vec![0; IO_UNUSED_LEN], true, simple_wrap_offset)),
            palette_ram: Box::new(DataRegion::new(vec![0; PALETTE_RAM_LEN], true, simple_wrap_offset)),
            vram: Box::new(DataRegion::new(vec![0; VRAM_LEN], true, vram_wrap_offset)),
            oam: Box::new(DataRegion::new(vec![0; OAM_LEN], true, simple_wrap_offset)),
            game_pak: Box::new(DataRegion::new(game_pak, false, simple_wrap_offset)),
        }
    }

    implement_memory_access!(read_u8, write_u8, u8);
    implement_memory_access!(read_u16, write_u16, u16);
    implement_memory_access!(read_u32, write_u32, u32);

    pub fn print_io_registers(&self) {
        println!("{}", self.io_registers);
    }

    pub fn get_io_registers(&mut self) -> &IoRegisters {
        self.io_registers.as_ref()
    }

    pub fn get_io_registers_mut(&mut self) -> &mut IoRegisters {
        self.io_registers.as_mut()
    }

    // Check and perform DMA transfers if enabled
    pub fn check_dma_transfers(&mut self) {
        // Check DMA0
        if (self.io_registers.dma0_cnt_h & 0x8000) != 0 {
            self.perform_dma_transfer(0);
        }
        // Check DMA1
        if (self.io_registers.dma1_cnt_h & 0x8000) != 0 {
            self.perform_dma_transfer(1);
        }
        // Check DMA2
        if (self.io_registers.dma2_cnt_h & 0x8000) != 0 {
            self.perform_dma_transfer(2);
        }
        // Check DMA3
        if (self.io_registers.dma3_cnt_h & 0x8000) != 0 {
            self.perform_dma_transfer(3);
        }
    }

    // Check and perform immediate DMA transfers (timing mode = 0)
    fn check_immediate_dma(&mut self) {
        // Check DMA0: enabled (bit 15) and immediate timing (bits 12-13 = 0)
        if (self.io_registers.dma0_cnt_h & 0x8000) != 0 &&
           ((self.io_registers.dma0_cnt_h >> 12) & 3) == 0 {
            self.perform_dma_transfer(0);
        }
        // Check DMA1
        if (self.io_registers.dma1_cnt_h & 0x8000) != 0 &&
           ((self.io_registers.dma1_cnt_h >> 12) & 3) == 0 {
            self.perform_dma_transfer(1);
        }
        // Check DMA2
        if (self.io_registers.dma2_cnt_h & 0x8000) != 0 &&
           ((self.io_registers.dma2_cnt_h >> 12) & 3) == 0 {
            self.perform_dma_transfer(2);
        }
        // Check DMA3
        if (self.io_registers.dma3_cnt_h & 0x8000) != 0 &&
           ((self.io_registers.dma3_cnt_h >> 12) & 3) == 0 {
            self.perform_dma_transfer(3);
        }
    }

    fn perform_dma_transfer(&mut self, channel: u8) {
        let (src, dst, count, control) = match channel {
            0 => (self.io_registers.dma0_sad, self.io_registers.dma0_dad,
                  self.io_registers.dma0_cnt_l as u32, self.io_registers.dma0_cnt_h),
            1 => (self.io_registers.dma1_sad, self.io_registers.dma1_dad,
                  self.io_registers.dma1_cnt_l as u32, self.io_registers.dma1_cnt_h),
            2 => (self.io_registers.dma2_sad, self.io_registers.dma2_dad,
                  self.io_registers.dma2_cnt_l as u32, self.io_registers.dma2_cnt_h),
            3 => (self.io_registers.dma3_sad, self.io_registers.dma3_dad,
                  self.io_registers.dma3_cnt_l as u32, self.io_registers.dma3_cnt_h),
            _ => return,
        };

        let transfer_type = (control >> 10) & 1; // 0=16bit, 1=32bit
        let src_control = (control >> 7) & 3; // 0=inc, 1=dec, 2=fixed, 3=inc+reload
        let dst_control = (control >> 5) & 3; // 0=inc, 1=dec, 2=fixed, 3=inc+reload
        let repeat = (control >> 9) & 1;
        let irq_enable = (control >> 14) & 1;

        let mut count = count;
        if count == 0 {
            count = match channel {
                3 => 0x10000, // DMA3 can transfer up to 64K
                _ => 0x4000,  // DMA0-2 can transfer up to 16K
            };
        }

        let unit_size = if transfer_type == 0 { 2 } else { 4 }; // 16bit or 32bit
        let mut src_addr = src;
        let mut dst_addr = dst;

        // Perform the transfer
        for _ in 0..count {
            if transfer_type == 0 {
                // 16-bit transfer
                let value = self.read_u16(src_addr);
                self.write_u16(dst_addr, value);
            } else {
                // 32-bit transfer
                let value = self.read_u32(src_addr);
                self.write_u32(dst_addr, value);
            }

            // Update source address
            match src_control {
                0 => src_addr = src_addr.wrapping_add(unit_size), // Increment
                1 => src_addr = src_addr.wrapping_sub(unit_size), // Decrement
                2 => {}, // Fixed
                3 => src_addr = src_addr.wrapping_add(unit_size), // Increment + reload
                _ => {}
            }

            // Update destination address
            match dst_control {
                0 => dst_addr = dst_addr.wrapping_add(unit_size), // Increment
                1 => dst_addr = dst_addr.wrapping_sub(unit_size), // Decrement
                2 => {}, // Fixed
                3 => dst_addr = dst_addr.wrapping_add(unit_size), // Increment + reload
                _ => {}
            }
        }

        // If not repeat mode, clear the enable bit
        if repeat == 0 {
            match channel {
                0 => self.io_registers.dma0_cnt_h &= !0x8000,
                1 => self.io_registers.dma1_cnt_h &= !0x8000,
                2 => self.io_registers.dma2_cnt_h &= !0x8000,
                3 => self.io_registers.dma3_cnt_h &= !0x8000,
                _ => {}
            }
        }

        // Trigger interrupt if enabled
        if irq_enable != 0 {
            let irq_bit = match channel {
                0 => 1 << 8,  // DMA0 IRQ
                1 => 1 << 9,  // DMA1 IRQ
                2 => 1 << 10, // DMA2 IRQ
                3 => 1 << 11, // DMA3 IRQ
                _ => 0,
            };
            self.io_registers.irf |= irq_bit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_decoding() {
        // BIOS
        match decode_address(0x0000_0000) {
            RegionKey::Bios(0) => (),
            _ => panic!(),
        }
        match decode_address(0x0000_3FFF) {
            RegionKey::Bios(0x3FFF) => (),
            _ => panic!(),
        }

        // WRAM1
        match decode_address(0x0200_0000) {
            RegionKey::Wram1(0) => (),
            _ => panic!(),
        }
        match decode_address(0x02FF_FFFF) {
            RegionKey::Wram1(0xFF_FFFF) => (),
            _ => panic!(),
        }

        // I/O Registers
        match decode_address(0x0400_0000) {
            RegionKey::IoRegisters(0) => (),
            _ => panic!(),
        }
        match decode_address(0x0400_03FE) {
            RegionKey::IoRegisters(0x3FE) => (),
            _ => panic!(),
        }

        // I/O Unused
        match decode_address(0x0400_0410) {
            RegionKey::IoUnused(0) => (),
            _ => panic!(),
        }

        // Game Pak
        match decode_address(0x0800_0000) {
            RegionKey::GamePak(0) => (),
            _ => panic!(),
        }
        match decode_address(0x09FF_FFFF) {
            RegionKey::GamePak(0x1FF_FFFF) => (),
            _ => panic!(),
        }
    }

    #[test]
    fn test_vram_wrapping() {
        let mut vram = Memory::new(vec![], vec![]).vram;

        // Test normal access
        vram.write_u8(0, 0x12);
        assert_eq!(vram.read_u8(0), 0x12);

        // Test mirroring
        vram.write_u8(0x18000, 0x34);
        assert_eq!(vram.read_u8(0), 0x34);
    }

    #[test]
    fn test_io_registers() {
        let mut io = IoRegisters::new();
        io.write_u16(0x000, 0x1234);
        assert_eq!(io.read_u16(0x000), 0x1234);
    }

    #[test]
    #[should_panic]
    fn test_io_registers_u8_read_panic() {
        let io = IoRegisters::new();
        io.read_u8(0x000);
    }

    #[test]
    #[should_panic]
    fn test_io_registers_u32_write_panic() {
        let mut io = IoRegisters::new();
        io.write_u32(0x000, 0x12345678);
    }
}
