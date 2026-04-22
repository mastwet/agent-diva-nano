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
//!     model: "deepseek-chat".to_string(),
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
//!     model: "deepseek-chat".to_string(),
//!     api_key: std::env::var("API_KEY")?,
//!     ..Default::default()
//! };
//!
//! let mut agent = Agent::new(config).build()?;
//! agent.start().await?;
//!
//! let r1 = agent.send("Hello").await?;
//! let r2 = agent.send("Tell me more").await?;
//!
//! agent.stop().await;
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod agent;
pub mod chat;
pub mod error;

mod internal;

pub use config::{NanoConfig, MCPServerConfig, SoulConfig, WebSearchConfig};
pub use agent::{Agent, AgentBuilder};
pub use chat::{chat, chat_stream};
pub use error::NanoError;

/// Re-export core event types so consumers don't need to depend on `agent-diva-core`.
pub use agent_diva_core::bus::AgentEvent;

/// Re-export provider registry so consumers can resolve provider names from model identifiers.
pub use agent_diva_providers::ProviderRegistry;
