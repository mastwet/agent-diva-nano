use std::collections::HashMap;
use std::path::PathBuf;

/// Re-export MCP server configuration from the core crate.
pub use agent_diva_core::config::MCPServerConfig;

/// Re-export tool assembly types.
pub use crate::tool_assembly::{BuiltInToolsConfig, ShellToolConfig, WebToolConfig};

/// Simplified web search configuration.
#[derive(Debug, Clone)]
pub struct WebSearchConfig {
    /// Search provider name (e.g. "bocha").
    pub provider: String,
    /// API key for the search provider.
    pub api_key: Option<String>,
    /// Maximum number of results to return.
    pub max_results: u32,
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            provider: "bocha".to_string(),
            api_key: None,
            max_results: 5,
        }
    }
}

/// Soul (identity) configuration.
#[derive(Debug, Clone)]
pub struct SoulConfig {
    /// Whether soul context injection is enabled.
    pub enabled: bool,
    /// Maximum characters for soul context.
    pub max_chars: usize,
    /// Bootstrap soul only once.
    pub bootstrap_once: bool,
    /// Notify on soul changes.
    pub notify_on_change: bool,
    /// Window in seconds for frequent-change detection.
    pub frequent_change_window_secs: u64,
    /// Threshold for frequent-change hints.
    pub frequent_change_threshold: usize,
    /// Show boundary confirmation hint.
    pub boundary_confirmation_hint: bool,
}

impl Default for SoulConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_chars: 4000,
            bootstrap_once: true,
            notify_on_change: true,
            frequent_change_window_secs: 300,
            frequent_change_threshold: 3,
            boundary_confirmation_hint: true,
        }
    }
}

/// Minimal configuration to create an agent.
///
/// # Example
/// ```rust,no_run
/// use agent_diva_nano::NanoConfig;
/// use std::path::PathBuf;
///
/// let config = NanoConfig {
///     model: "deepseek-chat"to_string(),
///     api_key: std::env::var("API_KEY").unwrap(),
///     ..Default::default()
/// };
/// ```
#[derive(Debug, Clone)]
pub struct NanoConfig {
    /// LLM model identifier, e.g. "deepseek-chat", "gpt-4o", "openrouter/anthropic/claude-sonnet-4".
    pub model: String,
    /// API key for the provider.
    pub api_key: String,
    /// Custom API base URL (optional, for private deployments or OpenRouter).
    pub api_base: Option<String>,
    /// Workspace directory — root for skills, memory, and SOUL.md.
    pub workspace: PathBuf,
    /// Maximum tool-call iterations per turn (default: 20).
    pub max_iterations: usize,
    /// Shell command execution timeout in seconds (default: 60).
    pub exec_timeout: u64,
    /// Restrict file-system tools to the workspace directory (default: true).
    pub restrict_to_workspace: bool,
    /// Web search configuration (optional).
    pub web_search: Option<WebSearchConfig>,
    /// MCP server configurations.
    pub mcp_servers: HashMap<String, MCPServerConfig>,
    /// Soul / identity context configuration.
    pub soul: SoulConfig,
    /// Built-in tools enable/disable configuration.
    /// If not set, defaults to BuiltInToolsConfig::default().
    pub builtin_tools: Option<BuiltInToolsConfig>,
}

impl Default for NanoConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            api_key: String::new(),
            api_base: None,
            workspace: PathBuf::from("."),
            max_iterations: 20,
            exec_timeout: 60,
            restrict_to_workspace: true,
            web_search: None,
            mcp_servers: HashMap::new(),
            soul: SoulConfig::default(),
            builtin_tools: None, // Uses BuiltInToolsConfig::default() when not set
        }
    }
}

impl NanoConfig {
    /// Build from environment variables:
    /// - `NANO_MODEL`
    /// - `NANO_API_KEY`
    /// - `NANO_API_BASE` (optional)
    pub fn from_env() -> Result<Self, String> {
        let model = std::env::var("NANO_MODEL")
            .map_err(|_| "NANO_MODEL environment variable not set")?;
        let api_key = std::env::var("NANO_API_KEY")
            .map_err(|_| "NANO_API_KEY environment variable not set")?;
        let api_base = std::env::var("NANO_API_BASE").ok().filter(|s| !s.is_empty());

        Ok(Self {
            model,
            api_key,
            api_base,
            ..Default::default()
        })
    }
}
