# gbae

A Game Boy Advance emulator in Rust, MIT licensed, with its own BIOS.

The core (`gbae` library) is a cycle-counting ARM7TDMI, the full memory map
with wait states and the ROM prefetch buffer, a scanline PPU with every
background mode, sprites, windows and colour effects, four PSG channels plus
Direct Sound, DMA, timers, flash/SRAM/EEPROM saves and the cartridge clock.
Its only dependency is `seq-macro`. The window build adds `winit`,
`softbuffer` and `cpal`.

## Running

```
cargo run --release              # loads rom.gba from the current directory
cargo run --release -- game.gba  # or a specific ROM
```

Without a ROM the Escape menu opens on a file browser. Nothing else is
needed: the emulator boots games with its built-in BIOS. A `gba_bios.bin`
next to the executable or `GBA_BIOS=path` selects an original BIOS image
instead, which brings back the boot logo and its jingle.

| GBA | key |
|---|---|
| D-pad | arrow keys |
| A / B | Z / X |
| L / R | A / S |
| Start / Select | Enter / Backspace |
| turbo | Tab |
| menu | Escape |

The menu offers resume, reset, save state, load state, a ROM browser, volume,
the turbo speed and key mapping; settings persist in `gbae.cfg`. Tab toggles
turbo: at 1.5x to 4x the sound plays along, pitched up like a tape running
fast; at Max the machine runs as fast as it can and is silent. Save data goes
to `<rom>.sav` next to the ROM, save states to `<rom>.state`.

## Tests and tools

```
cargo test
```

Everything the tests need is generated in the tests: machines boot the
built-in BIOS into ROMs assembled from `Instruction` values, so a fresh clone
runs the whole suite without downloading anything. Golden-frame tests compare
rendered frames with PNGs in `tests/golden`; run with `UPDATE_GOLDEN=1` to
re-record after checking the new image.

```
cargo run --release --example render -- - rom.gba 400 frame.png    # "-" is the built-in BIOS
cargo run --release --example trace -- - rom.gba 1000 break=0x08000100
cargo run --release --example bench -- - rom.gba 400
```

## Layout

```
src/bits.rs                   Bits trait: bit and bit-range access on integers
src/system/instructions/      one file per instruction family: decode, execute, encode, disassemble
src/system/instructions/asm   assembler with labels and literal pools, built on the encoders
src/system/cpu.rs             registers, banks, pipeline, exceptions
src/system/memory/            bus (wait states, prefetch), IO registers, timers, DMA, address map
src/system/bios.rs            built-in BIOS: assembled reset and IRQ code, native BIOS functions
src/system/ppu.rs             scanline renderer
src/system/apu.rs, synth.rs   sound channels, mixer, band-limited synthesis
src/system/save.rs, rtc.rs    SRAM, flash, EEPROM; GPIO with the S-3511 clock
src/system/state.rs           save-state format
src/system/gba.rs             the machine and its scanline scheduler
src/main.rs, menu.rs, audio.rs, config.rs, font.rs   window build
```

## The machine

`Gba` owns the `CPU`, the `Memory` and the `PPU`. Each step runs one
instruction, or skips to the next event while the CPU is halted, then hands
the elapsed cycles to memory, which advances timers, the APU and DMA. Two
events per scanline drive the display; the picture the frontend sees is the
one finished at the last VBlank, so it is always a whole frame.

```
        one scanline, 1232 cycles, 228 per frame (160 visible + 68 blank)
 |<-------------------- draw, 960 -------------------->|<----- hblank, 272 ----->|
 ^                                                     ^
 start_scanline                                        start_hblank
   VCOUNT, DISPSTAT flags, VCOUNT match IRQ              HBLANK flag and IRQ
   line 160: VBLANK flag, VBLANK IRQ, VBlank DMA,        lines 0..159: render line,
             picture finished
                                                         HBlank DMA

 CPU ──instruction──► Memory.tick(cycles) ──► timers ──overflow──► APU FIFO pop ──► DMA refill
                                          └─► APU (batched, exact at timer overflows)
```

## Instructions

Every family lives in one file under `system/instructions`: a plain data
struct, `decode_*` functions building it from an instruction word,
`execute`, `encode_arm` / `encode_thumb` and `disassemble`. The encoders are
checked against the decoders by round-trip tests over all 4096 ARM lookup
entries and all 65536 Thumb words, and the assembler in `asm.rs` builds on
them.

```
 ARM word                                THUMB word
 bits 27..20 and 7..4 ──► 12-bit index   bits 15..6 ──► 10-bit index
                            │                              │
                     ARM_LUT[index]                  THUMB_LUT[index]
                            │                              │
             execute_arm::<INDEX>(word)     execute_thumb::<INDEX>(word)
                            │                              │
     put INDEX bits back, decode with the pattern table, execute
              "000xxxxx 1xx1" ─► load_store::decode_extra_arm
              "00010010 0001" ─► branch::decode_bx_arm
              "101xxxxx xxxx" ─► branch::decode_b_arm / decode_bl_arm
                            ▼
                  enum Instruction { DataProcessing, LoadStore, Branch, ... }
```

Because the index is a compile-time constant in each handler, the compiler
folds the pattern table and the decoded fields, so every table entry becomes
a specialised handler without a second description of any instruction.

## CPU

`cpu.rs` keeps the register file as a flat array and swaps banked registers
when the mode changes, so a register access is one index. `Register`, `Mode`
and `Psr` are types, not bare integers. The two-stage fetch pipeline is what
makes self-modifying code, BIOS read protection and PC-relative values behave
like the hardware; conditions are checked with a 16-entry table.

## Memory

`memory/mod.rs` maps addresses to regions. CPU accesses go through
`fetch_*`, `load_*` and `store_*`, which charge the bus for wait states,
sequential access and the ROM prefetch buffer; plain `read_*` / `write_*` are
untimed and used by DMA, the PPU, the BIOS functions and tests. Unmapped or
protected reads return the last fetched opcode.

```
 00000000 BIOS 16K       04000000 IO registers     08000000 ROM, wait state 0
 02000000 EWRAM 256K     05000000 palette 1K       0A000000 ROM, wait state 1
 03000000 IWRAM 32K      06000000 VRAM 96K         0C000000 ROM, wait state 2
                         07000000 OAM 1K           0E000000 SRAM / flash

 fetch / load / store ─► Bus: wait states, prefetch, cycle count
 read / write ──────────► Region: Bios | Ram(kind) | Io | Rom | Gpio | Eeprom | Backup | Unmapped
```

The IO registers decode their fields through the `Bits` trait with DMA and
timer registers indexed by channel. Timers keep a budget of cycles until the
next overflow and report overflows and IRQ requests to their owner; memory
splits its cycle blocks at those overflows so Direct Sound samples land on
their exact cycle. DMA channels latch their addresses on enable and run on
immediate, VBlank, HBlank or FIFO triggers.

## PPU

`ppu.rs` renders one scanline at a time into per-layer line buffers of
`Option<Color>`: text backgrounds with scrolling, sizes, flips and mosaic;
affine backgrounds; the bitmap modes through the same affine transform;
sprites with all shapes, affine and double-size modes, both mapping modes,
the OBJ window, the per-line cycle budget and mosaic. Composition takes the
two topmost visible layers per pixel by priority, applies window rules and
colour effects, and writes RGB888 into the framebuffer.

## APU

```
 square 1, square 2, wave, noise ── evaluated on a 64-cycle grid ─┐
 FIFO A, FIFO B ── popped on timer overflow, refilled by DMA ─────┤
                                                                   ▼
                                      mixer: bias, volumes, clamp to 10 bits
                                                                   │ level changed at cycle t
                                                                   ▼
                   band-limited step: 32-tap windowed sinc, 256 phases, device rate
                                                                   ▼
                          15 Hz DC blocker ──► i16 stereo ──► audio device
```

The synthesizer generates at whatever rate the frontend's device reports, so
nothing is resampled anywhere, and every step lands at the cycle the game
caused it. Turbo asks for a lower rate, which plays the same sound faster.

## BIOS

`bios.rs` assembles a 16 KB image with the reset sequence and the interrupt
dispatcher, and implements the BIOS functions natively when that image is in
use: reset, halt and IntrWait, division, square root and arc tangent, memory
copies, affine set-up, bit unpacking and the LZ77, Huffman, run-length and
difference decoders. An original BIOS image is emulated as ordinary code.

## Saves, clock, states

`save.rs` detects the save type from the ROM's ID string and implements SRAM,
64K and 128K flash command sets and the EEPROM protocol. `rtc.rs` exposes
the cartridge GPIO pins with an S-3511 clock behind them, fed with the host
time. `state.rs` is a versioned binary format; every component has
`save_state` / `load_state`, and a state records the ROM length and hash so
it is only applied to the same game.

## Frontend

`main.rs` runs the emulator on the winit event loop, paced to the GBA frame
rate, a multiple of it in turbo or unthrottled at Max, presents frames
through `softbuffer` with integer scaling, maps keyboard events through the
configurable bindings, streams audio through `cpal` at the device's native
rate and writes `.sav` files when the game changes its save data. `menu.rs`
draws the Escape menu with the bitmap font in `font.rs`; `config.rs` reads
and writes `gbae.cfg`.

## Acknowledgments

- [GBATEK](https://problemkaputt.de/gbatek.htm) and the ARM7TDMI technical reference manual
- the public domain [font8x8](https://github.com/dhepper/font8x8) used by the menu
