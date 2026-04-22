//! Simple CLI example using the nano agent library.
//!
//! Run with:
//! ```bash
//! NANO_MODEL=deepseek-chat NANO_API_KEY=sk-xxx cargo run --example chat -- "Explain Rust ownership"
//! ```

use agent_diva_nano::{chat, NanoConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let message = args.get(1).map(|s| s.as_str()).unwrap_or("Hello!");

    let config = NanoConfig::from_env()?;

    let response = chat(message, &config).await?;
    println!("{}", response);

    Ok(())
}
