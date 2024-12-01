use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

// Debug commands that can be sent to the emulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugCommand {
    Step { count: u32 },
    Continue,
    Halt,
    ReadMemory { address: u32, length: u32 },
    ReadRegister { register: u8 },
    GetCpuState,
    GetPalette,
    GetScreenshot,
    Disassemble { address: u32, count: u32, mode: Option<String> },
    AddBreakpoint { address: u32 },
    RemoveBreakpoint { address: u32 },
    ListBreakpoints,
}

// Responses from the emulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugResponse {
    StepComplete { instructions: u32 },
    ContinueStarted,
    HaltComplete { pc: u32 },
    MemoryData { address: u32, data: Vec<u8> },
    RegisterValue { register: u8, value: u32 },
    CpuState { registers: [u32; 16], cpsr: u32, pc: u32 },
    PaletteData { data: Vec<u16> },
    Screenshot { width: u32, height: u32, rgba_data: Vec<u8> },
    Disassembly { instructions: Vec<String> },
    BreakpointAdded { address: u32 },
    BreakpointRemoved { address: u32 },
    BreakpointList { breakpoints: Vec<u32> },
    Error { message: String },
}

// Command request with response channel
pub struct CommandRequest {
    pub command: DebugCommand,
    pub response_tx: tokio::sync::oneshot::Sender<DebugResponse>,
}

pub type CommandSender = mpsc::UnboundedSender<CommandRequest>;
pub type CommandReceiver = mpsc::UnboundedReceiver<CommandRequest>;

pub fn create_channel() -> (CommandSender, CommandReceiver) {
    mpsc::unbounded_channel()
}

// MCP Server state
#[derive(Clone)]
struct McpState {
    command_tx: Arc<Mutex<CommandSender>>,
}

// Start MCP server on the given port
pub async fn start_mcp_server(port: u16, command_tx: CommandSender) -> Result<(), Box<dyn std::error::Error>> {
    let state = McpState {
        command_tx: Arc::new(Mutex::new(command_tx)),
    };

    let app = Router::new()
        .route("/mcp", post(mcp_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{}", port);
    eprintln!("MCP server listening on http://{}/mcp", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// HTTP POST handler for MCP JSON-RPC
async fn mcp_handler(
    State(state): State<McpState>,
    Json(request): Json<Value>,
) -> Response {
    eprintln!("Received MCP request: {}", serde_json::to_string_pretty(&request).unwrap_or_default());
    let response = handle_mcp_message(&request.to_string(), &state).await;
    eprintln!("Sending MCP response: {}", response);

    // Return JSON response with proper Content-Type header
    use axum::http::header;
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        response,
    )
        .into_response()
}

// Handle a single MCP message
async fn handle_mcp_message(text: &str, state: &McpState) -> String {
    let request: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            return json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32700,
                    "message": format!("Parse error: {}", e)
                },
                "id": null
            })
            .to_string();
        }
    };

    let id = request.get("id").cloned();
    let method = request.get("method").and_then(|m| m.as_str());
    let params = request.get("params");

    // Handle notifications (no response needed)
    if method == Some("notifications/initialized") {
        eprintln!("Received initialized notification");
        return json!({}).to_string(); // Empty response for notification
    }

    let result = match method {
        Some("initialize") => handle_initialize(),
        Some("tools/list") => handle_tools_list(),
        Some("tools/call") => handle_tool_call(params, state).await,
        _ => {
            return json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {:?}", method)
                },
                "id": id
            })
            .to_string();
        }
    };

    json!({
        "jsonrpc": "2.0",
        "result": result,
        "id": id
    })
    .to_string()
}

// Handle initialize request
fn handle_initialize() -> Value {
    json!({
        "protocolVersion": "2025-06-18",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "gba-debugger",
            "version": "0.1.0"
        }
    })
}

// Handle tools/list request
fn handle_tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "continue_execution",
                "description": "Resume execution from halted state",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "halt_execution",
                "description": "Halt/pause execution immediately at current PC (works while running)",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "step",
                "description": "Step forward by N instructions (requires halted state)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "count": {
                            "type": "number",
                            "description": "Number of instructions to step (default: 1)"
                        }
                    }
                }
            },
            {
                "name": "read_memory",
                "description": "Read memory at a specific address (requires halted state)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Memory address (hex format like 0x06000000)"
                        },
                        "length": {
                            "type": "number",
                            "description": "Number of bytes to read"
                        }
                    },
                    "required": ["address", "length"]
                }
            },
            {
                "name": "read_register",
                "description": "Read a CPU register value (requires halted state)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "register": {
                            "type": "string",
                            "description": "Register number (0-15) or name (pc, sp, lr)"
                        }
                    },
                    "required": ["register"]
                }
            },
            {
                "name": "get_cpu_state",
                "description": "Get complete CPU state (all registers and CPSR) (requires halted state)",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "read_palette",
                "description": "Read palette RAM (requires halted state)",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_screenshot",
                "description": "Get the current screen output as a base64-encoded PNG image",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "disassemble",
                "description": "Disassemble instructions at a specific address (requires halted state)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Memory address (hex format like 0x00000230)"
                        },
                        "count": {
                            "type": "number",
                            "description": "Number of instructions to disassemble (default: 10)"
                        },
                        "mode": {
                            "type": "string",
                            "description": "Instruction set mode: 'arm', 'thumb', or 'auto' (default: auto uses CPU state)",
                            "enum": ["arm", "thumb", "auto"]
                        }
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "add_breakpoint",
                "description": "Add a breakpoint at a specific address",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Memory address (hex format like 0x00000244)"
                        }
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "remove_breakpoint",
                "description": "Remove a breakpoint at a specific address",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "address": {
                            "type": "string",
                            "description": "Memory address (hex format like 0x00000244)"
                        }
                    },
                    "required": ["address"]
                }
            },
            {
                "name": "list_breakpoints",
                "description": "List all active breakpoints",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

// Handle tools/call request
async fn handle_tool_call(params: Option<&Value>, state: &McpState) -> Value {
    let params = match params {
        Some(p) => p,
        None => return json!({"content": [{"type": "text", "text": "No parameters provided"}], "isError": true}),
    };

    let tool_name = match params.get("name").and_then(|n| n.as_str()) {
        Some(n) => n,
        None => return json!({"content": [{"type": "text", "text": "No tool name provided"}], "isError": true}),
    };

    let empty_json = json!({});
    let arguments = params.get("arguments").unwrap_or(&empty_json);

    match tool_name {
        "continue_execution" => execute_continue(state).await,
        "halt_execution" => execute_halt(state).await,
        "step" => execute_step(arguments, state).await,
        "read_memory" => execute_read_memory(arguments, state).await,
        "read_register" => execute_read_register(arguments, state).await,
        "get_cpu_state" => execute_get_cpu_state(state).await,
        "read_palette" => execute_read_palette(state).await,
        "get_screenshot" => execute_get_screenshot(state).await,
        "disassemble" => execute_disassemble(arguments, state).await,
        "add_breakpoint" => execute_add_breakpoint(arguments, state).await,
        "remove_breakpoint" => execute_remove_breakpoint(arguments, state).await,
        "list_breakpoints" => execute_list_breakpoints(state).await,
        _ => json!({"content": [{"type": "text", "text": format!("Unknown tool: {}", tool_name)}], "isError": true}),
    }
}

// Execute continue command
async fn execute_continue(state: &McpState) -> Value {
    match send_command(state, DebugCommand::Continue).await {
        Ok(DebugResponse::ContinueStarted) => {
            json!({"content": [{"type": "text", "text": "Continuing execution..."}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute halt command
async fn execute_halt(state: &McpState) -> Value {
    match send_command(state, DebugCommand::Halt).await {
        Ok(DebugResponse::HaltComplete { pc }) => {
            json!({"content": [{"type": "text", "text": format!("Execution halted at PC: 0x{:08X}", pc)}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute step command
async fn execute_step(args: &Value, state: &McpState) -> Value {
    let count = args.get("count").and_then(|c| c.as_u64()).unwrap_or(1) as u32;

    match send_command(state, DebugCommand::Step { count }).await {
        Ok(DebugResponse::StepComplete { instructions }) => {
            json!({"content": [{"type": "text", "text": format!("Stepped {} instruction(s)", instructions)}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute read_memory command
async fn execute_read_memory(args: &Value, state: &McpState) -> Value {
    let address_str = match args.get("address").and_then(|a| a.as_str()) {
        Some(a) => a,
        None => return json!({"content": [{"type": "text", "text": "Missing address parameter"}], "isError": true}),
    };

    let address = if address_str.starts_with("0x") {
        u32::from_str_radix(&address_str[2..], 16)
    } else {
        u32::from_str_radix(address_str, 16)
    };

    let address = match address {
        Ok(a) => a,
        Err(e) => return json!({"content": [{"type": "text", "text": format!("Invalid address: {}", e)}], "isError": true}),
    };

    let length = args.get("length").and_then(|l| l.as_u64()).unwrap_or(16) as u32;

    match send_command(state, DebugCommand::ReadMemory { address, length }).await {
        Ok(DebugResponse::MemoryData { address, data }) => {
            let mut output = format!("Memory at {:#010X} ({} bytes):\n", address, data.len());
            for (i, byte) in data.iter().enumerate() {
                if i % 16 == 0 {
                    output.push_str(&format!("\n{:#010X}: ", address + i as u32));
                }
                output.push_str(&format!("{:02X} ", byte));
            }
            json!({"content": [{"type": "text", "text": output}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute read_register command
async fn execute_read_register(args: &Value, state: &McpState) -> Value {
    let register_str = match args.get("register").and_then(|r| r.as_str()) {
        Some(r) => r,
        None => return json!({"content": [{"type": "text", "text": "Missing register parameter"}], "isError": true}),
    };

    let register = match register_str.to_lowercase().as_str() {
        "pc" => 15,
        "lr" => 14,
        "sp" => 13,
        num => match num.parse::<u8>() {
            Ok(n) if n < 16 => n,
            _ => return json!({"content": [{"type": "text", "text": "Invalid register"}], "isError": true}),
        },
    };

    match send_command(state, DebugCommand::ReadRegister { register }).await {
        Ok(DebugResponse::RegisterValue { register, value }) => {
            json!({"content": [{"type": "text", "text": format!("r{}: {:#010X} ({})", register, value, value)}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute get_cpu_state command
async fn execute_get_cpu_state(state: &McpState) -> Value {
    match send_command(state, DebugCommand::GetCpuState).await {
        Ok(DebugResponse::CpuState { registers, cpsr, pc }) => {
            let mut output = String::from("CPU State:\n");
            output.push_str(&format!("  PC:   {:#010X}\n", pc));
            output.push_str(&format!("  CPSR: {:#010X}\n", cpsr));
            output.push_str("  Registers:\n");
            for (i, val) in registers.iter().enumerate() {
                output.push_str(&format!("    r{:<2}: {:#010X}\n", i, val));
            }
            json!({"content": [{"type": "text", "text": output}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute read_palette command
async fn execute_read_palette(state: &McpState) -> Value {
    match send_command(state, DebugCommand::GetPalette).await {
        Ok(DebugResponse::PaletteData { data }) => {
            let mut output = String::from("Palette RAM (256 colors):\n");
            for (i, color) in data.iter().take(32).enumerate() {
                if i % 8 == 0 {
                    output.push_str(&format!("\n[{:3}]: ", i));
                }
                let r = (color >> 0) & 0x1F;
                let g = (color >> 5) & 0x1F;
                let b = (color >> 10) & 0x1F;
                output.push_str(&format!("RGB({:2},{:2},{:2}) ", r, g, b));
            }
            output.push_str("\n... (showing first 32 colors)");
            json!({"content": [{"type": "text", "text": output}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute get_screenshot command
async fn execute_get_screenshot(state: &McpState) -> Value {
    match send_command(state, DebugCommand::GetScreenshot).await {
        Ok(DebugResponse::Screenshot { width: _, height: _, rgba_data }) => {
            // Encode as base64
            use serde_json::json;
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&rgba_data);

            json!({
                "content": [{
                    "type": "image",
                    "data": base64_data,
                    "mimeType": "image/png"
                }]
            })
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute disassemble command
async fn execute_disassemble(args: &Value, state: &McpState) -> Value {
    let address_str = match args.get("address").and_then(|a| a.as_str()) {
        Some(a) => a,
        None => return json!({"content": [{"type": "text", "text": "Missing address parameter"}], "isError": true}),
    };

    let address = if address_str.starts_with("0x") {
        u32::from_str_radix(&address_str[2..], 16)
    } else {
        u32::from_str_radix(address_str, 16)
    };

    let address = match address {
        Ok(a) => a,
        Err(e) => return json!({"content": [{"type": "text", "text": format!("Invalid address: {}", e)}], "isError": true}),
    };

    let count = args.get("count").and_then(|c| c.as_u64()).unwrap_or(10) as u32;
    let mode = args.get("mode").and_then(|m| m.as_str()).map(|s| s.to_string());

    match send_command(state, DebugCommand::Disassemble { address, count, mode }).await {
        Ok(DebugResponse::Disassembly { instructions }) => {
            let mut output = format!("Disassembly at 0x{:08X}:\n", address);
            for (i, inst) in instructions.iter().enumerate() {
                output.push_str(&format!("0x{:08X}:  {}\n", address + (i as u32 * 4), inst));
            }
            json!({"content": [{"type": "text", "text": output}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute add_breakpoint command
async fn execute_add_breakpoint(args: &Value, state: &McpState) -> Value {
    let address_str = match args.get("address").and_then(|a| a.as_str()) {
        Some(a) => a,
        None => return json!({"content": [{"type": "text", "text": "Missing address parameter"}], "isError": true}),
    };

    let address = if address_str.starts_with("0x") {
        u32::from_str_radix(&address_str[2..], 16)
    } else {
        u32::from_str_radix(address_str, 16)
    };

    let address = match address {
        Ok(a) => a,
        Err(e) => return json!({"content": [{"type": "text", "text": format!("Invalid address: {}", e)}], "isError": true}),
    };

    match send_command(state, DebugCommand::AddBreakpoint { address }).await {
        Ok(DebugResponse::BreakpointAdded { address }) => {
            json!({"content": [{"type": "text", "text": format!("Breakpoint added at 0x{:08X}", address)}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute remove_breakpoint command
async fn execute_remove_breakpoint(args: &Value, state: &McpState) -> Value {
    let address_str = match args.get("address").and_then(|a| a.as_str()) {
        Some(a) => a,
        None => return json!({"content": [{"type": "text", "text": "Missing address parameter"}], "isError": true}),
    };

    let address = if address_str.starts_with("0x") {
        u32::from_str_radix(&address_str[2..], 16)
    } else {
        u32::from_str_radix(address_str, 16)
    };

    let address = match address {
        Ok(a) => a,
        Err(e) => return json!({"content": [{"type": "text", "text": format!("Invalid address: {}", e)}], "isError": true}),
    };

    match send_command(state, DebugCommand::RemoveBreakpoint { address }).await {
        Ok(DebugResponse::BreakpointRemoved { address }) => {
            json!({"content": [{"type": "text", "text": format!("Breakpoint removed at 0x{:08X}", address)}]})
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Execute list_breakpoints command
async fn execute_list_breakpoints(state: &McpState) -> Value {
    match send_command(state, DebugCommand::ListBreakpoints).await {
        Ok(DebugResponse::BreakpointList { breakpoints }) => {
            if breakpoints.is_empty() {
                json!({"content": [{"type": "text", "text": "No breakpoints set"}]})
            } else {
                let mut output = String::from("Active breakpoints:\n");
                for addr in breakpoints {
                    output.push_str(&format!("  0x{:08X}\n", addr));
                }
                json!({"content": [{"type": "text", "text": output}]})
            }
        }
        Ok(DebugResponse::Error { message }) => {
            json!({"content": [{"type": "text", "text": format!("Error: {}", message)}], "isError": true})
        }
        Err(e) => json!({"content": [{"type": "text", "text": format!("Failed: {}", e)}], "isError": true}),
        _ => json!({"content": [{"type": "text", "text": "Unexpected response"}], "isError": true}),
    }
}

// Helper to send a command and await response
async fn send_command(state: &McpState, command: DebugCommand) -> Result<DebugResponse, String> {
    let (response_tx, response_rx) = tokio::sync::oneshot::channel();

    // Clone the sender to avoid holding the lock across await
    {
        let tx = state.command_tx.lock().unwrap();
        tx.send(CommandRequest { command, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;
    }

    response_rx.await.map_err(|e| format!("Failed to receive response: {}", e))
}
