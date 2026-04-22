use crate::{NanoConfig, NanoError};
use agent_diva_agent::{
    tool_config::network::{
        NetworkToolConfig, WebFetchRuntimeConfig, WebRuntimeConfig, WebSearchRuntimeConfig,
    },
    context::SoulContextSettings,
    agent_loop::SoulGovernanceSettings,
    ToolConfig,
};
use agent_diva_providers::{LiteLLMClient, ProviderRegistry};

/// Resolve a provider name from the model identifier.
pub fn resolve_provider_name(model: &str) -> String {
    let registry = ProviderRegistry::new();
    model
        .split('/')
        .next()
        .and_then(|prefix| registry.find_by_name(prefix))
        .or_else(|| registry.find_by_model(model))
        .map(|spec| spec.name.clone())
        .unwrap_or_else(|| "openai".to_string())
}

/// Build an LLM provider client from minimal config.
pub fn build_provider(
    model: &str,
    api_key: &str,
    api_base: Option<&str>,
) -> Result<LiteLLMClient, NanoError> {
    let provider_name = resolve_provider_name(model);
    let api_key = if api_key.is_empty() {
        None
    } else {
        Some(api_key.to_string())
    };
    let api_base = api_base.filter(|s| !s.is_empty()).map(|s| s.to_string());

    Ok(LiteLLMClient::new(
        api_key,
        api_base,
        model.to_string(),
        None,
        Some(provider_name),
        None,
    ))
}

/// Build `ToolConfig` from `NanoConfig` (for Standard mode).
pub fn build_tool_config(config: &NanoConfig) -> ToolConfig {
    let network = if let Some(ref search) = config.web_search {
        NetworkToolConfig {
            web: WebRuntimeConfig {
                search: WebSearchRuntimeConfig {
                    provider: search.provider.clone(),
                    enabled: true,
                    api_key: search.api_key.clone(),
                    max_results: search.max_results,
                },
                fetch: WebFetchRuntimeConfig { enabled: true },
            },
        }
    } else {
        NetworkToolConfig::default()
    };

    ToolConfig {
        network,
        exec_timeout: config.exec_timeout,
        restrict_to_workspace: config.restrict_to_workspace,
        mcp_servers: config.mcp_servers.clone(),
        cron_service: None,
        soul_context: SoulContextSettings {
            enabled: config.soul.enabled,
            max_chars: config.soul.max_chars,
            bootstrap_once: config.soul.bootstrap_once,
        },
        notify_on_soul_change: config.soul.notify_on_change,
        soul_governance: SoulGovernanceSettings {
            frequent_change_window_secs: config.soul.frequent_change_window_secs,
            frequent_change_threshold: config.soul.frequent_change_threshold,
            boundary_confirmation_hint: config.soul.boundary_confirmation_hint,
        },
    }
}

/// Build `ToolConfig` from ToolAssembly (placeholder for future use).
/// Note: In Nano mode, ToolAssembly directly builds a ToolRegistry,
/// so this function is primarily for potential future Standard mode integration.
pub fn build_tool_config_from_assembly(
    _assembly: &crate::tool_assembly::ToolAssembly,
) -> ToolConfig {
    ToolConfig::default()
}
