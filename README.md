# agent-diva-nano

Standalone **nano / template-line** runtime: local gateway + HTTP control plane, sharing Agent Diva core crates via **path dependencies** into the parent monorepo.

- **Do not** add this crate to the root workspace `members`.
- Build from monorepo root: `cargo check --manifest-path .workspace/agent-diva-nano/Cargo.toml`.
- The main **`agent-diva`** CLI is **`agent-diva-cli`** in the root workspace (manager-backed); it does not link this crate.

See [../README.md](../README.md) and [docs/dev/agent-diva-nano-extracted.md](../../docs/dev/agent-diva-nano-extracted.md).
