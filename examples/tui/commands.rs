//! Command parsing.

pub enum Command {
    Send(String),
    Stop,
    SwitchModel(String),
    Config,
    NewSession,
    Clear,
    Quit,
}

pub fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();
    if trimmed == "/quit" {
        Command::Quit
    } else if trimmed == "/clear" {
        Command::Clear
    } else if trimmed == "/new" {
        Command::NewSession
    } else if trimmed == "/stop" {
        Command::Stop
    } else if trimmed == "/config" {
        Command::Config
    } else if let Some(rest) = trimmed.strip_prefix("/model ") {
        let model = rest.trim().to_string();
        if model.is_empty() {
            Command::Send(input.to_string())
        } else {
            Command::SwitchModel(model)
        }
    } else {
        Command::Send(input.to_string())
    }
}