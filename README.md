# agent-diva-nano

`agent-diva-nano` is the lightweight starter line for building an Agent Diva
runtime outside the main monorepo. It exposes a compact agent API, a simple
`chat()` entry point, and opt-in built-in tools on top of the published
`agent-diva-*` crates.

## Who this is for

- Library consumers that want a small Agent Diva entry point.
- Template or starter projects that do not want the full manager-backed CLI.
- Experiments that need direct control over provider config and tool assembly.

This crate is not the main `agent-diva` product CLI. The full manager-backed
command-line application lives in `agent-diva-cli`.

## Crates.io usage

```toml
[dependencies]
agent-diva-nano = "0.4.11"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

```rust,no_run
use agent_diva_nano::{chat, NanoConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = NanoConfig::from_env()?;
    let reply = chat("Explain Rust ownership.", &config).await?;
    println!("{reply}");
    Ok(())
}
```

Required environment variables for `NanoConfig::from_env()`:

- `NANO_MODEL`
- `NANO_API_KEY`
- `NANO_API_BASE` (optional)

## Optional capabilities

- Enable the default `files` feature to re-export `agent-diva-files` support.
- Use `AgentBuilder`, `ToolAssembly`, and `BuiltInToolsConfig` when you need a
  longer-lived runtime or custom tool topology.

## Monorepo development

- This crate remains outside the root workspace `members`.
- Local validation from this repository can use:
  `cargo check --manifest-path .workspace/agent-diva-nano/Cargo.toml`
- The local manifest includes `patch.crates-io` entries only to support
  monorepo development. Published consumers resolve normal crates.io versions.

See [../README.md](../README.md) and [docs/dev/agent-diva-nano-extracted.md](../../docs/dev/agent-diva-nano-extracted.md).
