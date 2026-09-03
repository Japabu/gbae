use crate::bits::Bits;

use super::state::{Reader, StateError, Writer};

const SCK: u32 = 0;
const SIO: u32 = 1;
const CS: u32 = 2;

const COMMAND_RESET: u8 = 0;
const COMMAND_STATUS: u8 = 1;
const COMMAND_DATE_TIME: u8 = 2;
const COMMAND_TIME: u8 = 3;

const STATUS_24_HOUR: u8 = 1 << 6;
const STATUS_WRITABLE: u8 = 0x6A;

pub struct Gpio {
    data: u8,
    direction: u8,
    readable: bool,
    pub rtc: Rtc,
}

impl Gpio {
    pub fn new() -> Gpio {
        Gpio {
            data: 0,
            direction: 0,
            readable: false,
            rtc: Rtc::new(),
        }
    }

    pub fn readable(&self) -> bool {
        self.readable
    }

    pub fn save_state(&self, writer: &mut Writer) {
        writer.u8(self.data);
        writer.u8(self.direction);
        writer.bool(self.readable);
        self.rtc.save_state(writer);
    }

    pub fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.data = reader.u8()?;
        self.direction = reader.u8()?;
        self.readable = reader.bool()?;
        self.rtc.load_state(reader)
    }

    pub fn read(&self, offset: u32) -> u16 {
        match offset {
            0 => {
                let inputs = 0u8.with_bit(SIO, self.rtc.sio_output()) & !self.direction;
                u16::from(self.data & self.direction | inputs)
            }
            2 => u16::from(self.direction),
            4 => u16::from(self.readable),
            _ => 0,
        }
    }

    pub fn write(&mut self, offset: u32, value: u16) {
        match offset {
            0 => {
                self.data = value.bits(0..4) as u8;
                self.rtc.update(self.data & self.direction);
            }
            2 => self.direction = value.bits(0..4) as u8,
            4 => self.readable = value.bit(0),
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transfer {
    Command,
    Reading,
    Writing,
}

impl Transfer {
    const ALL: [Transfer; 3] = [Transfer::Command, Transfer::Reading, Transfer::Writing];
}

pub struct Rtc {
    unix_seconds: u64,
    offset_seconds: i64,
    status: u8,
    pins: u8,
    transfer: Transfer,
    bit_count: u32,
    byte: u8,
    command: u8,
    buffer: [u8; 7],
    buffer_length: usize,
    buffer_index: usize,
    sio_output: bool,
}

impl Rtc {
    fn new() -> Rtc {
        Rtc {
            unix_seconds: 946_684_800,
            offset_seconds: 0,
            status: 0,
            pins: 0,
            transfer: Transfer::Command,
            bit_count: 0,
            byte: 0,
            command: 0,
            buffer: [0; 7],
            buffer_length: 0,
            buffer_index: 0,
            sio_output: false,
        }
    }

    pub fn set_time(&mut self, unix_seconds: u64) {
        self.unix_seconds = unix_seconds;
    }

    fn sio_output(&self) -> bool {
        self.sio_output
    }

    fn save_state(&self, writer: &mut Writer) {
        writer.u64(self.unix_seconds);
        writer.i64(self.offset_seconds);
        writer.u8(self.status);
        writer.u8(self.pins);
        writer.u8(self.transfer as u8);
        writer.u32(self.bit_count);
        writer.u8(self.byte);
        writer.u8(self.command);
        writer.bytes(&self.buffer);
        writer.u8(self.buffer_length as u8);
        writer.u8(self.buffer_index as u8);
        writer.bool(self.sio_output);
    }

    fn load_state(&mut self, reader: &mut Reader) -> Result<(), StateError> {
        self.unix_seconds = reader.u64()?;
        self.offset_seconds = reader.i64()?;
        self.status = reader.u8()?;
        self.pins = reader.u8()?;
        self.transfer = *Transfer::ALL.get(usize::from(reader.u8()?)).ok_or(StateError::Corrupt)?;
        self.bit_count = reader.u32()?;
        self.byte = reader.u8()?;
        self.command = reader.u8()?;
        reader.bytes_into(&mut self.buffer)?;
        self.buffer_length = usize::from(reader.u8()?) % 8;
        self.buffer_index = usize::from(reader.u8()?) % 8;
        self.sio_output = reader.bool()?;
        Ok(())
    }

    fn update(&mut self, pins: u8) {
        let previous = self.pins;
        self.pins = pins;
        if !pins.bit(CS) || !previous.bit(CS) {
            self.transfer = Transfer::Command;
            self.bit_count = 0;
            self.byte = 0;
        } else if pins.bit(SCK) && !previous.bit(SCK) {
            self.clock(pins.bit(SIO));
        }
    }

    fn clock(&mut self, sio: bool) {
        match self.transfer {
            Transfer::Command => {
                self.byte = self.byte.with_bit(self.bit_count, sio);
                self.bit_count += 1;
                if self.bit_count == 8 {
                    self.start_command(self.byte);
                }
            }
            Transfer::Reading => {
                if self.buffer_index < self.buffer_length {
                    self.sio_output = self.buffer[self.buffer_index].bit(self.bit_count);
                    self.bit_count += 1;
                    if self.bit_count == 8 {
                        self.bit_count = 0;
                        self.buffer_index += 1;
                    }
                }
            }
            Transfer::Writing => {
                self.byte = self.byte.with_bit(self.bit_count, sio);
                self.bit_count += 1;
                if self.bit_count == 8 {
                    if self.buffer_index < self.buffer_length {
                        self.buffer[self.buffer_index] = self.byte;
                        self.buffer_index += 1;
                    }
                    self.bit_count = 0;
                    self.byte = 0;
                    if self.buffer_index == self.buffer_length {
                        self.finish_write();
                    }
                }
            }
        }
    }

    fn start_command(&mut self, byte: u8) {
        let byte = if byte.bits(4..8) == 0x6 { byte } else { byte.reverse_bits() };
        self.command = byte.bits(1..4);
        self.bit_count = 0;
        self.byte = 0;
        self.buffer_index = 0;
        self.buffer_length = match self.command {
            COMMAND_STATUS => 1,
            COMMAND_DATE_TIME => 7,
            COMMAND_TIME => 3,
            _ => 0,
        };
        if byte.bit(0) {
            self.transfer = Transfer::Reading;
            self.fill_buffer();
        } else {
            self.transfer = Transfer::Writing;
            if self.command == COMMAND_RESET {
                self.status = 0;
                self.offset_seconds = 0;
            }
        }
    }

    fn fill_buffer(&mut self) {
        let time = self.current_time();
        match self.command {
            COMMAND_STATUS => self.buffer[0] = self.status,
            COMMAND_DATE_TIME => self.buffer = time,
            COMMAND_TIME => self.buffer[..3].copy_from_slice(&time[4..7]),
            _ => {}
        }
    }

    fn finish_write(&mut self) {
        match self.command {
            COMMAND_STATUS => self.status = self.buffer[0] & STATUS_WRITABLE,
            COMMAND_DATE_TIME => self.set_written_time(self.buffer),
            COMMAND_TIME => {
                let mut time = self.current_time();
                time[4..7].copy_from_slice(&self.buffer[..3]);
                self.set_written_time(time);
            }
            _ => {}
        }
    }

    fn current_time(&self) -> [u8; 7] {
        let seconds = (self.unix_seconds as i64 + self.offset_seconds).max(0) as u64;
        let (year, month, day, weekday) = civil_from_days(seconds / 86_400);
        let second_of_day = seconds % 86_400;
        let hour = (second_of_day / 3600) as u8;
        let hour = if self.status & STATUS_24_HOUR != 0 { bcd(hour) } else { bcd(hour % 12).with_bit(7, hour >= 12) };
        [
            bcd((year % 100) as u8),
            bcd(month),
            bcd(day),
            weekday,
            hour,
            bcd((second_of_day / 60 % 60) as u8),
            bcd((second_of_day % 60) as u8),
        ]
    }

    fn set_written_time(&mut self, time: [u8; 7]) {
        let hour = if self.status & STATUS_24_HOUR != 0 {
            from_bcd(time[4].bits(0..6))
        } else {
            from_bcd(time[4].bits(0..5)) + if time[4].bit(7) { 12 } else { 0 }
        };
        let days = days_from_civil(2000 + u64::from(from_bcd(time[0])), from_bcd(time[1]), from_bcd(time[2]));
        let written = days * 86_400 + u64::from(hour) * 3600 + u64::from(from_bcd(time[5])) * 60 + u64::from(from_bcd(time[6]));
        self.offset_seconds = written as i64 - self.unix_seconds as i64;
    }
}

fn bcd(value: u8) -> u8 {
    (value / 10) << 4 | value % 10
}

fn from_bcd(value: u8) -> u8 {
    value.bits(4..8) * 10 + value.bits(0..4)
}

fn civil_from_days(days: u64) -> (u64, u8, u8, u8) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u8;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 } as u8;
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    let weekday = ((days + 4) % 7) as u8;
    (year as u64, month, day, weekday)
}

fn days_from_civil(year: u64, month: u8, day: u8) -> u64 {
    let year = year as i64 - if month <= 2 { 1 } else { 0 };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted_month = if month > 2 { month as i64 - 3 } else { month as i64 + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era - 719_468) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN_SCK: u8 = 1 << SCK;
    const PIN_SIO: u8 = 1 << SIO;
    const PIN_CS: u8 = 1 << CS;

    #[test]
    fn test_civil_conversion() {
        assert_eq!(civil_from_days(0), (1970, 1, 1, 4));
        assert_eq!(civil_from_days(days_from_civil(2005, 9, 2)), (2005, 9, 2, 5));
        assert_eq!(civil_from_days(days_from_civil(2000, 2, 29)), (2000, 2, 29, 2));
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    fn send_byte(gpio: &mut Gpio, byte: u8) {
        for i in (0..8).rev() {
            let sio = if byte >> i & 1 != 0 { PIN_SIO } else { 0 };
            gpio.write(0, (PIN_CS | sio) as u16);
            gpio.write(0, (PIN_CS | PIN_SCK | sio) as u16);
        }
    }

    fn send_data_byte(gpio: &mut Gpio, byte: u8) {
        for i in 0..8 {
            let sio = if byte >> i & 1 != 0 { PIN_SIO } else { 0 };
            gpio.write(0, (PIN_CS | sio) as u16);
            gpio.write(0, (PIN_CS | PIN_SCK | sio) as u16);
        }
    }

    fn receive_byte(gpio: &mut Gpio) -> u8 {
        let mut byte = 0;
        for i in 0..8 {
            gpio.write(0, PIN_CS as u16);
            gpio.write(0, (PIN_CS | PIN_SCK) as u16);
            byte |= ((gpio.read(0) as u8 & PIN_SIO != 0) as u8) << i;
        }
        byte
    }

    #[test]
    fn test_date_time_read_in_24_hour_mode() {
        let mut gpio = Gpio::new();
        gpio.rtc.set_time(days_from_civil(2005, 9, 2) * 86_400 + 13 * 3600 + 45 * 60 + 30);
        gpio.write(4, 1);
        gpio.write(2, (PIN_SCK | PIN_SIO | PIN_CS) as u16);
        gpio.write(0, 0);
        send_byte(&mut gpio, 0x62);
        send_data_byte(&mut gpio, STATUS_24_HOUR);
        gpio.write(0, 0);
        send_byte(&mut gpio, 0x65);
        gpio.write(2, (PIN_SCK | PIN_CS) as u16);
        let time: Vec<u8> = (0..7).map(|_| receive_byte(&mut gpio)).collect();
        gpio.write(0, 0);
        assert_eq!(time, [0x05, 0x09, 0x02, 5, 0x13, 0x45, 0x30]);
    }

    #[test]
    fn test_reversed_command_byte_is_accepted() {
        let mut gpio = Gpio::new();
        gpio.write(4, 1);
        gpio.write(2, (PIN_SCK | PIN_SIO | PIN_CS) as u16);
        gpio.write(0, 0);
        send_byte(&mut gpio, 0x65u8.reverse_bits());
        gpio.write(2, (PIN_SCK | PIN_CS) as u16);
        let year = receive_byte(&mut gpio);
        assert_eq!(year, 0x00);
    }
}
