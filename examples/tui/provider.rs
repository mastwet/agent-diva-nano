//! Provider resolution.

use agent_diva_nano::ProviderRegistry;

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