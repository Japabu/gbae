use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

/// Commands that can be sent to the emulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugCommand {
    Step { count: u32 },
    Continue,
    ReadMemory { address: u32, length: u32 },
    ReadRegister { register: u8 },
    GetCpuState,
    GetPalette,
}

/// Responses from the emulator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebugResponse {
    StepComplete { instructions: u32 },
    ContinueStarted,
    MemoryData { address: u32, data: Vec<u8> },
    RegisterValue { register: u8, value: u32 },
    CpuState {
        registers: [u32; 16],
        cpsr: u32,
        pc: u32,
    },
    PaletteData { data: Vec<u16> },
    Error { message: String },
}

/// A command with a response channel
pub struct CommandRequest {
    pub command: DebugCommand,
    pub response_tx: oneshot::Sender<DebugResponse>,
}

/// Channel pair for IPC
pub type CommandSender = mpsc::UnboundedSender<CommandRequest>;
pub type CommandReceiver = mpsc::UnboundedReceiver<CommandRequest>;

/// Create a new command channel pair
pub fn create_channel() -> (CommandSender, CommandReceiver) {
    mpsc::unbounded_channel()
}
