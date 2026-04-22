use crate::{run_server, AppState, Manager};
use agent_diva_agent::{
    agent_loop::SoulGovernanceSettings, context::SoulContextSettings,
    tool_config::network::NetworkToolConfig, tool_config::network::WebFetchRuntimeConfig,
    tool_config::network::WebRuntimeConfig, tool_config::network::WebSearchRuntimeConfig,
    AgentLoop, ToolConfig,
};
use agent_diva_channels::ChannelManager;
use agent_diva_core::bus::{InboundMessage, MessageBus};
use agent_diva_core::config::{Config, ConfigLoader};
use agent_diva_core::cron::service::JobCallback;
use agent_diva_core::cron::CronService;
use agent_diva_providers::{
    DynamicProvider, LLMProvider, LiteLLMClient, ProviderAccess, ProviderCatalogService,
    ProviderRegistry,
};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::error;

#[derive(Clone)]
pub struct GatewayRuntimeConfig {
    pub config: Config,
    pub loader: ConfigLoader,
    pub workspace: PathBuf,
    pub cron_store: PathBuf,
    pub port: u16,
}

fn provider_registry() -> ProviderRegistry {
    ProviderRegistry::new()
}

fn infer_provider_name_from_model(model: &str) -> Option<String> {
    let registry = provider_registry();
    model
        .split('/')
        .next()
        .and_then(|prefix| registry.find_by_name(prefix))
        .or_else(|| registry.find_by_model(model))
        .map(|spec| spec.name.clone())
}

fn current_provider_name(config: &Config) -> Option<String> {
    let preferred_provider = config
        .agents
        .defaults
        .provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    preferred_provider.or_else(|| infer_provider_name_from_model(&config.agents.defaults.model))
}

fn resolve_provider_name_for_model(
    config: &Config,
    model: &str,
    preferred_provider: Option<&str>,
) -> Option<String> {
    let inferred_provider = infer_provider_name_from_model(model);
    if let Some(provider_name) = preferred_provider
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let registry = provider_registry();
        if registry.find_by_name(provider_name).is_some() {
            return Some(provider_name.to_string());
        }
        if inferred_provider.as_deref() == Some(provider_name) {
            return inferred_provider;
        }
    }

    inferred_provider.or_else(|| {
        (model == config.agents.defaults.model)
            .then(|| current_provider_name(config))
            .flatten()
    })
}

fn build_provider(config: &Config, model: &str) -> Result<LiteLLMClient> {
    let catalog = ProviderCatalogService::new();
    let provider_name = resolve_provider_name_for_model(
        config,
        model,
        (model == config.agents.defaults.model)
            .then_some(config.agents.defaults.provider.as_deref())
            .flatten(),
    )
    .ok_or_else(|| anyhow::anyhow!("No provider found for model: {}", model))?;
    let access = catalog
        .get_provider_access(config, &provider_name)
        .unwrap_or_else(|| ProviderAccess::from_config(None));
    let extra_headers = (!access.extra_headers.is_empty()).then(|| {
        access
            .extra_headers
            .into_iter()
            .collect::<std::collections::HashMap<String, String>>()
    });

    Ok(LiteLLMClient::new(
        access.api_key,
        access.api_base,
        model.to_string(),
        extra_headers,
        Some(provider_name),
        config.agents.defaults.reasoning_effort.clone(),
    ))
}

fn build_network_tool_config(config: &Config) -> NetworkToolConfig {
    let api_key = config.tools.web.search.api_key.trim().to_string();
    NetworkToolConfig {
        web: WebRuntimeConfig {
            search: WebSearchRuntimeConfig {
                provider: config.tools.web.search.provider.clone(),
                enabled: config.tools.web.search.enabled,
                api_key: if api_key.is_empty() {
                    None
                } else {
                    Some(api_key)
                },
                max_results: config.tools.web.search.max_results,
            },
            fetch: WebFetchRuntimeConfig {
                enabled: config.tools.web.fetch.enabled,
            },
        },
    }
}

pub async fn run_local_gateway(runtime: GatewayRuntimeConfig) -> Result<()> {
    let config = runtime.config;

    let bus = MessageBus::new();

    let cron_store = runtime.cron_store;
    let bus_for_cron = bus.clone();
    let cron_callback: JobCallback =
        Arc::new(
            move |job: agent_diva_core::cron::CronJob,
                  cancel_token|
                  -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Option<String>> + Send>,
            > {
                let bus = bus_for_cron.clone();
                Box::pin(async move {
                    if cancel_token.is_cancelled() {
                        return Some("Error: cancelled".to_string());
                    }
                    let deliver = job.payload.deliver;
                    if !deliver {
                        return Some("skipped (deliver=false)".to_string());
                    }

                    let target_channel = job
                        .payload
                        .channel
                        .clone()
                        .unwrap_or_else(|| "cli".to_string());
                    let target_chat_id = job
                        .payload
                        .to
                        .clone()
                        .unwrap_or_else(|| "direct".to_string());
                    let (conversation_channel, conversation_chat_id) = if target_channel == "gui" {
                        let chat_id = if target_chat_id.starts_with("cron:") {
                            target_chat_id
                        } else {
                            format!("cron:{}", target_chat_id)
                        };
                        ("api".to_string(), chat_id)
                    } else {
                        (target_channel.clone(), target_chat_id)
                    };

                    let inbound = InboundMessage::new(
                        conversation_channel,
                        "cron",
                        conversation_chat_id,
                        job.payload.message,
                    )
                    .with_metadata("cron_job_id", job.id.clone())
                    .with_metadata("cron_trigger", "scheduled")
                    .with_metadata("cron_delivery_channel", target_channel);

                    if let Err(e) = bus.publish_inbound(inbound) {
                        error!("Failed to publish cron inbound job {}: {}", job.id, e);
                        return Some(format!(
                            "failed to publish cron inbound job {}: {}",
                            job.id, e
                        ));
                    }

                    Some("triggered agent turn".to_string())
                })
            },
        );
    let cron_service = Arc::new(CronService::new(cron_store, Some(cron_callback)));
    cron_service.start().await;

    let initial_provider = Arc::new(build_provider(&config, &config.agents.defaults.model)?);
    let dynamic_provider = Arc::new(DynamicProvider::new(initial_provider));
    let agent_provider: Arc<dyn LLMProvider> = dynamic_provider.clone();

    let model = config.agents.defaults.model.clone();
    let max_iterations = config.agents.defaults.max_tool_iterations as usize;
    let exec_timeout = config.tools.exec.timeout;
    let restrict_to_workspace = config.tools.restrict_to_workspace;
    let network = build_network_tool_config(&config);
    let (runtime_control_tx, runtime_control_rx) = mpsc::unbounded_channel();

    let tool_config = ToolConfig {
        network,
        exec_timeout,
        restrict_to_workspace,
        mcp_servers: config.tools.active_mcp_servers(),
        cron_service: Some(Arc::clone(&cron_service)),
        soul_context: SoulContextSettings {
            enabled: config.agents.soul.enabled,
            max_chars: config.agents.soul.max_chars,
            bootstrap_once: config.agents.soul.bootstrap_once,
        },
        notify_on_soul_change: config.agents.soul.notify_on_change,
        soul_governance: SoulGovernanceSettings {
            frequent_change_window_secs: config.agents.soul.frequent_change_window_secs,
            frequent_change_threshold: config.agents.soul.frequent_change_threshold,
            boundary_confirmation_hint: config.agents.soul.boundary_confirmation_hint,
        },
    };

    let agent = AgentLoop::with_tools(
        bus.clone(),
        agent_provider,
        runtime.workspace,
        Some(model),
        Some(max_iterations),
        tool_config,
        Some(runtime_control_rx),
    );

    let (provider_api_key, provider_api_base) = {
        let provider_name = current_provider_name(&config)
            .ok_or_else(|| anyhow::anyhow!("No provider found for model"))?;
        let catalog = ProviderCatalogService::new();
        let access = catalog
            .get_provider_access(&config, &provider_name)
            .unwrap_or_else(|| ProviderAccess::from_config(None));
        let resolved_api_base = access.api_base.clone().or_else(|| {
            catalog
                .get_provider_view(&config, &provider_name)
                .and_then(|view| view.api_base)
        });
        (access.api_key, resolved_api_base)
    };

    let mut channel_manager = ChannelManager::new(config.clone());
    let (inbound_tx, mut inbound_rx) = mpsc::channel::<InboundMessage>(1024);
    channel_manager.set_inbound_sender(inbound_tx);
    let bus_for_inbound_bridge = bus.clone();
    let inbound_bridge_handle = tokio::spawn(async move {
        while let Some(msg) = inbound_rx.recv().await {
            if let Err(e) = bus_for_inbound_bridge.publish_inbound(msg) {
                error!("Failed to publish inbound message to bus: {}", e);
            }
        }
    });

    if let Err(e) = channel_manager.initialize().await {
        error!("Failed to initialize channels: {}", e);
    }

    let configured_channels = [
        ("telegram", config.channels.telegram.enabled),
        ("discord", config.channels.discord.enabled),
        ("whatsapp", config.channels.whatsapp.enabled),
        ("feishu", config.channels.feishu.enabled),
        ("dingtalk", config.channels.dingtalk.enabled),
        ("email", config.channels.email.enabled),
        ("slack", config.channels.slack.enabled),
        ("qq", config.channels.qq.enabled),
        ("matrix", config.channels.matrix.enabled),
    ];

    let channel_manager = Arc::new(channel_manager);
    let (api_tx, api_rx) = mpsc::channel(100);
    let manager = Manager::new(
        api_rx,
        bus.clone(),
        dynamic_provider,
        runtime.loader,
        config.agents.defaults.provider.clone(),
        config.agents.defaults.model.clone(),
        provider_api_key,
        provider_api_base,
        Some(channel_manager.clone()),
        Some(runtime_control_tx),
        Arc::clone(&cron_service),
    );
    let _api_tx_keepalive = api_tx.clone();

    for (channel_name, enabled) in configured_channels {
        if !enabled {
            continue;
        }
        let manager = channel_manager.clone();
        let channel_name = channel_name.to_string();
        let channel_key = channel_name.clone();
        bus.subscribe_outbound(channel_name, move |msg| {
            let manager = manager.clone();
            let channel_key = channel_key.clone();
            async move {
                if let Err(e) = manager.send(&channel_key, msg).await {
                    error!("Failed to send outbound message to {}: {}", channel_key, e);
                }
            }
        })
        .await;
    }

    let bus_for_outbound_dispatch = bus.clone();
    let outbound_dispatch_handle = tokio::spawn(async move {
        bus_for_outbound_dispatch.dispatch_outbound_loop().await;
    });

    let channel_manager_for_task = channel_manager.clone();
    let _channel_handle = tokio::spawn(async move {
        if let Err(e) = channel_manager_for_task.start_all().await {
            error!("Channel manager error: {}", e);
        }
    });

    let agent = Some(agent);
    let agent_handle = tokio::spawn(async move {
        if let Some(mut agent) = agent {
            if let Err(e) = agent.run().await {
                error!("Agent loop error: {}", e);
            }
        }
    });

    let mut manager_handle = tokio::spawn(async move {
        if let Err(e) = manager.run().await {
            if e.to_string().contains("RESTART_REQUIRED") {
                return Err(e);
            }
            error!("Manager loop error: {}", e);
            Ok(())
        } else {
            Ok(())
        }
    });

    let state = AppState {
        api_tx,
        bus: bus.clone(),
    };
    let (server_shutdown_tx, server_shutdown_rx) = broadcast::channel(1);
    let port = runtime.port;
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_server(state, port, server_shutdown_rx).await {
            error!("API Server error: {}", e);
        }
    });

    let mut manager_handle_completed = false;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        res = &mut manager_handle => {
            manager_handle_completed = true;
            match res {
                Ok(Err(e)) => error!("Manager loop error: {}", e),
                Err(e) => error!("Manager loop panicked or cancelled: {}", e),
                _ => {}
            }
        }
    }

    bus.stop().await;

    let _ = server_shutdown_tx.send(());
    let _ = server_handle.await;

    if !manager_handle_completed {
        manager_handle.abort();
        let _ = manager_handle.await;
    }

    inbound_bridge_handle.abort();
    let _ = inbound_bridge_handle.await;

    outbound_dispatch_handle.abort();
    let _ = outbound_dispatch_handle.await;

    agent_handle.abort();
    let _ = agent_handle.await;

    if let Err(e) = channel_manager.stop_all().await {
        error!("Failed to stop channels: {}", e);
    }
    cron_service.stop().await;

    Ok(())
}
