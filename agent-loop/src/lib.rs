pub mod error;
pub mod prompt;
pub mod openai;
pub mod tools;
pub mod types;

// Allows the `#[toolset]` macro's generated `::kiruklaw_agent_loop::…`
// paths to resolve within this crate itself (e.g. in tests).
extern crate self as kiruklaw_agent_loop;

pub use kiruklaw_macros::{self as macros, tool, toolset};

pub use error::Error as AgentLoopError;
pub use types::*;
pub use prompt::{AgentLoop, ContextProvider, prompt};
