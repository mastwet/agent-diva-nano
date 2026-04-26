//! Thin wrapper around the shared agent-diva tool assembly.

use agent_diva_agent::tool_config::network::{
    NetworkToolConfig, WebFetchRuntimeConfig, WebRuntimeConfig, WebSearchRuntimeConfig,
};
pub use agent_diva_agent::{BuiltInToolsConfig, SubagentSpawner};
use agent_diva_core::config::MCPServerConfig;
#[cfg(feature = "files")]
use agent_diva_files::FileManager;
use agent_diva_tooling::{Tool, ToolRegistry};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Shell tool configuration.
#[derive(Debug, Clone)]
pub struct ShellToolConfig {
    pub timeout_secs: u64,
    pub working_dir: Option<PathBuf>,
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
    pub search_enabled: bool,
    pub fetch_enabled: bool,
    pub search_provider: String,
    pub search_api_key: Option<String>,
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
    custom_tools: Vec<Arc<dyn Tool>>,
    mcp_servers: HashMap<String, MCPServerConfig>,
    subagent_spawner: Option<Arc<dyn SubagentSpawner>>,
    #[cfg(feature = "files")]
    file_manager: Option<Arc<FileManager>>,
}

impl ToolAssembly {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            builtin_config: BuiltInToolsConfig::default(),
            shell_config: ShellToolConfig::default(),
            web_config: WebToolConfig::default(),
            custom_tools: Vec::new(),
            mcp_servers: HashMap::new(),
            subagent_spawner: None,
            #[cfg(feature = "files")]
            file_manager: None,
        }
    }

    #[cfg(feature = "files")]
    pub fn with_file_manager(mut self, manager: Arc<FileManager>) -> Self {
        self.file_manager = Some(manager);
        self
    }

    pub fn builtin(mut self, config: BuiltInToolsConfig) -> Self {
        self.builtin_config = config;
        self
    }

    pub fn filesystem(mut self, enabled: bool) -> Self {
        self.builtin_config.filesystem = enabled;
        self
    }

    pub fn shell(mut self, enabled: bool) -> Self {
        self.builtin_config.shell = enabled;
        self
    }

    pub fn shell_config(mut self, config: ShellToolConfig) -> Self {
        self.shell_config = config;
        self.builtin_config.shell = true;
        self
    }

    pub fn web(mut self, enabled: bool) -> Self {
        self.builtin_config.web_search = enabled;
        self.builtin_config.web_fetch = enabled;
        self
    }

    pub fn web_config(mut self, config: WebToolConfig) -> Self {
        self.web_config = config;
        self.builtin_config.web_search = true;
        self.builtin_config.web_fetch = true;
        self
    }

    pub fn spawn(mut self, enabled: bool) -> Self {
        self.builtin_config.spawn = enabled;
        self
    }

    pub fn with_subagent_spawner(mut self, spawner: Arc<dyn SubagentSpawner>) -> Self {
        self.subagent_spawner = Some(spawner);
        self.builtin_config.spawn = true;
        self
    }

    pub fn cron(mut self, enabled: bool) -> Self {
        self.builtin_config.cron = enabled;
        self
    }

    pub fn mcp(mut self, enabled: bool) -> Self {
        self.builtin_config.mcp = enabled;
        self
    }

    pub fn add_mcp_server(mut self, name: String, config: MCPServerConfig) -> Self {
        self.mcp_servers.insert(name, config);
        self.builtin_config.mcp = true;
        self
    }

    pub fn mcp_servers(mut self, servers: HashMap<String, MCPServerConfig>) -> Self {
        self.mcp_servers = servers;
        self
    }

    pub fn attachment(mut self, enabled: bool) -> Self {
        self.builtin_config.attachment = enabled;
        self
    }

    pub fn restrict_to_workspace(mut self, restrict: bool) -> Self {
        self.shell_config.restrict_to_workspace = restrict;
        self
    }

    pub fn with_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.custom_tools.push(tool);
        self
    }

    pub fn with_tools(mut self, tools: Vec<Arc<dyn Tool>>) -> Self {
        self.custom_tools.extend(tools);
        self
    }

    pub fn build(self) -> ToolRegistry {
        self.into_shared().build()
    }

    fn into_shared(self) -> agent_diva_agent::ToolAssembly {
        let network = NetworkToolConfig {
            web: WebRuntimeConfig {
                search: WebSearchRuntimeConfig {
                    provider: self.web_config.search_provider,
                    enabled: self.web_config.search_enabled,
                    api_key: self.web_config.search_api_key,
                    max_results: self.web_config.max_results,
                },
                fetch: WebFetchRuntimeConfig {
                    enabled: self.web_config.fetch_enabled,
                },
            },
        };

        let mut assembly = agent_diva_agent::ToolAssembly::new(self.workspace)
            .builtin(self.builtin_config)
            .with_network_config(network)
            .with_exec_timeout(self.shell_config.timeout_secs)
            .restrict_to_workspace(self.shell_config.restrict_to_workspace)
            .mcp_servers(self.mcp_servers)
            .with_tools(self.custom_tools);
        if let Some(spawner) = self.subagent_spawner {
            assembly = assembly.with_subagent_spawner(spawner);
        }
        #[cfg(feature = "files")]
        if let Some(file_manager) = self.file_manager {
            assembly = assembly.with_file_manager(file_manager);
        }
        assembly
    }
}
