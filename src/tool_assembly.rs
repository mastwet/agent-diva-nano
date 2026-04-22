//! Flexible tool assembly for agent-diva-nano.
//!
//! This module provides a builder-pattern interface for configuring
//! which tools are available to an agent, allowing fine-grained control
//! over tool registration and custom tool injection.

use agent_diva_tools::{Tool, ToolRegistry};
use agent_diva_core::security::{SecurityConfig, SecurityPolicy};
use agent_diva_core::cron::CronService;
use agent_diva_files::FileManager;
use std::sync::Arc;
use std::path::PathBuf;

/// Configuration for enabling/disabling built-in tool categories.
#[derive(Debug, Clone)]
pub struct BuiltInToolsConfig {
    /// Enable file system tools (read, write, edit, list_dir).
    pub filesystem: bool,
    /// Enable shell execution tool.
    pub shell: bool,
    /// Enable web search and fetch tools.
    pub web: bool,
    /// Enable spawn tool for subagent creation.
    pub spawn: bool,
    /// Enable cron/scheduling tool.
    pub cron: bool,
    /// Enable MCP tools discovered from configured servers.
    pub mcp: bool,
    /// Enable attachment reading tool.
    pub attachment: bool,
}

impl Default for BuiltInToolsConfig {
    fn default() -> Self {
        Self {
            filesystem: true,
            shell: true,
            web: true,
            spawn: true,
            cron: false, // Requires explicit CronService setup
            mcp: true,
            attachment: true,
        }
    }
}

impl BuiltInToolsConfig {
    /// Create a minimal config with only essential tools.
    pub fn minimal() -> Self {
        Self {
            filesystem: true,
            shell: false,
            web: false,
            spawn: false,
            cron: false,
            mcp: false,
            attachment: false,
        }
    }

    /// Create a config with no built-in tools (only custom tools).
    pub fn none() -> Self {
        Self {
            filesystem: false,
            shell: false,
            web: false,
            spawn: false,
            cron: false,
            mcp: false,
            attachment: false,
        }
    }

    /// Create a config with all built-in tools enabled.
    pub fn all() -> Self {
        Self {
            filesystem: true,
            shell: true,
            web: true,
            spawn: true,
            cron: true,
            mcp: true,
            attachment: true,
        }
    }
}

/// Shell tool configuration.
#[derive(Debug, Clone)]
pub struct ShellToolConfig {
    /// Execution timeout in seconds.
    pub timeout_secs: u64,
    /// Working directory for shell commands.
    pub working_dir: Option<PathBuf>,
    /// Restrict commands to workspace directory.
    pub restrict_to_workspace: bool,
}

impl Default for ShellToolConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 60,
            working_dir: None,
            restrict_to_workspace: true,
        }
    }
}

/// Web tool configuration.
#[derive(Debug, Clone)]
pub struct WebToolConfig {
    /// Enable web search.
    pub search_enabled: bool,
    /// Enable web fetch.
    pub fetch_enabled: bool,
    /// Search provider name.
    pub search_provider: String,
    /// Search API key.
    pub search_api_key: Option<String>,
    /// Max search results.
    pub max_results: u32,
}

impl Default for WebToolConfig {
    fn default() -> Self {
        Self {
            search_enabled: true,
            fetch_enabled: true,
            search_provider: "bocha".to_string(),
            search_api_key: None,
            max_results: 5,
        }
    }
}

/// Builder for assembling a ToolRegistry with fine-grained control.
pub struct ToolAssembly {
    workspace: PathBuf,
    builtin_config: BuiltInToolsConfig,
    shell_config: ShellToolConfig,
    web_config: WebToolConfig,
    security_config: SecurityConfig,
    custom_tools: Vec<Arc<dyn Tool>>,
    mcp_servers: std::collections::HashMap<String, agent_diva_core::config::MCPServerConfig>,
    cron_service: Option<Arc<CronService>>,
    subagent_spawner: Option<Arc<dyn SubagentSpawner>>,
    file_manager: Option<Arc<FileManager>>,
}

/// Trait for custom subagent spawning logic.
/// Allows users to provide their own subagent implementation.
#[async_trait::async_trait]
pub trait SubagentSpawner: Send + Sync {
    /// Spawn a subagent task.
    async fn spawn(
        &self,
        task: String,
        label: Option<String>,
        channel: String,
        chat_id: String,
    ) -> Result<String, agent_diva_tools::ToolError>;
}

impl ToolAssembly {
    /// Create a new ToolAssembly builder for the given workspace.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            builtin_config: BuiltInToolsConfig::default(),
            shell_config: ShellToolConfig::default(),
            web_config: WebToolConfig::default(),
            security_config: SecurityConfig::default(),
            custom_tools: Vec::new(),
            mcp_servers: std::collections::HashMap::new(),
            cron_service: None,
            subagent_spawner: None,
            file_manager: None,
        }
    }

    /// Provide a file manager for attachment handling.
    pub fn with_file_manager(mut self, manager: Arc<FileManager>) -> Self {
        self.file_manager = Some(manager);
        self
    }

    /// Configure which built-in tools are enabled.
    pub fn builtin(mut self, config: BuiltInToolsConfig) -> Self {
        self.builtin_config = config;
        self
    }

    /// Enable or disable filesystem tools.
    pub fn filesystem(mut self, enabled: bool) -> Self {
        self.builtin_config.filesystem = enabled;
        self
    }

    /// Enable or disable shell tool.
    pub fn shell(mut self, enabled: bool) -> Self {
        self.builtin_config.shell = enabled;
        self
    }

    /// Configure shell tool settings.
    pub fn shell_config(mut self, config: ShellToolConfig) -> Self {
        self.shell_config = config;
        self.builtin_config.shell = true;
        self
    }

    /// Enable or disable web tools.
    pub fn web(mut self, enabled: bool) -> Self {
        self.builtin_config.web = enabled;
        self
    }

    /// Configure web tool settings.
    pub fn web_config(mut self, config: WebToolConfig) -> Self {
        self.web_config = config;
        self.builtin_config.web = true;
        self
    }

    /// Enable or disable spawn tool.
    pub fn spawn(mut self, enabled: bool) -> Self {
        self.builtin_config.spawn = enabled;
        self
    }

    /// Provide a custom subagent spawner implementation.
    pub fn with_subagent_spawner(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.subagent_spawner = Some(spawner);
        self.builtin_config.spawn = true;
        self
    }

    /// Enable or disable cron tool.
    pub fn cron(mut self, enabled: bool) -> Self {
        self.builtin_config.cron = enabled;
        self
    }

    /// Provide a cron service for scheduling.
    pub fn with_cron_service(mut self, service: Arc<CronService>) -> Self {
        self.cron_service = Some(service);
        self.builtin_config.cron = true;
        self
    }

    /// Enable or disable MCP tools.
    pub fn mcp(mut self, enabled: bool) -> Self {
        self.builtin_config.mcp = enabled;
        self
    }

    /// Add MCP server configuration.
    pub fn add_mcp_server(mut self, name: String, config: agent_diva_core::config::MCPServerConfig) -> Self {
        self.mcp_servers.insert(name, config);
        self.builtin_config.mcp = true;
        self
    }

    /// Set MCP servers configuration.
    pub fn mcp_servers(mut self, servers: std::collections::HashMap<String, agent_diva_core::config::MCPServerConfig>) -> Self {
        self.mcp_servers = servers;
        self
    }

    /// Enable or disable attachment tool.
    pub fn attachment(mut self, enabled: bool) -> Self {
        self.builtin_config.attachment = enabled;
        self
    }

    /// Configure security settings.
    pub fn security(mut self, config: SecurityConfig) -> Self {
        self.security_config = config;
        self
    }

    /// Restrict file operations to workspace only.
    pub fn restrict_to_workspace(mut self, restrict: bool) -> Self {
        self.security_config.workspace_only = restrict;
        self.shell_config.restrict_to_workspace = restrict;
        self
    }

    /// Add a custom tool.
    pub fn with_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.custom_tools.push(tool);
        self
    }

    /// Add multiple custom tools.
    pub fn with_tools(mut self, tools: Vec<Arc<dyn Tool>>) -> Self {
        self.custom_tools.extend(tools);
        self
    }

    /// Build the ToolRegistry with all configured tools.
    pub fn build(self) -> ToolRegistry {
        let mut registry = ToolRegistry::new();

        // Register filesystem tools
        if self.builtin_config.filesystem {
            let security = Arc::new(SecurityPolicy::with_config(
                self.workspace.clone(),
                self.security_config.clone(),
            ));
            registry.register(Arc::new(agent_diva_tools::ReadFileTool::new(security.clone())));
            registry.register(Arc::new(agent_diva_tools::WriteFileTool::new(security.clone())));
            registry.register(Arc::new(agent_diva_tools::EditFileTool::new(security.clone())));
            registry.register(Arc::new(agent_diva_tools::ListDirTool::new(security)));
        }

        // Register shell tool
        if self.builtin_config.shell {
            registry.register(Arc::new(agent_diva_tools::ExecTool::with_config(
                self.shell_config.timeout_secs,
                self.shell_config.working_dir.clone().or(Some(self.workspace.clone())),
                self.shell_config.restrict_to_workspace,
            )));
        }

        // Register web tools
        if self.builtin_config.web {
            if self.web_config.search_enabled {
                // WebSearchTool only takes api_key
                registry.register(Arc::new(agent_diva_tools::WebSearchTool::new(
                    self.web_config.search_api_key.clone(),
                )));
            }
            if self.web_config.fetch_enabled {
                registry.register(Arc::new(agent_diva_tools::WebFetchTool::new()));
            }
        }

        // Register spawn tool
        if self.builtin_config.spawn {
            if let Some(spawner) = self.subagent_spawner {
                registry.register(Arc::new(agent_diva_tools::SpawnTool::new(
                    move |task, label, channel, chat_id| {
                        let spawner = spawner.clone();
                        async move {
                            spawner.spawn(task, label, channel, chat_id).await
                        }
                    },
                )));
            }
            // Note: If no custom spawner provided, this will be handled by AgentLoop integration
        }

        // Register cron tool
        if self.builtin_config.cron {
            if let Some(cron_service) = self.cron_service {
                registry.register(Arc::new(agent_diva_tools::CronTool::new(cron_service)));
            }
        }

        // Register MCP tools
        if self.builtin_config.mcp && !self.mcp_servers.is_empty() {
            for mcp_tool in agent_diva_tools::load_mcp_tools_sync(&self.mcp_servers) {
                registry.register(mcp_tool);
            }
        }

        // Register attachment tool
        if self.builtin_config.attachment {
            // Attachment tool requires FileManager - skip if not provided
            // FileManager creation is async, so we can't create it here
            if let Some(fm) = self.file_manager.clone() {
                registry.register(Arc::new(agent_diva_tools::ReadAttachmentTool::new(fm)));
            }
        }

        // Register custom tools
        for tool in self.custom_tools {
            registry.register(tool);
        }

        registry
    }

    /// Build and return both the registry and configuration for AgentLoop integration.
    pub fn build_for_agent_loop(self) -> (ToolRegistry, AgentLoopToolConfig) {
        let builtin_config = self.builtin_config.clone();
        let shell_config = self.shell_config.clone();
        let web_config = self.web_config.clone();
        let security_config = self.security_config.clone();
        let mcp_servers = self.mcp_servers.clone();
        let cron_service = self.cron_service.clone();
        let subagent_spawner = self.subagent_spawner.clone();
        
        let registry = self.build();
        let loop_config = AgentLoopToolConfig {
            builtin_config,
            shell_config,
            web_config,
            security_config,
            mcp_servers,
            cron_service,
            subagent_spawner,
        };
        (registry, loop_config)
    }
}

/// Configuration needed for AgentLoop integration when using ToolAssembly.
#[derive(Clone)]
pub struct AgentLoopToolConfig {
    pub builtin_config: BuiltInToolsConfig,
    pub shell_config: ShellToolConfig,
    pub web_config: WebToolConfig,
    pub security_config: SecurityConfig,
    pub mcp_servers: std::collections::HashMap<String, agent_diva_core::config::MCPServerConfig>,
    pub cron_service: Option<Arc<CronService>>,
    pub subagent_spawner: Option<Arc<dyn SubagentSpawner>>,
}

impl Default for AgentLoopToolConfig {
    fn default() -> Self {
        Self {
            builtin_config: BuiltInToolsConfig::default(),
            shell_config: ShellToolConfig::default(),
            web_config: WebToolConfig::default(),
            security_config: SecurityConfig::default(),
            mcp_servers: std::collections::HashMap::new(),
            cron_service: None,
            subagent_spawner: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_assembly_minimal() {
        let registry = ToolAssembly::new(PathBuf::from("/tmp/test"))
            .builtin(BuiltInToolsConfig::minimal())
            .build();
        
        // Minimal config has filesystem enabled
        assert!(registry.has("read_file"));
        assert!(registry.has("write_file"));
        assert!(!registry.has("exec"));
        assert!(!registry.has("web_search"));
    }

    #[test]
    fn test_tool_assembly_none() {
        let registry = ToolAssembly::new(PathBuf::from("/tmp/test"))
            .builtin(BuiltInToolsConfig::none())
            .build();
        
        assert!(registry.is_empty());
    }

    #[test]
    fn test_tool_assembly_default() {
        let registry = ToolAssembly::new(PathBuf::from("/tmp/test"))
            .build();
        
        // Default has most tools enabled (except cron which requires service)
        assert!(registry.has("read_file"));
        assert!(registry.has("exec"));
        assert!(registry.has("web_search"));
        assert!(registry.has("spawn"));
        assert!(!registry.has("cron")); // cron requires explicit service
    }

    #[test]
    fn test_tool_assembly_custom_config() {
        let registry = ToolAssembly::new(PathBuf::from("/tmp/test"))
            .filesystem(true)
            .shell(false)
            .web(true)
            .spawn(false)
            .build();
        
        assert!(registry.has("read_file"));
        assert!(!registry.has("exec"));
        assert!(registry.has("web_search"));
        assert!(!registry.has("spawn"));
    }
}