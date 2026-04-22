use crate::{Agent, NanoConfig, NanoError};
use agent_diva_core::bus::AgentEvent;
use tokio::sync::mpsc;

/// Send a single message and wait for the complete text response.
///
/// This is the simplest way to use the library — an agent is created,
/// started, the message is sent, and the agent is stopped automatically.
///
/// # Example
/// ```rust,no_run
/// use agent_diva_nano::{chat, NanoConfig};
///
/// # async fn example() -> Result<(), agent_diva_nano::NanoError> {
/// let config = NanoConfig {
///     model: "deepseek-chat".to_string(),
///     api_key: std::env::var("API_KEY").unwrap(),
///     ..Default::default()
/// };
/// let response = chat("Explain Rust ownership", &config).await?;
/// println!("{}", response);
/// # Ok(())
/// # }
/// ```
pub async fn chat(message: impl Into<String>, config: &NanoConfig) -> Result<String, NanoError> {
    let mut agent = Agent::new(config.clone()).build()?;
    agent.start().await?;
    let response = agent.send(message).await;
    agent.stop().await;
    response
}

/// Send a single message and receive a stream of [`AgentEvent`]s.
///
/// The returned channel yields events such as `AssistantDelta`,
/// `ToolCallStarted`, `ToolCallFinished`, and finally `FinalResponse`.
///
/// # Example
/// ```rust,no_run
/// use agent_diva_nano::{chat_stream, NanoConfig};
///
/// # async fn example() -> Result<(), agent_diva_nano::NanoError> {
/// let config = NanoConfig {
///     model: "deepseek-chat".to_string(),
///     api_key: std::env::var("API_KEY").unwrap(),
///     ..Default::default()
/// };
/// let mut rx = chat_stream("Explain Rust ownership", &config).await?;
/// while let Some(event) = rx.recv().await {
///     println!("{:?}", event);
/// }
/// # Ok(())
/// # }
/// ```
pub async fn chat_stream(
    message: impl Into<String>,
    config: &NanoConfig,
) -> Result<mpsc::UnboundedReceiver<AgentEvent>, NanoError> {
    let mut agent = Agent::new(config.clone()).build()?;
    agent.start().await?;
    let rx = agent.send_stream(message).await;
    // Note: the agent runs until all events are consumed.
    // In a real application you may want to keep the agent alive longer.
    rx
}
