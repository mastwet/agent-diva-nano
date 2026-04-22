//! Lightweight agent loop implementation for agent-diva-nano.
//!
//! This module provides a simplified agent loop that allows complete control
//! over tool registration, enabling fine-grained assembly of available tools.

use agent_diva_core::bus::{AgentEvent, InboundMessage, MessageBus, OutboundMessage};
use agent_diva_core::error_context::ErrorContext;
use agent_diva_core::session::SessionManager;
use agent_diva_files::FileManager;
use agent_diva_providers::{LLMProvider, LLMStreamEvent, ProviderEventStream, ToolCallRequest};
use agent_diva_tools::ToolRegistry;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn, Instrument};

use crate::internal::context::{NanoContextBuilder, NanoSoulSettings};

/// Configuration for the nano agent loop.
#[derive(Clone)]
pub struct NanoLoopConfig {
    /// Maximum tool-call iterations per turn.
    pub max_iterations: usize,
    /// Memory window for context consolidation.
    pub memory_window: usize,
    /// Soul context settings.
    pub soul_settings: NanoSoulSettings,
    /// Notify on soul changes.
    pub notify_on_soul_change: bool,
}

impl Default for NanoLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 20,
            memory_window: 10,
            soul_settings: NanoSoulSettings::default(),
            notify_on_soul_change: true,
        }
    }
}

/// Lightweight agent loop with customizable tool registry.
pub struct NanoAgentLoop {
    bus: MessageBus,
    provider: Arc<dyn LLMProvider>,
    workspace: PathBuf,
    model: String,
    config: NanoLoopConfig,
    sessions: SessionManager,
    tools: ToolRegistry,
    context: NanoContextBuilder,
    file_manager: Arc<FileManager>,
    cancelled_sessions: HashSet<String>,
    runtime_control_rx: Option<mpsc::UnboundedReceiver<NanoRuntimeControlCommand>>,
}

/// Runtime control commands for nano agent loop.
pub enum NanoRuntimeControlCommand {
    /// Cancel a specific session.
    CancelSession { chat_id: String },
    /// Stop the agent loop entirely.
    Stop,
    /// Reload tools from assembly.
    ReloadTools(ToolRegistry),
}

impl NanoAgentLoop {
    /// Create a new nano agent loop with a pre-built tool registry.
    pub async fn new(
        bus: MessageBus,
        provider: Arc<dyn LLMProvider>,
        workspace: PathBuf,
        model: Option<String>,
        config: NanoLoopConfig,
        tools: ToolRegistry,
        file_manager: Arc<FileManager>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let model = model.unwrap_or_else(|| provider.get_default_model());
        let context = NanoContextBuilder::new(workspace.clone())
            .with_soul_settings(config.soul_settings.clone());
        let sessions = SessionManager::new(workspace.clone());

        Ok(Self {
            bus,
            provider,
            workspace,
            model,
            config,
            sessions,
            tools,
            context,
            file_manager,
            cancelled_sessions: HashSet::new(),
            runtime_control_rx: None,
        })
    }

    /// Create with runtime control channel.
    pub fn with_runtime_control(
        mut self,
        rx: mpsc::UnboundedReceiver<NanoRuntimeControlCommand>,
    ) -> Self {
        self.runtime_control_rx = Some(rx);
        self
    }

    /// Get the tool registry.
    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    /// Get mutable tool registry for dynamic modification.
    pub fn tools_mut(&mut self) -> &mut ToolRegistry {
        &mut self.tools
    }

    /// Get the file manager.
    pub fn file_manager(&self) -> Arc<FileManager> {
        self.file_manager.clone()
    }

    /// Run the agent loop, processing messages from the bus.
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Nano agent loop started");

        let Some(mut inbound_rx) = self.bus.take_inbound_receiver().await else {
            error!("Failed to take inbound receiver");
            return Err("Inbound receiver already taken".into());
        };

        loop {
            if let Some(control_rx) = self.runtime_control_rx.as_mut() {
                tokio::select! {
                    control = control_rx.recv() => {
                        match control {
                            Some(cmd) => {
                                if self.handle_runtime_control(cmd) {
                                    info!("Nano agent loop stopped via control command");
                                    return Ok(());
                                }
                            }
                            None => {
                                info!("Runtime control channel closed");
                                self.runtime_control_rx = None;
                            }
                        }
                    }
                    maybe_msg = inbound_rx.recv() => {
                        match maybe_msg {
                            Some(msg) => self.handle_inbound(msg).await,
                            None => {
                                info!("Message bus closed, stopping nano agent loop");
                                break;
                            }
                        }
                    }
                }
            } else {
                match tokio::time::timeout(Duration::from_secs(1), inbound_rx.recv()).await {
                    Ok(Some(msg)) => self.handle_inbound(msg).await,
                    Ok(None) => {
                        info!("Message bus closed, stopping nano agent loop");
                        break;
                    }
                    Err(_) => continue,
                }
            }
        }

        info!("Nano agent loop stopped");
        Ok(())
    }

    /// Handle runtime control command.
    /// Returns true if the loop should stop.
    fn handle_runtime_control(&mut self, cmd: NanoRuntimeControlCommand) -> bool {
        match cmd {
            NanoRuntimeControlCommand::CancelSession { chat_id } => {
                let chat_id_clone = chat_id.clone();
                self.cancelled_sessions.insert(chat_id);
                info!("Session {} marked for cancellation", chat_id_clone);
                false
            }
            NanoRuntimeControlCommand::Stop => true,
            NanoRuntimeControlCommand::ReloadTools(new_registry) => {
                self.tools = new_registry;
                info!("Tools reloaded, now have {} tools", self.tools.len());
                false
            }
        }
    }

    /// Handle an inbound message.
    async fn handle_inbound(&mut self, msg: InboundMessage) {
        debug!("Received message from {}:{}", msg.channel, msg.chat_id);
        
        if self.cancelled_sessions.contains(&msg.chat_id) {
            self.cancelled_sessions.remove(&msg.chat_id);
            self.emit_event(&msg, AgentEvent::Error {
                message: "Session was cancelled".to_string(),
            });
            return;
        }

        let event_msg = msg.clone();
        match self.process_inbound_message(msg).await {
            Ok(Some(response)) => {
                if let Err(e) = self.bus.publish_outbound(response) {
                    error!("Failed to publish response: {}", e);
                }
            }
            Ok(None) => debug!("No response needed"),
            Err(e) => {
                let error_message = format!("Failed to process message: {}", e);
                let ctx = ErrorContext::new("handle_inbound", &error_message)
                    .with_metadata("channel", event_msg.channel.clone())
                    .with_metadata("chat_id", event_msg.chat_id.clone())
                    .with_metadata("sender_id", event_msg.sender_id.clone());
                error!("{}", ctx.to_detailed_string());
                self.emit_error_event(&event_msg, error_message);
            }
        }
    }

    /// Process a single inbound message.
    async fn process_inbound_message(
        &mut self,
        msg: InboundMessage,
    ) -> Result<Option<OutboundMessage>, Box<dyn std::error::Error>> {
        let trace_id = uuid::Uuid::new_v4().to_string();
        let span = tracing::info_span!("NanoAgentSpan", trace_id = %trace_id);

        self.process_turn(msg, trace_id).instrument(span).await
    }

    /// Process a turn of conversation.
    async fn process_turn(
        &mut self,
        msg: InboundMessage,
        trace_id: String,
    ) -> Result<Option<OutboundMessage>, Box<dyn std::error::Error>> {
        // Build context for the turn
        let session_key = format!("{}:{}", msg.channel, msg.chat_id);
        let session = self.sessions.get_or_create(&session_key);

        // Build messages for LLM
        let messages = self.context.build_messages(
            &msg,
            session,
            &self.tools,
            self.config.memory_window,
        )?;

        // Get tool definitions
        let tool_defs = self.tools.get_definitions();
        let tools_param = if tool_defs.is_empty() {
            None
        } else {
            Some(tool_defs)
        };

        // Stream from provider
        let stream = self.provider.chat_stream(
            messages,
            tools_param,
            Some(self.model.clone()),
            4096,
            0.7,
        ).await?;

        // Process the stream and handle tool calls
        self.process_stream(stream, msg, session_key, trace_id).await
    }

    /// Process streaming response from provider.
    async fn process_stream(
        &mut self,
        stream: ProviderEventStream,
        msg: InboundMessage,
        session_key: String,
        trace_id: String,
    ) -> Result<Option<OutboundMessage>, Box<dyn std::error::Error>> {
        use futures::StreamExt;
        let mut stream = stream;
        let mut full_content = String::new();
        let mut reasoning_content = String::new();
        let mut tool_calls: Vec<ToolCallRequest> = Vec::new();
        let mut tool_call_accumulator: std::collections::HashMap<usize, (Option<String>, Option<String>, String)> = std::collections::HashMap::new();
        let mut iteration_count = 0;

        loop {
            match tokio::time::timeout(Duration::from_secs(120), stream.next()).await {
                Ok(Some(event)) => {
                    match event {
                        Ok(LLMStreamEvent::TextDelta(delta)) => {
                            full_content.push_str(&delta);
                            self.emit_event(&msg, AgentEvent::AssistantDelta { text: delta });
                        }
                        Ok(LLMStreamEvent::ReasoningDelta(delta)) => {
                            reasoning_content.push_str(&delta);
                        }
                        Ok(LLMStreamEvent::ToolCallDelta { index, id, name, arguments_delta }) => {
                            // Accumulate tool call deltas by index
                            let entry = tool_call_accumulator.entry(index).or_insert((None, None, String::new()));
                            if let Some(id) = id {
                                entry.0 = Some(id);
                            }
                            if let Some(name) = name {
                                entry.1 = Some(name);
                            }
                            if let Some(args) = arguments_delta {
                                entry.2.push_str(&args);
                            }
                        }
                        Ok(LLMStreamEvent::Completed(response)) => {
                            // Build tool calls from accumulated deltas or response
                            if tool_call_accumulator.is_empty() && !response.tool_calls.is_empty() {
                                tool_calls = response.tool_calls.clone();
                            } else {
                                // Build from accumulator
                                for (_, (id, name, args)) in tool_call_accumulator.drain() {
                                    if let (Some(id), Some(name)) = (id, name) {
                                        let arguments = serde_json::from_str(&args)
                                            .unwrap_or(std::collections::HashMap::new());
                                        tool_calls.push(ToolCallRequest {
                                            id,
                                            call_type: "function".to_string(),
                                            name,
                                            arguments,
                                        });
                                    }
                                }
                            }

                            // Check if we have tool calls to execute
                            if !tool_calls.is_empty() && iteration_count < self.config.max_iterations {
                                iteration_count += 1;
                                
                                // Execute tool calls
                                let tool_results = self.execute_tool_calls(&tool_calls, &msg).await;
                                
                                // Build next request with tool results
                                // (This is simplified - full implementation would need proper context management)
                                tool_calls.clear();
                                tool_call_accumulator.clear();
                                continue;
                            }
                            
                            // Final response - use response content if available
                            let final_content = response.content.clone().unwrap_or(full_content.clone());
                            
                            self.emit_event(&msg, AgentEvent::FinalResponse {
                                content: final_content.clone(),
                            });

                            // Update session
                            if let Some(session) = self.sessions.get(&session_key) {
                                // Clone session to add message since we can't modify through &Session
                                let mut session_clone = session.clone();
                                session_clone.add_message("user", msg.content.clone());
                                session_clone.add_message("assistant", final_content.clone());
                                self.sessions.save(&session_clone)?;
                            }

                            let mut outbound = OutboundMessage::new(
                                &msg.channel,
                                &msg.chat_id,
                                final_content,
                            );
                            if !reasoning_content.is_empty() {
                                outbound.reasoning_content = Some(reasoning_content);
                            }
                            return Ok(Some(outbound));
                        }
                        Err(e) => {
                            self.emit_error_event(&msg, e.to_string());
                            return Err(e.into());
                        }
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    warn!("Stream timeout for trace {}", trace_id);
                    self.emit_error_event(&msg, "Stream timeout".to_string());
                    return Err("Stream timeout".into());
                }
            }
        }

        Ok(None)
    }

    /// Execute tool calls and return results.
    async fn execute_tool_calls(
        &mut self,
        tool_calls: &[ToolCallRequest],
        msg: &InboundMessage,
    ) -> Vec<(String, String)> {
        let mut results = Vec::new();

        for tc in tool_calls {
            // Build args_preview from arguments
            let args_preview = serde_json::to_string(&tc.arguments)
                .unwrap_or_default()
                .chars()
                .take(100)
                .collect();
            
            self.emit_event(&msg, AgentEvent::ToolCallStarted {
                name: tc.name.clone(),
                args_preview,
                call_id: tc.id.clone(),
            });

            // Convert arguments HashMap to JSON Value
            let params = serde_json::to_value(&tc.arguments).unwrap_or(serde_json::Value::Null);

            let result = self.tools.execute(&tc.name, params).await;
            let is_error = result.starts_with("Error");

            self.emit_event(&msg, AgentEvent::ToolCallFinished {
                name: tc.name.clone(),
                result: result.clone(),
                is_error,
                call_id: tc.id.clone(),
            });

            results.push((tc.id.clone(), result));
        }

        results
    }

    /// Emit an event to the bus.
    fn emit_event(&self, msg: &InboundMessage, event: AgentEvent) {
        if let Err(e) = self.bus.publish_event(&msg.channel, &msg.chat_id, event) {
            warn!("Failed to emit event: {}", e);
        }
    }

    /// Emit an error event.
    fn emit_error_event(&self, msg: &InboundMessage, message: String) {
        self.emit_event(msg, AgentEvent::Error { message });
    }

    /// Process a message directly (for CLI or testing).
    pub async fn process_direct(
        &mut self,
        content: impl Into<String>,
        channel: impl Into<String>,
        chat_id: impl Into<String>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let content = content.into();
        let channel = channel.into();
        let chat_id = chat_id.into();

        let msg = InboundMessage::new(channel, "user", chat_id, content);

        let response = self.process_inbound_message(msg).await?;
        Ok(response
            .map(|r| {
                let content = r.content;
                if let Some(reasoning) = r.reasoning_content {
                    if !reasoning.is_empty() {
                        return format!("{}\n\n{}", reasoning, content);
                    }
                }
                content
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_diva_providers::{LLMResponse, Message, ProviderError, ProviderResult};
    use async_trait::async_trait;
    use futures::stream;

    struct MockProvider;

    #[async_trait]
    impl LLMProvider for MockProvider {
        async fn chat(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<serde_json::Value>>,
            _model: Option<String>,
            _max_tokens: i32,
            _temperature: f64,
        ) -> ProviderResult<LLMResponse> {
            Ok(LLMResponse {
                content: "mock response".to_string(),
                reasoning_content: None,
                tool_calls: Vec::new(),
            })
        }

        async fn chat_stream(
            &self,
            _messages: Vec<Message>,
            _tools: Option<Vec<serde_json::Value>>,
            _model: Option<String>,
            _max_tokens: i32,
            _temperature: f64,
        ) -> ProviderResult<ProviderEventStream> {
            Ok(Box::pin(stream::iter(vec![
                Ok(ProviderEvent::TextDelta { delta: "mock".to_string() }),
                Ok(ProviderEvent::Done),
            ])))
        }

        fn get_default_model(&self) -> String {
            "mock-model".to_string()
        }
    }

    #[tokio::test]
    async fn test_nano_agent_loop_creation() {
        let bus = MessageBus::new();
        let provider = Arc::new(MockProvider);
        let workspace = PathBuf::from("/tmp/test");
        let tools = ToolRegistry::new();
        let storage_path = workspace.join(".agent-diva/files");
        let file_config = agent_diva_files::FileConfig::with_path(&storage_path);
        let file_manager = Arc::new(FileManager::new(file_config).await.unwrap());

        let agent = NanoAgentLoop::new(
            bus,
            provider,
            workspace,
            None,
            NanoLoopConfig::default(),
            tools,
            file_manager,
        ).await;

        assert!(agent.is_ok());
        let agent = agent.unwrap();
        assert_eq!(agent.config.max_iterations, 20);
    }
}