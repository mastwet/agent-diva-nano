//! TUI application state and data types.

#[derive(Clone, Copy, PartialEq)]
pub enum TimelineKind {
    User,
    Assistant,
    Tool,
    System,
    Error,
    Thinking,
}

#[derive(Clone)]
pub struct TimelineItem {
    pub kind: TimelineKind,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum WizardStep {
    Model,
    ApiKey,
    ApiBase,
    Confirm,
}

#[derive(Clone, PartialEq)]
pub enum AppMode {
    Normal,
    ConfigWizard { step: WizardStep },
}

pub struct TuiApp {
    pub input: String,
    pub timeline: Vec<TimelineItem>,
    pub pending: bool,
    pub should_quit: bool,
    pub scroll: u16,
    pub assistant_line: Option<usize>,
    pub session_key: String,
    pub model: String,
    pub provider_name: String,
    pub stop_requested: bool,
    pub mode: AppMode,
    // Config wizard state
    pub wizard_model: String,
    pub wizard_api_key: String,
    pub wizard_api_base: String,
}

impl TuiApp {
    pub fn new(session_key: String, model: String, provider_name: String) -> Self {
        Self {
            input: String::new(),
            timeline: vec![TimelineItem {
                kind: TimelineKind::System,
                text: "Welcome to Agent Diva Nano TUI. Enter to send, Shift+Enter for newline. Commands: /quit /clear /new /stop /model <name> /config".to_string(),
            }],
            pending: false,
            should_quit: false,
            scroll: 0,
            assistant_line: None,
            session_key,
            model,
            provider_name,
            stop_requested: false,
            mode: AppMode::Normal,
            wizard_model: String::new(),
            wizard_api_key: String::new(),
            wizard_api_base: String::new(),
        }
    }

    pub fn enter_config_wizard(&mut self, current_config: Option<&agent_diva_nano::NanoConfig>) {
        self.mode = AppMode::ConfigWizard {
            step: WizardStep::Model,
        };
        if let Some(cfg) = current_config {
            self.wizard_model = cfg.model.clone();
            self.wizard_api_key = cfg.api_key.clone();
            self.wizard_api_base = cfg.api_base.clone().unwrap_or_default();
        } else {
            self.wizard_model.clear();
            self.wizard_api_key.clear();
            self.wizard_api_base.clear();
        }
        self.input.clear();
        self.timeline.push(TimelineItem {
            kind: TimelineKind::System,
            text: "--- Provider Configuration ---".to_string(),
        });
        self.add_line(TimelineKind::System, "Step 1/3: Enter the model identifier (e.g. deepseek-chat, gpt-4o, openrouter/anthropic/claude-sonnet-4)");
    }

    pub fn add_line(&mut self, kind: TimelineKind, text: impl Into<String>) {
        self.timeline.push(TimelineItem {
            kind,
            text: text.into(),
        });
        self.scroll_to_bottom();
    }

    pub fn scroll_to_bottom(&mut self) {
        let total_lines: u16 = self
            .timeline
            .iter()
            .map(|item| item.text.lines().count().max(1) as u16)
            .sum();
        self.scroll = total_lines;
    }

    pub fn apply_agent_event(&mut self, event: agent_diva_nano::AgentEvent) {
        use agent_diva_nano::AgentEvent;

        match event {
            AgentEvent::IterationStarted {
                index,
                max_iterations,
            } => {
                self.assistant_line = None;
                self.add_line(
                    TimelineKind::System,
                    format!("iteration {}/{}", index, max_iterations),
                );
            }
            AgentEvent::AssistantDelta { text } => {
                if text.is_empty() {
                    return;
                }
                if let Some(idx) = self.assistant_line {
                    if let Some(item) = self.timeline.get_mut(idx) {
                        item.text.push_str(&text);
                    }
                } else {
                    self.timeline.push(TimelineItem {
                        kind: TimelineKind::Assistant,
                        text,
                    });
                    self.assistant_line = Some(self.timeline.len() - 1);
                }
                self.scroll_to_bottom();
            }
            AgentEvent::ReasoningDelta { text } => {
                if text.is_empty() {
                    return;
                }
                self.assistant_line = None;
                if let Some(item) = self.timeline.last_mut() {
                    if matches!(item.kind, TimelineKind::Thinking) {
                        item.text.push_str(&text);
                        self.scroll_to_bottom();
                        return;
                    }
                }
                self.add_line(TimelineKind::Thinking, text);
            }
            AgentEvent::ToolCallStarted {
                name,
                args_preview,
                call_id,
            } => {
                self.assistant_line = None;
                self.add_line(
                    TimelineKind::Tool,
                    format!("[tool:start] {} [{}] {}", name, call_id, args_preview),
                );
            }
            AgentEvent::ToolCallFinished {
                name,
                result,
                is_error,
                call_id,
            } => {
                self.assistant_line = None;
                let (prefix, kind) = if is_error {
                    ("[tool:error]", TimelineKind::Error)
                } else {
                    ("[tool:done]", TimelineKind::Tool)
                };
                self.add_line(
                    kind,
                    format!("{} {} [{}] {}", prefix, name, call_id, result),
                );
            }
            AgentEvent::FinalResponse { .. } => {
                self.pending = false;
                self.assistant_line = None;
            }
            AgentEvent::Error { message } => {
                self.pending = false;
                self.assistant_line = None;
                self.add_line(TimelineKind::Error, format!("error: {}", message));
            }
            _ => {}
        }
    }

    pub fn input_title(&self) -> String {
        match self.mode {
            AppMode::Normal => "input (Enter send, Shift+Enter newline)".to_string(),
            AppMode::ConfigWizard { step } => match step {
                WizardStep::Model => "Step 1/3: Model (e.g. deepseek-chat)".to_string(),
                WizardStep::ApiKey => "Step 2/3: API Key".to_string(),
                WizardStep::ApiBase => {
                    "Step 3/3: API Base (optional, Enter to skip)".to_string()
                }
                WizardStep::Confirm => "Confirm: Enter to save, /cancel to reconfigure".to_string(),
            },
        }
    }
}