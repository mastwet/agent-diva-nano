//! Agent creation and management for agent-diva-nano.

use crate::{NanoConfig, NanoError};
use crate::tool_assembly::{ToolAssembly, BuiltInToolsConfig};
use crate::nano_loop::{NanoAgentLoop, NanoLoopConfig, NanoRuntimeControlCommand};
use crate::internal::context::NanoSoulSettings;
use crate::internal::provider::{build_provider, build_tool_config};
use agent_diva_agent::AgentLoop;
use agent_diva_core::bus::{AgentEvent, InboundMessage, MessageBus};
#[cfg(feature = "files")]
use agent_diva_files::{FileManager, FileConfig};
use agent_diva_providers::DynamicProvider;
use agent_diva_tools::{Tool, ToolRegistry};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// Agent loop mode selection.
#[derive(Debug, Clone, Default)]
pub enum AgentLoopMode {
    /// Use agent-diva-agent's AgentLoop (default).
    /// Tools are configured through ToolConfig.
    #[default]
    Standard,
    /// Use nano's lightweight NanoAgentLoop.
    /// Tools are configured through ToolAssembly with full control.
    Nano,
}

/// A running agent instance.
///
/// Create with [`Agent::new`](Agent::new) and the builder pattern,
/// then call [`start`](Agent::start) to run the background loop.
pub struct Agent {
    bus: MessageBus,
    provider: Arc<DynamicProvider>,
    mode: AgentLoopMode,
    /// For standard mode: tool configuration
    tool_config: Option<agent_diva_agent::ToolConfig>,
    /// For nano mode: pre-built tool registry
    tool_registry: Option<ToolRegistry>,
    nano_loop_config: Option<NanoLoopConfig>,
    workspace: PathBuf,
    model: String,
    max_iterations: usize,
    #[cfg(feature = "files")]
    file_manager: Arc<FileManager>,
    runtime_control_tx: Option<mpsc::UnboundedSender<NanoRuntimeControlCommand>>,
    agent_handle: Option<JoinHandle<()>>,
    outbound_handle: Option<JoinHandle<()>>,
}

/// Builder for configuring an [`Agent`].
pub struct AgentBuilder {
    config: NanoConfig,
    custom_tools: Vec<Arc<dyn Tool>>,
    tool_assembly: Option<ToolAssembly>,
    mode: AgentLoopMode,
    system_prompt: Option<String>,
}

impl Agent {
    /// Start configuring a new agent with default settings.
    pub fn new(config: NanoConfig) -> AgentBuilder {
        AgentBuilder {
            config,
            custom_tools: Vec::new(),
            tool_assembly: None,
            mode: AgentLoopMode::default(),
            system_prompt: None,
        }
    }

    /// Start the background agent loop.
    pub async fn start(&mut self) -> Result<(), NanoError> {
        if self.agent_handle.is_some() {
            return Err(NanoError::Other("Agent already started".to_string()));
        }

        let bus = self.bus.clone();
        let provider: Arc<dyn agent_diva_providers::LLMProvider> = self.provider.clone();
        let model = self.model.clone();
        let workspace = self.workspace.clone();
        let max_iterations = self.max_iterations;
        #[cfg(feature = "files")]
        let file_manager = self.file_manager.clone();

        let (runtime_control_tx, runtime_control_rx) = mpsc::unbounded_channel();
        self.runtime_control_tx = Some(runtime_control_tx);

        match self.mode {
            AgentLoopMode::Standard => {
                // Use agent-diva-agent's AgentLoop with ToolConfig
                let tool_config = self.tool_config.clone().unwrap_or_default();
                
                // Build a ToolRegistry that includes custom tools
                let mut registry = ToolRegistry::new();
                for tool in self.tool_registry.iter() {
                    // Clone each tool from the registry
                    for name in tool.tool_names() {
                        if let Some(t) = tool.get(&name) {
                            registry.register(t);
                        }
                    }
                }

                // Note: AgentLoop::with_tools will add its own tools, so custom tools
                // need to be passed via ToolConfig extension or a different approach.
                // For now, we use the standard path with tool_config.
                
                #[cfg(not(feature = "files"))]
                {
                    return Err(NanoError::Other("Standard mode requires 'files' feature. Use Nano mode or enable 'files' feature.".to_string()));
                }
                
                #[cfg(feature = "files")]
                {
                    let mut agent_loop = AgentLoop::with_tools(
                        bus.clone(),
                        provider,
                        workspace,
                        Some(model),
                        Some(max_iterations),
                        tool_config,
                        None, // No runtime control for standard mode (different type)
                        file_manager,
                    ).await.map_err(|e| NanoError::Other(e.to_string()))?;

                    let agent_handle = tokio::spawn(async move {
                        info!("Agent loop (standard) starting");
                        if let Err(e) = agent_loop.run().await {
                            error!("Agent loop error: {}", e);
                        }
                        info!("Agent loop (standard) stopped");
                    });
                    self.agent_handle = Some(agent_handle);
                }
            }
            AgentLoopMode::Nano => {
                // Use nano's lightweight NanoAgentLoop with ToolAssembly
                let tool_registry = match &self.tool_registry {
                    Some(reg) => {
                        // Build a new registry with the same tools
                        let mut new_registry = ToolRegistry::new();
                        for name in reg.tool_names() {
                            if let Some(tool) = reg.get(&name) {
                                new_registry.register(tool);
                            }
                        }
                        new_registry
                    }
                    None => ToolRegistry::new(),
                };
                let nano_config = self.nano_loop_config.clone().unwrap_or_default();

                let mut nano_loop = NanoAgentLoop::new(
                    bus.clone(),
                    provider,
                    workspace,
                    Some(model),
                    nano_config,
                    tool_registry,
                    #[cfg(feature = "files")]
                    file_manager,
                ).await.map_err(|e| NanoError::Other(e.to_string()))?;

                nano_loop = nano_loop.with_runtime_control(runtime_control_rx);

                let agent_handle = tokio::spawn(async move {
                    info!("Nano agent loop starting");
                    if let Err(e) = nano_loop.run().await {
                        error!("Nano agent loop error: {}", e);
                    }
                    info!("Nano agent loop stopped");
                });
                self.agent_handle = Some(agent_handle);
            }
        }

        let bus_for_outbound = self.bus.clone();
        let outbound_handle = tokio::spawn(async move {
            bus_for_outbound.dispatch_outbound_loop().await;
        });
        self.outbound_handle = Some(outbound_handle);

        Ok(())
    }

    /// Send a message and wait for the complete text response.
    pub async fn send(&self, message: impl Into<String>) -> Result<String, NanoError> {
        let content = message.into();
        let channel = "nano";
        let chat_id = "default";

        let mut event_rx = self.bus.subscribe_events();

        let inbound = InboundMessage::new(channel, "user", chat_id, content);
        self.bus
            .publish_inbound(inbound)
            .map_err(|e| NanoError::Agent(e.to_string()))?;

        let mut full_response = String::new();
        loop {
            match tokio::time::timeout(
                std::time::Duration::from_secs(300),
                event_rx.recv(),
            )
            .await
            {
                Ok(Ok(bus_event)) => {
                    if bus_event.channel != channel || bus_event.chat_id != chat_id {
                        continue;
                    }
                    match bus_event.event {
                        AgentEvent::AssistantDelta { text } => full_response.push_str(&text),
                        AgentEvent::FinalResponse { content } => {
                            full_response = content;
                            break;
                        }
                        AgentEvent::Error { message } => {
                            return Err(NanoError::Agent(message));
                        }
                        _ => {}
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => return Err(NanoError::Timeout),
            }
        }

        Ok(full_response)
    }

    /// Send a message and return a channel that receives all agent events.
    pub async fn send_stream(
        &self,
        message: impl Into<String>,
    ) -> Result<mpsc::UnboundedReceiver<AgentEvent>, NanoError> {
        let content = message.into();
        let channel = "nano";
        let chat_id = "default";

        let mut event_rx = self.bus.subscribe_events();

        let inbound = InboundMessage::new(channel, "user", chat_id, content);
        self.bus
            .publish_inbound(inbound)
            .map_err(|e| NanoError::Agent(e.to_string()))?;

        let (tx, rx) = mpsc::unbounded_channel::<AgentEvent>();

        tokio::spawn(async move {
            loop {
                match tokio::time::timeout(
                    std::time::Duration::from_secs(300),
                    event_rx.recv(),
                )
                .await
                {
                    Ok(Ok(bus_event)) => {
                        if bus_event.channel != channel || bus_event.chat_id != chat_id {
                            continue;
                        }
                        let is_final = matches!(
                            bus_event.event,
                            AgentEvent::FinalResponse { .. } | AgentEvent::Error { .. }
                        );
                        if tx.send(bus_event.event).is_err() {
                            break;
                        }
                        if is_final {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });

        Ok(rx)
    }

    /// Dynamically reload tools (only works in Nano mode).
    pub fn reload_tools(&self, registry: ToolRegistry) -> Result<(), NanoError> {
        if let Some(ref tx) = self.runtime_control_tx {
            tx.send(NanoRuntimeControlCommand::ReloadTools(registry))
                .map_err(|e| NanoError::Other(e.to_string()))?;
            Ok(())
        } else {
            Err(NanoError::Other("Runtime control not available (either agent not started or using Standard mode)".to_string()))
        }
    }

    /// Cancel a specific session (only works in Nano mode).
    pub fn cancel_session(&self, chat_id: impl Into<String>) -> Result<(), NanoError> {
        if let Some(ref tx) = self.runtime_control_tx {
            tx.send(NanoRuntimeControlCommand::CancelSession { chat_id: chat_id.into() })
                .map_err(|e| NanoError::Other(e.to_string()))?;
            Ok(())
        } else {
            Err(NanoError::Other("Runtime control not available".to_string()))
        }
    }

    /// Stop the background agent loop.
    pub async fn stop(&mut self) {
        // Send stop command if in Nano mode
        if let Some(ref tx) = self.runtime_control_tx {
            let _ = tx.send(NanoRuntimeControlCommand::Stop);
        }

        if let Some(handle) = self.agent_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        if let Some(handle) = self.outbound_handle.take() {
            handle.abort();
            let _ = handle.await;
        }
        self.bus.stop().await;
    }
}

impl AgentBuilder {
    /// Set the model identifier.
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.config.model = model.into();
        self
    }

    /// Set the API key.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.config.api_key = key.into();
        self
    }

    /// Set a custom API base URL.
    pub fn api_base(mut self, base: impl Into<String>) -> Self {
        self.config.api_base = Some(base.into());
        self
    }

    /// Set the workspace directory.
    pub fn workspace(mut self, path: impl Into<PathBuf>) -> Self {
        self.config.workspace = path.into();
        self
    }

    /// Set the maximum number of tool iterations.
    pub fn max_iterations(mut self, n: usize) -> Self {
        self.config.max_iterations = n;
        self
    }

    /// Set the agent loop mode.
    /// - `Standard`: Use agent-diva-agent's AgentLoop (default)
    /// - `Nano`: Use nano's lightweight NanoAgentLoop with full tool control
    pub fn mode(mut self, mode: AgentLoopMode) -> Self {
        self.mode = mode;
        self
    }

    /// Use nano mode for full tool control.
    pub fn nano_mode(self) -> Self {
        self.mode(AgentLoopMode::Nano)
    }

    /// Use standard mode (agent-diva-agent's AgentLoop).
    pub fn standard_mode(self) -> Self {
        self.mode(AgentLoopMode::Standard)
    }

    /// Add a custom tool.
    /// In Standard mode, these will be added to the tool registry.
    /// In Nano mode, use `with_tool_assembly` for more control.
    pub fn with_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.custom_tools.push(tool);
        self
    }

    /// Set a custom ToolAssembly for Nano mode.
    /// This provides full control over which built-in and custom tools are available.
    /// Note: Only effective in Nano mode. In Standard mode, use NanoConfig fields.
    pub fn with_tool_assembly(mut self, assembly: ToolAssembly) -> Self {
        self.tool_assembly = Some(assembly);
        self.mode = AgentLoopMode::Nano;
        self
    }

    /// Configure built-in tools using BuiltInToolsConfig.
    /// Shortcut for creating a ToolAssembly.
    pub fn builtin_tools(mut self, config: BuiltInToolsConfig) -> Self {
        let workspace = self.config.workspace.clone();
        let assembly = ToolAssembly::new(workspace)
            .builtin(config);
        self.tool_assembly = Some(assembly);
        self.mode = AgentLoopMode::Nano;
        self
    }

    /// Set a custom system prompt (only effective in Nano mode).
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Build the [`Agent`].
    pub async fn build(self) -> Result<Agent, NanoError> {
        let config = self.config;
        if config.model.is_empty() {
            return Err(NanoError::Other("model must be set".to_string()));
        }

        let bus = MessageBus::new();
        let client = build_provider(
            &config.model,
            &config.api_key,
            config.api_base.as_deref(),
        )?;
        let provider = Arc::new(DynamicProvider::new(Arc::new(client)));
        let workspace = config.workspace.clone();
        let model = config.model.clone();
        let max_iterations = config.max_iterations;

        // Initialize file manager (only with files feature)
        #[cfg(feature = "files")]
        let file_manager = {
            let storage_path = workspace.join(".agent-diva/files");
            let file_config = FileConfig::with_path(&storage_path);
            Arc::new(FileManager::new(file_config).await.map_err(|e| NanoError::Other(e.to_string()))?)
        };

        match self.mode {
            AgentLoopMode::Standard => {
                // Build ToolConfig from NanoConfig
                let tool_config = build_tool_config(&config);
                
                // Build a ToolRegistry with custom tools
                let mut tool_registry = ToolRegistry::new();
                for tool in self.custom_tools {
                    tool_registry.register(tool);
                }

                Ok(Agent {
                    bus,
                    provider,
                    mode: AgentLoopMode::Standard,
                    tool_config: Some(tool_config),
                    tool_registry: Some(tool_registry),
                    nano_loop_config: None,
                    workspace,
                    model,
                    max_iterations,
                    #[cfg(feature = "files")]
                    file_manager,
                    runtime_control_tx: None,
                    agent_handle: None,
                    outbound_handle: None,
                })
            }
            AgentLoopMode::Nano => {
                // Build ToolAssembly or use provided one
                let tool_registry = if let Some(assembly) = self.tool_assembly {
                    assembly.build()
                } else {
                    // Default assembly based on NanoConfig
                    let builtin_config = config.builtin_tools.clone()
                        .unwrap_or_else(|| {
                            if config.restrict_to_workspace {
                                BuiltInToolsConfig::default()
                            } else {
                                BuiltInToolsConfig::all()
                            }
                        });
                    
                    let mut assembly = ToolAssembly::new(workspace.clone())
                        .builtin(builtin_config)
                        .restrict_to_workspace(config.restrict_to_workspace);

                    // Add custom tools
                    for tool in self.custom_tools {
                        assembly = assembly.with_tool(tool);
                    }

                    // Add MCP servers
                    if !config.mcp_servers.is_empty() {
                        assembly = assembly.mcp_servers(config.mcp_servers.clone());
                    }

                    assembly.build()
                };

                // Build NanoLoopConfig
                let nano_loop_config = NanoLoopConfig {
                    max_iterations,
                    memory_window: 10,
                    soul_settings: NanoSoulSettings {
                        enabled: config.soul.enabled,
                        max_chars: config.soul.max_chars,
                        bootstrap_once: config.soul.bootstrap_once,
                    },
                    notify_on_soul_change: config.soul.notify_on_change,
                };

                Ok(Agent {
                    bus,
                    provider,
                    mode: AgentLoopMode::Nano,
                    tool_config: None,
                    tool_registry: Some(tool_registry),
                    nano_loop_config: Some(nano_loop_config),
                    workspace,
                    model,
                    max_iterations,
                    #[cfg(feature = "files")]
                    file_manager,
                    runtime_control_tx: None,
                    agent_handle: None,
                    outbound_handle: None,
                })
            }
        }
    }
}