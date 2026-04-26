//! Context builder for nano agent loop.

use agent_diva_core::bus::InboundMessage;
use agent_diva_core::session::Session;
use agent_diva_providers::Message;
use agent_diva_tooling::ToolRegistry;
use std::path::PathBuf;

/// Soul settings for nano context.
#[derive(Clone, Debug)]
pub struct NanoSoulSettings {
    /// Whether soul context is enabled.
    pub enabled: bool,
    /// Maximum characters for soul context.
    pub max_chars: usize,
    /// Bootstrap soul only once.
    pub bootstrap_once: bool,
}

impl Default for NanoSoulSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_chars: 4000,
            bootstrap_once: true,
        }
    }
}

/// Context builder for building messages sent to the LLM.
pub struct NanoContextBuilder {
    workspace: PathBuf,
    soul_settings: NanoSoulSettings,
    soul_content: Option<String>,
    system_prompt: Option<String>,
}

impl NanoContextBuilder {
    /// Create a new context builder.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            soul_settings: NanoSoulSettings::default(),
            soul_content: None,
            system_prompt: None,
        }
    }

    /// Set soul settings.
    pub fn with_soul_settings(mut self, settings: NanoSoulSettings) -> Self {
        self.soul_settings = settings;
        self
    }

    /// Set custom system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Load soul content from SOUL.md.
    pub fn load_soul(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.soul_settings.enabled {
            return Ok(());
        }

        let soul_path = self.workspace.join("SOUL.md");
        if soul_path.exists() {
            let content = std::fs::read_to_string(&soul_path)?;
            let truncated = if content.len() > self.soul_settings.max_chars {
                content.chars().take(self.soul_settings.max_chars).collect()
            } else {
                content
            };
            self.soul_content = Some(truncated);
        }
        Ok(())
    }

    /// Build messages for the LLM.
    pub fn build_messages(
        &self,
        msg: &InboundMessage,
        session: &Session,
        tools: &ToolRegistry,
        memory_window: usize,
    ) -> Result<Vec<Message>, Box<dyn std::error::Error>> {
        let mut messages = Vec::new();

        // System message with tools and optional soul
        let system = self.build_system_message(tools)?;
        messages.push(Message::system(system));

        // Session history (limited by memory window)
        let history = session.get_history(memory_window);
        for chat_msg in history.iter() {
            match chat_msg.role.as_str() {
                "user" => {
                    messages.push(Message::user(&chat_msg.content));
                }
                "assistant" => {
                    // Include reasoning content if present
                    let content = if let Some(ref reasoning) = chat_msg.reasoning_content {
                        if !reasoning.is_empty() {
                            format!("{}\n\n{}", reasoning, chat_msg.content)
                        } else {
                            chat_msg.content.clone()
                        }
                    } else {
                        chat_msg.content.clone()
                    };
                    let mut msg = Message::assistant(content);
                    // Include tool calls if present
                    if let Some(ref tool_calls) = chat_msg.tool_calls {
                        // Convert to ToolCallRequest format if needed
                        msg.tool_calls = Some(
                            tool_calls.iter()
                                .filter_map(|tc| {
                                    // Try to parse as ToolCallRequest
                                    serde_json::from_value(tc.clone()).ok()
                                })
                                .collect()
                        );
                    }
                    messages.push(msg);
                }
                "tool" => {
                    // Tool result message
                    if let Some(ref tool_call_id) = chat_msg.tool_call_id {
                        messages.push(Message::tool(&chat_msg.content, tool_call_id));
                    }
                }
                _ => {}
            }
        }

        // Current user message
        messages.push(Message::user(&msg.content));

        Ok(messages)
    }

    /// Build system message.
    fn build_system_message(&self, tools: &ToolRegistry) -> Result<String, Box<dyn std::error::Error>> {
        let mut system = String::new();

        // Add custom system prompt or default
        if let Some(ref prompt) = self.system_prompt {
            system.push_str(prompt);
            system.push_str("\n\n");
        } else {
            system.push_str("You are a helpful AI assistant.\n\n");
        }

        // Add soul context
        if let Some(ref soul) = self.soul_content {
            system.push_str("--- Identity Context ---\n");
            system.push_str(soul);
            system.push_str("\n--- End Identity Context ---\n\n");
        }

        // Add tool descriptions if any
        if !tools.is_empty() {
            system.push_str("You have access to the following tools:\n");
            for name in tools.tool_names() {
                if let Some(tool) = tools.get(&name) {
                    system.push_str(&format!("- {}: {}\n", tool.name(), tool.description()));
                }
            }
            system.push_str("\nWhen you need to use a tool, the model will generate a tool call with the tool name and arguments.\n");
        }

        Ok(system)
    }
}
