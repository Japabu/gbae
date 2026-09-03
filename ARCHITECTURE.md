# Architecture

## Crate layout

`gbae` is a library crate with the emulator core and an optional binary
(feature `frontend`) that puts a window, audio and input around it. Tests and
examples use the library headlessly through `system::gba::Gba`.

## The machine

`Gba` owns the `CPU`, the `Memory` and the `PPU` and drives them from a small
scheduler. `step()` runs one instruction (or, while the CPU is halted, jumps to
the next event) and then advances timers, the APU and DMA by the cycles that
passed. Two events exist per scanline: the start of HBlank at cycle 960, where
the line is rendered and HBlank DMA and IRQs fire, and the start of the next
line at cycle 1232, where VCOUNT, DISPSTAT flags, VBlank IRQ and VBlank DMA
are handled.

## Instructions

Every instruction family lives in one file under `system/instructions`
(`data_processing`, `load_store`, `load_store_multiple`, `branch`, `multiply`,
`ctrl_ext`, `swi`). Each file has a plain data struct, `decode_*` functions
that build it from an instruction word, an `execute` method and a
`disassemble` method. `instructions/mod.rs` gathers the structs in `enum
Instruction`, and `decode_arm` / `decode_thumb` there are the only place where
the encoding is described: a table of bit patterns such as `"000xxxxx 1xx1"`
that a const fn turns into masks at compile time.

Dispatch is constant-time. `lut.rs` builds two static tables,
`ARM_LUT[4096]` indexed by instruction bits 27-20 and 7-4, and
`THUMB_LUT[1024]` indexed by bits 15-6. Entry `i` is `execute_arm::<i>`, a
generic function that puts the index bits back into the instruction word and
calls the ordinary decoder and executor. Because the index is a compile-time
constant the compiler folds the pattern table and the decoded fields, so each
entry ends up as a specialised handler without a second description of the
instruction anywhere.

## CPU

`cpu.rs` keeps the current register file as a flat array and swaps the banked
registers in and out when the mode changes, so register access is an array
index. It models the two-stage fetch pipeline (`pipeline: [u32; 2]`), which is
what makes self-modifying code, BIOS read protection and PC-relative values
behave like the hardware. Condition codes are checked with a 16-entry table.
The CPU only counts cycles that `Memory` reports.

## Memory and timing

`memory.rs` maps addresses with `decode_address` into concrete buffers. CPU
accesses go through `fetch_*`, `load_*` and `store_*`, which charge cycles by
region, width and sequentiality, decode WAITCNT for ROM and SRAM, and model the
ROM prefetch buffer. Plain `read_*` / `write_*` are untimed and used by DMA,
the PPU, the debugger tools and tests. Instructions add their internal cycles
through `Memory::idle`, DMA transfers stall the CPU by their documented cost,
and unmapped or protected reads return the last prefetched opcode.

IO registers are a struct with one direct `match` per access width; timers
keep a budget of cycles until the next possible overflow so that they are only
recomputed when it matters; DMA channels latch their addresses on enable and
run on immediate, VBlank, HBlank or FIFO triggers.

## PPU

`ppu.rs` renders one scanline at a time into per-layer line buffers: text
backgrounds with scrolling, sizes, flips and mosaic; affine backgrounds; the
bitmap modes through the same affine transform; sprites with all shapes,
affine and double-size modes, both mapping modes, the OBJ window, the per-line
cycle budget and mosaic. Composition takes the two topmost visible layers per
pixel according to a per-line priority order, applies window rules and colour
effects, and writes RGB888 into the framebuffer.

## APU

`apu.rs` owns the sound registers. The PSG channels advance by elapsed cycles
between output samples, a 512 Hz frame sequencer drives length, envelope and
sweep, the two FIFOs pop a sample on their timer's overflow and ask `Memory`
for a DMA refill when half empty, and the mixer applies volumes and SOUNDBIAS.
Output is 48 kHz stereo, taken by the frontend once per frame.

## Saves, RTC, states

`save.rs` detects the save type from the ROM's ID string and implements SRAM,
64K and 128K flash command sets and the serial EEPROM protocol. `rtc.rs`
exposes the cartridge GPIO pins with an S-3511 clock behind them, fed with the
host time by the frontend. `state.rs` is a tiny versioned binary format; every
component has `save_state` / `load_state`, and a state records the ROM length
and hash so it is only applied to the same game.

## Frontend

`main.rs` runs the emulator on the winit event loop, paced to the GBA frame
rate (or unthrottled in turbo), presents the frame through `softbuffer` with
integer scaling, forwards keyboard events through the configurable mapping,
streams audio through `cpal` and writes `.sav` files when the game changes its
save data. `menu.rs` draws the Escape menu into the frame with the bitmap font
in `font.rs`; `config.rs` reads and writes `gbae.cfg`.

## Testing

Unit tests sit next to the code. `tests/` holds integration tests that drive
the headless machine: PPU behaviour against expected pixels and golden PNGs,
DMA and timer semantics, cycle counts from the ARM7TDMI datasheet, saves,
audio, input, save states, and boot goldens for the official BIOS and Pokémon
Emerald. `tests/roms.rs` runs jsmolka's gba-tests and FuzzARM and compares the
final screen with recorded pass screens.
