use crate::{NanoConfig, NanoError};
use crate::internal::provider::{build_provider, build_tool_config};
use agent_diva_agent::AgentLoop;
use agent_diva_core::bus::{AgentEvent, InboundMessage, MessageBus};
use agent_diva_files::{FileManager, FileConfig};
use agent_diva_providers::DynamicProvider;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info};

/// A running agent instance.
///
/// Create with [`Agent::new`](Agent::new) and the builder pattern,
/// then call [`start`](Agent::start) to run the background loop.
pub struct Agent {
    bus: MessageBus,
    provider: Arc<DynamicProvider>,
    tool_config: agent_diva_agent::ToolConfig,
    workspace: PathBuf,
    model: String,
    max_iterations: usize,
    file_manager: Arc<FileManager>,
    runtime_control_tx: Option<mpsc::UnboundedSender<agent_diva_agent::RuntimeControlCommand>>,
    agent_handle: Option<JoinHandle<()>>,
    outbound_handle: Option<JoinHandle<()>>,
}

/// Builder for configuring an [`Agent`].
pub struct AgentBuilder {
    config: NanoConfig,
    custom_tools: Vec<Arc<dyn agent_diva_tools::Tool>>,
}

impl Agent {
    /// Start configuring a new agent.
    pub fn new(config: NanoConfig) -> AgentBuilder {
        AgentBuilder {
            config,
            custom_tools: Vec::new(),
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
        let tool_config = self.tool_config.clone();
        let max_iterations = self.max_iterations;
        let file_manager = self.file_manager.clone();

        let (runtime_control_tx, runtime_control_rx) = mpsc::unbounded_channel();
        self.runtime_control_tx = Some(runtime_control_tx);

        let mut agent_loop = AgentLoop::with_tools(
            bus.clone(),
            provider,
            workspace,
            Some(model),
            Some(max_iterations),
            tool_config,
            Some(runtime_control_rx),
            file_manager,
        ).await.map_err(|e| NanoError::Other(e.to_string()))?;

        let agent_handle = tokio::spawn(async move {
            info!("Agent loop starting");
            if let Err(e) = agent_loop.run().await {
                error!("Agent loop error: {}", e);
            }
            info!("Agent loop stopped");
        });

        let bus_for_outbound = self.bus.clone();
        let outbound_handle = tokio::spawn(async move {
            bus_for_outbound.dispatch_outbound_loop().await;
        });

        self.agent_handle = Some(agent_handle);
        self.outbound_handle = Some(outbound_handle);

        Ok(())
    }

    /// Send a message and wait for the complete text response.
    pub async fn send(&self, message: impl Into<String>) -> Result<String, NanoError> {
        let content = message.into();
        let channel = "nano";
        let chat_id = "default";

        // Subscribe before publishing to avoid race conditions
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

    /// Stop the background agent loop.
    pub async fn stop(&mut self) {
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

    /// Add a custom tool.
    pub fn with_tool(mut self, tool: Arc<dyn agent_diva_tools::Tool>) -> Self {
        self.custom_tools.push(tool);
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
        let tool_config = build_tool_config(&config);
        let workspace = config.workspace.clone();
        let model = config.model.clone();
        let max_iterations = config.max_iterations;

        // Initialize file manager for attachment handling
        let storage_path = workspace.join(".agent-diva/files");
        let file_config = FileConfig::with_path(&storage_path);
        let file_manager = Arc::new(FileManager::new(file_config).await.map_err(|e| NanoError::Other(e.to_string()))?);

        Ok(Agent {
            bus,
            provider,
            tool_config,
            workspace,
            model,
            max_iterations,
            file_manager,
            runtime_control_tx: None,
            agent_handle: None,
            outbound_handle: None,
        })
    }
}
