//! Standalone TUI example for agent-diva-nano.
//!
//! No environment variables required — configuration is managed interactively
//! inside the TUI and persisted to `.nano/config.json`.
//!
//! Commands:
//! - `/quit`          – exit the TUI
//! - `/clear`         – clear the timeline
//! - `/new`           – start a new session
//! - `/stop`          – stop the current request (soft interrupt)
//! - `/model <name>`  – switch to a different model (rebuilds the agent)
//! - `/config`        – reconfigure provider (model / api_key / api_base)
//! - `Shift+Enter`    – insert a newline in the input area

mod app;
mod commands;
mod config;
mod manager;
mod provider;
mod ui;
mod wizard;

use agent_diva_nano::{AgentEvent, NanoConfig};
use crossterm::event::{self, Event as CEvent, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

use app::{AppMode, TuiApp};
use commands::parse_command;
use config::{load_initial_config, TuiConfigFile};
use manager::AgentManager;
use provider::resolve_provider_name;
use wizard::handle_wizard_step;

// ------------------------------------------------------------------
// Input handling
// ------------------------------------------------------------------

async fn handle_key_event(
    key: crossterm::event::KeyEvent,
    app: &mut TuiApp,
    manager: &mut AgentManager,
    current_rx: &mut Option<mpsc::UnboundedReceiver<AgentEvent>>,
) {
    if key.kind != KeyEventKind::Press {
        return;
    }
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Esc => app.should_quit = true,
        KeyCode::PageUp | KeyCode::Up => {
            app.scroll = app.scroll.saturating_sub(1);
        }
        KeyCode::PageDown | KeyCode::Down => {
            app.scroll = app.scroll.saturating_add(1);
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if app.mode == AppMode::Normal {
                app.input.push('\n');
            }
        }
        KeyCode::Enter => {
            let content = app.input.trim_end().to_string();
            app.input.clear();
            match app.mode.clone() {
                AppMode::ConfigWizard { step } => {
                    handle_wizard_step(step, content, app, manager).await;
                }
                AppMode::Normal => {
                    if content.is_empty() {
                        return;
                    }
                    handle_normal_command(content, app, manager, current_rx).await;
                }
            }
        }
        KeyCode::Backspace => {
            app.input.pop();
        }
        KeyCode::Char(ch) => {
            app.input.push(ch);
        }
        _ => {}
    }
}

async fn handle_normal_command(
    content: String,
    app: &mut TuiApp,
    manager: &mut AgentManager,
    current_rx: &mut Option<mpsc::UnboundedReceiver<AgentEvent>>,
) {
    use app::TimelineKind;
    use commands::Command;

    match parse_command(&content) {
        Command::Quit => app.should_quit = true,
        Command::Clear => {
            app.timeline.clear();
            app.assistant_line = None;
            app.scroll = 0;
        }
        Command::NewSession => {
            app.session_key = format!(
                "nano:tui:{}",
                chrono::Local::now().format("%Y%m%d%H%M%S")
            );
            app.assistant_line = None;
            app.add_line(
                TimelineKind::System,
                format!("new session: {}", app.session_key),
            );
        }
        Command::Stop => {
            if app.pending {
                app.stop_requested = true;
                app.pending = false;
                *current_rx = None;
                app.add_line(TimelineKind::System, "stop requested");
            }
        }
        Command::Config => {
            if app.pending {
                app.stop_requested = true;
                app.pending = false;
                *current_rx = None;
            }
            app.enter_config_wizard(Some(&manager.config));
        }
        Command::SwitchModel(model) => {
            if app.pending {
                app.stop_requested = true;
                app.pending = false;
                *current_rx = None;
            }
            app.add_line(
                TimelineKind::System,
                format!("switching to model: {}...", model),
            );
            match manager.switch_model(model.clone()).await {
                Ok(()) => {
                    app.model = model;
                    app.provider_name = resolve_provider_name(&app.model);
                    // Also save the updated model to config file
                    let file_config = TuiConfigFile::from_nano_config(&manager.config);
                    let _ = file_config.save();
                    app.add_line(TimelineKind::System, "model switched. ready.");
                }
                Err(e) => {
                    app.add_line(
                        TimelineKind::Error,
                        format!("failed to switch model: {}", e),
                    );
                }
            }
        }
        Command::Send(text) => {
            if app.pending {
                app.add_line(TimelineKind::System, "already processing. please wait.");
                return;
            }
            app.add_line(TimelineKind::User, text.clone());
            app.pending = true;
            app.assistant_line = None;
            app.stop_requested = false;

            if let Some(agent) = manager.agent() {
                match agent.send_stream(text).await {
                    Ok(rx) => {
                        *current_rx = Some(rx);
                    }
                    Err(e) => {
                        app.pending = false;
                        app.add_line(TimelineKind::Error, format!("failed to send: {}", e));
                    }
                }
            } else {
                app.pending = false;
                app.add_line(TimelineKind::Error, "agent not started".to_string());
            }
        }
    }
}

// ------------------------------------------------------------------
// Agent event receiver helper
// ------------------------------------------------------------------

async fn recv_agent_event(
    rx: &mut Option<mpsc::UnboundedReceiver<AgentEvent>>,
) -> Option<AgentEvent> {
    rx.as_mut()?.recv().await
}

// ------------------------------------------------------------------
// Main
// ------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Try to load config; if unavailable, start in wizard mode
    let (initial_config, start_in_wizard) = match load_initial_config() {
        Ok(cfg) => (cfg, false),
        Err(_) => (NanoConfig::default(), true),
    };

    let initial_model = initial_config.model.clone();
    let initial_provider = if initial_model.is_empty() {
        String::from("unknown")
    } else {
        resolve_provider_name(&initial_model)
    };

    let mut manager = AgentManager::new(initial_config.clone());
    if !start_in_wizard {
        manager.start().await?;
    }

    // 2. Initialize TUI
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = TuiApp::new(
        format!("nano:tui:{}", chrono::Local::now().format("%Y%m%d%H%M%S")),
        initial_model,
        initial_provider,
    );

    if start_in_wizard {
        app.enter_config_wizard(None);
    }

    // 3. Start keyboard event forwarding task
    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<crossterm::event::Event>();
    tokio::spawn(async move {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(50)) {
                if let Ok(evt) = event::read() {
                    if key_tx.send(evt).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // 4. Main event loop
    let mut current_rx: Option<mpsc::UnboundedReceiver<AgentEvent>> = None;

    let result: Result<(), Box<dyn std::error::Error>> = async {
        loop {
            terminal.draw(|frame| ui::draw_ui(frame, &app))?;

            tokio::select! {
                Some(event) = key_rx.recv() => {
                    if let CEvent::Key(key) = event {
                        handle_key_event(key, &mut app, &mut manager, &mut current_rx).await;
                    }
                }

                maybe_evt = recv_agent_event(&mut current_rx), if current_rx.is_some() => {
                    if let Some(evt) = maybe_evt {
                        let is_final = matches!(
                            evt,
                            AgentEvent::FinalResponse { .. } | AgentEvent::Error { .. }
                        );
                        if !app.stop_requested {
                            app.apply_agent_event(evt);
                        }
                        if is_final {
                            app.pending = false;
                            app.stop_requested = false;
                            current_rx = None;
                        }
                    } else {
                        app.pending = false;
                        current_rx = None;
                    }
                }
            }

            if app.should_quit {
                break Ok(());
            }
        }
    }
    .await;

    // 5. Cleanup
    manager.stop().await;
    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}