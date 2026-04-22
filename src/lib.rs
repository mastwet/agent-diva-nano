//! agent-diva-nano — minimal "create an agent" library.
//!
//! Add to your project:
//! ```toml
//! [dependencies]
//! agent-diva-nano = "0.3"
//! tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
//! ```
//!
//! # Quick start
//!
//! ```rust,no_run
//! use agent_diva_nano::{chat, NanoConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = NanoConfig {
//!     model: "deepseek-chat"to_string(),
//!     api_key: std::env::var("API_KEY")?,
//!     ..Default::default()
//! };
//!
//! let reply = chat("What is Rust ownership?", &config).await?;
//! println!("{}", reply);
//! # Ok(())
//! # }
//! ```
//!
//! # Stateful agent
//!
//! For multi-turn conversations, use [`Agent`] directly:
//!
//! ```rust,no_run
//! use agent_diva_nano::{Agent, NanoConfig};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = NanoConfig {
//!     model: "deepseek-chat"to_string(),
//!     api_key: std::env::var("API_KEY")?,
//!     ..Default::default()
//! };
//!
//! let mut agent = Agent::new(config).build().await?;
//! agent.start().await?;
//!
//! let r1 = agent.send("Hello").await?;
//! let r2 = agent.send("Tell me more").await?;
//!
//! agent.stop().await;
//! # Ok(())
//! # }
//! ```
//!
//! # Flexible tool assembly
//!
//! Use [`ToolAssembly`] for fine-grained control over which tools are available:
//!
//! ```rust,no_run
//! use agent_diva_nano::{AgentBuilder, ToolAssembly, BuiltInToolsConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create agent with minimal tools (only filesystem)
//! let assembly = ToolAssembly::new(std::path::PathBuf::from("./workspace"))
//!     .builtin(BuiltInToolsConfig::minimal())
//!     .build();
//!
//! // Or create agent with custom tools only
//! let assembly = ToolAssembly::new(std::path::PathBuf::from("./workspace"))
//!     .builtin(BuiltInToolsConfig::none())
//!     .with_tool(my_custom_tool);
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod agent;
pub mod chat;
pub mod error;
pub mod tool_assembly;
pub mod nano_loop;

mod internal;

pub use config::{NanoConfig, MCPServerConfig, SoulConfig, WebSearchConfig, BuiltInToolsConfig, ShellToolConfig, WebToolConfig};
pub use agent::{Agent, AgentBuilder, AgentLoopMode};
pub use chat::{chat, chat_stream};
pub use error::NanoError;
pub use tool_assembly::{ToolAssembly, SubagentSpawner};
pub use nano_loop::{NanoAgentLoop, NanoLoopConfig, NanoRuntimeControlCommand};

/// Re-export tool types for custom tool creation.
pub use agent_diva_tools::{Tool, ToolError, ToolRegistry};

/// Re-export core event types so consumers don't need to depend on `agent-diva-core`.
pub use agent_diva_core::bus::AgentEvent;

/// Re-export provider registry so consumers can resolve provider names from model identifiers.
pub use agent_diva_providers::ProviderRegistry;

/// Re-export FileManager when files feature is enabled.
#[cfg(feature = "files")]
pub use agent_diva_files::FileManager;
