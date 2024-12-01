# GBA Emulator with MCP Support

A Game Boy Advance emulator with built-in Model Context Protocol (MCP) server for autonomous debugging with Claude Code.

## Quick Start

```bash
# Run the emulator (starts with MCP server on port 3000)
cargo run

# Use custom port
GBA_MCP_PORT=3001 cargo run
```

The emulator will:
- Load `gba_bios.bin` and `rom.gba` from the current directory
- Start in **HALTED** state (no execution)
- Start MCP WebSocket server on `ws://127.0.0.1:3000/mcp` (or custom port)

## Architecture

### Clean and Simple Design

The emulator is a single binary with all MCP functionality built-in:

- **No external crates**: All MCP code is in `src/mcp.rs`
- **WebSocket-based**: Uses Axum for clean WebSocket transport
- **Stateless connection**: Kill and restart emulator anytime, Claude Code reconnects automatically
- **Headless operation**: No display, pure debugging focus

### Hot Reloading Workflow

1. Run emulator: `cargo run`
2. Claude Code connects via WebSocket
3. Make code changes
4. Kill emulator (Ctrl+C)
5. Run again: `cargo run`
6. Claude Code auto-reconnects with new tools available!

## MCP Tools

The emulator provides 6 debugging tools through MCP:

### `continue_execution`
Resume execution from halted state.

### `step`
Step forward by N instructions (default: 1).

### `read_memory`
Read memory at an address (hex format like "0x06000000").

### `read_register`
Read a CPU register (0-15, or "pc", "sp", "lr").

### `get_cpu_state`
Get all CPU registers and CPSR.

### `read_palette`
Read palette RAM (first 32 colors shown).

## Claude Code Setup

The emulator uses a **network-based MCP connection** instead of stdio, which allows hot reloading.

Add to your MCP configuration:

```json
{
  "mcpServers": {
    "gba-debugger": {
      "transport": "websocket",
      "url": "ws://127.0.0.1:3000/mcp"
    }
  }
}
```

## Features

- ARM7TDMI CPU emulation
  - ARM and Thumb instruction sets
  - CPU modes and banked registers
  - Condition code flags
- Memory system
  - BIOS ROM, Work RAM, VRAM, Palette RAM
  - Memory-mapped I/O registers
  - Game ROM (cartridge) support
- Picture Processing Unit (PPU)
  - Mode 0 background rendering
  - 4bpp tile support
- MCP Server
  - WebSocket transport for hot reloading
  - 6 debugging tools
  - JSON-RPC 2.0 protocol

## Building

```bash
cargo build
cargo build --release
```

## Autonomous Debugging Example

With Claude Code connected:

1. **Check initial state**: Use `get_cpu_state` to see CPU registers
2. **Inspect BIOS**: Use `read_memory` with address "0x00000000"
3. **Step through code**: Use `step` with count
4. **Continue execution**: Use `continue_execution` to run
5. **Check graphics**: Use `read_palette` and `read_memory` on VRAM

Claude can autonomously debug issues like:
- Why the Nintendo logo isn't displaying
- Incorrect palette colors
- CPU stuck in loops
- Memory access issues

## Acknowledgments

- [GBATEK](https://problemkaputt.de/gbatek.htm) - Technical documentation
- [ARM7TDMI Technical Reference Manual](https://documentation-service.arm.com/static/5e8e353cef2d0b5d1f41a560) - CPU documentation
