# Architecture Overview

## Design Philosophy

**Simplicity and cleanliness**: The entire emulator is a single binary with MCP functionality built-in. No complex multi-crate setup, no stdio redirection - just a clean WebSocket server.

## Structure

```
gbae/
├── src/
│   ├── main.rs           # 96 lines - Entry point, emulator loop
│   ├── mcp.rs            # 415 lines - Complete MCP server
│   ├── debugger.rs       # 83 lines - Clean command execution
│   ├── system/           # Emulator core
│   │   ├── cpu.rs
│   │   ├── memory.rs
│   │   ├── ppu.rs
│   │   └── ...
│   └── ...
├── Cargo.toml            # Single package, no workspace
├── README.md             # Complete documentation
└── ARCHITECTURE.md       # This file
```

## MCP Implementation

### Transport: WebSocket

**Why WebSocket over stdio?**

1. **Hot reloading**: Restart emulator without restarting Claude Code
2. **Network flexibility**: Can run emulator remotely
3. **Clean separation**: No process spawning, no pipe management
4. **Stateless**: Connection can drop and reconnect freely

### Protocol: JSON-RPC 2.0

Simple and standard. Example request:

```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "step",
    "arguments": {"count": 10}
  },
  "id": 1
}
```

### Tools (6 total)

All tools follow the same pattern:
1. Parse arguments from JSON
2. Send command through channel
3. Await response
4. Format as MCP result

```rust
DebugCommand → Channel → Debugger → CPU/Memory
                            ↓
                    DebugResponse → JSON
```

## Emulator Flow

### Startup Sequence

```rust
main() → tokio::Runtime
  ↓
run_emulator()
  ↓
  ├─→ spawn MCP server (port 3000)
  └─→ run_headless()
        ↓
        ├─→ Initialize CPU, Memory, PPU
        ├─→ Start HALTED
        └─→ loop:
              ├─→ handle_mcp_commands() (non-blocking)
              └─→ if running: cpu.cycle()
```

### State Management

- **Halted by default**: Emulator starts paused
- **Command-driven**: All execution controlled via MCP
- **Non-blocking**: MCP commands processed between CPU cycles

## Key Design Decisions

### 1. Single Binary

**Rationale**: No need for workspace complexity. Everything in one place.

**Benefits**:
- Simpler build process
- Easier to understand
- No version mismatches
- Faster compilation

### 2. Network-Based MCP

**Rationale**: stdio-based MCP can't be reloaded without restarting the parent process.

**Benefits**:
- Hot reload workflow
- Independent processes
- Better debugging
- Network transparency

### 3. Halted Start

**Rationale**: Claude needs to inspect state before execution begins.

**Benefits**:
- Inspect initial CPU state
- Set up observations
- Controlled execution
- No race conditions

### 4. Headless Operation

**Rationale**: Focus on autonomous debugging, not interactive play.

**Benefits**:
- Simpler code
- Faster execution
- Better for CI/CD
- Less dependencies

## Communication Flow

```
┌─────────────┐                    ┌──────────────┐
│ Claude Code │◄──WebSocket (JSON)─┤ MCP Server   │
└─────────────┘                    │ (Axum)       │
                                   └──────┬───────┘
                                          │
                                   ┌──────▼───────┐
                                   │ Command      │
                                   │ Channel      │
                                   │ (mpsc)       │
                                   └──────┬───────┘
                                          │
                                   ┌──────▼───────┐
                                   │ Debugger     │
                                   │              │
                                   └──────┬───────┘
                                          │
                         ┌────────────────┼────────────────┐
                         ▼                ▼                ▼
                    ┌────────┐      ┌─────────┐      ┌────────┐
                    │  CPU   │      │ Memory  │      │  PPU   │
                    └────────┘      └─────────┘      └────────┘
```

## Future Enhancements

Potential additions (maintaining simplicity):

### High Priority
- [ ] Screenshot tool (return framebuffer as base64 PNG)
- [ ] Halt/pause tool (stop execution)
- [ ] Breakpoint support (halt at address)

### Medium Priority
- [ ] Memory write tool
- [ ] Register write tool
- [ ] Execution speed control

### Low Priority
- [ ] Async event notifications (register changes, breakpoints hit)
- [ ] Display mode (optional GUI)
- [ ] Recording/playback

## Code Quality

### Current State
- **Clean separation**: MCP, Debugger, Emulator are independent
- **Minimal dependencies**: Only what's needed
- **Type safety**: Strong typing throughout
- **Error handling**: Result types, no panics in MCP layer

### Metrics
- **Main binary**: ~400 lines (mcp.rs) + ~100 lines (main.rs/debugger.rs)
- **Dependencies**: 16 (down from 20+ with separate crates)
- **Build time**: <20s release, <2s dev
- **Binary size**: ~12MB debug, ~5MB release

## Testing Strategy

### Manual Testing
1. Start emulator: `cargo run`
2. Connect Claude Code
3. Use tools to verify functionality

### Future: Integration Tests
- Mock WebSocket client
- Test each tool
- Verify command flow
- Test error cases

## Performance Considerations

### Non-Blocking Design
- MCP commands don't block emulation
- Commands processed between cycles
- Responses sent asynchronously

### Memory Efficiency
- Shared framebuffer (Arc<RwLock>)
- Command channel (unbounded, minimal overhead)
- No polling loops

### CPU Usage
- Sleep when halted (10ms intervals)
- Run full speed when executing
- WebSocket is event-driven

## Comparison: Before vs After

### Before (3-crate workspace)
```
gbae/
├── src/lib.rs (re-exports)
├── src/main.rs (complex modes)
├── ipc/
│   ├── Cargo.toml
│   └── src/lib.rs (types only)
└── mcp-server/
    ├── Cargo.toml
    ├── src/lib.rs (rmcp integration)
    └── src/main.rs (stdio transport)
```

**Issues**:
- Complex workspace setup
- stdio transport (no hot reload)
- 3 separate crates
- Re-export confusion

### After (single binary)
```
gbae/
├── src/
│   ├── main.rs (clean startup)
│   ├── mcp.rs (self-contained)
│   └── debugger.rs (simple)
└── Cargo.toml (single package)
```

**Benefits**:
- One package, one binary
- WebSocket transport (hot reload!)
- Self-contained MCP implementation
- Clean, understandable structure

## Summary

This architecture prioritizes:
1. **Simplicity**: Everything in one binary
2. **Hot reload**: Network-based MCP
3. **Cleanliness**: Minimal, focused code
4. **Autonomy**: Designed for Claude Code's autonomous debugging workflow

The result is a ~500 line addition to the emulator that provides full MCP debugging capabilities with hot reload support.
