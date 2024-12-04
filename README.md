# GBAE - Game Boy Advance Emulator

A Game Boy Advance emulator written in Rust, focusing on accuracy and maintainability.

## Features

- ARM7TDMI CPU emulation
  - ARM and Thumb instruction sets
  - Accurate CPU flag handling
  - CPU pipeline simulation
- Memory system
  - BIOS ROM
  - Game ROM (cartridge)
  - Work RAM
  - Memory-mapped I/O

## Building

Requires Rust and Cargo. To build:

```bash
cargo build
```

For optimized release build:

```bash
cargo build --release
```

## Running

To run the emulator:

```bash
cargo run -- path/to/rom.gba
```

## Testing

Run the test suite:

```bash
cargo test
```

## Project Structure

- `src/`
  - `system/` - Core emulation components
    - `cpu.rs` - ARM7TDMI CPU implementation
    - `memory.rs` - Memory subsystem
    - `instructions/` - CPU instruction implementations
  - `bitutil.rs` - Bit manipulation utilities
  - `cartridge.rs` - Game ROM handling
  - `main.rs` - Entry point

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- [GBATEK](https://problemkaputt.de/gbatek.htm) - Technical documentation
- [ARM7TDMI Technical Reference Manual](https://documentation-service.arm.com/static/5e8e353cef2d0b5d1f41a560) - CPU documentation
