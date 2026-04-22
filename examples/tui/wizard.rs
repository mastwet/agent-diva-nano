//! Configuration wizard handling.

use crate::app::{AppMode, TimelineKind, TuiApp, WizardStep};
use crate::config::TuiConfigFile;
use crate::manager::AgentManager;
use crate::provider::resolve_provider_name;

pub async fn handle_wizard_step(
    step: WizardStep,
    content: String,
    app: &mut TuiApp,
    manager: &mut AgentManager,
) {
    match step {
        WizardStep::Model => {
            if content.is_empty() {
                app.add_line(
                    TimelineKind::Error,
                    "Model cannot be empty. Please enter a model identifier.",
                );
                return;
            }
            app.wizard_model = content;
            app.mode = AppMode::ConfigWizard {
                step: WizardStep::ApiKey,
            };
            app.add_line(
                TimelineKind::System,
                "Step 2/3: Enter the API key for your provider.",
            );
        }
        WizardStep::ApiKey => {
            if content.is_empty() {
                app.add_line(
                    TimelineKind::Error,
                    "API key cannot be empty. Please enter an API key.",
                );
                return;
            }
            app.wizard_api_key = content;
            app.mode = AppMode::ConfigWizard {
                step: WizardStep::ApiBase,
            };
            app.add_line(
                TimelineKind::System,
                "Step 3/3: Enter a custom API base URL (optional). Press Enter to skip if your provider uses the default endpoint.",
            );
        }
        WizardStep::ApiBase => {
            app.wizard_api_base = content;
            let api_base_display = if app.wizard_api_base.is_empty() {
                "(default)".to_string()
            } else {
                app.wizard_api_base.clone()
            };
            app.mode = AppMode::ConfigWizard {
                step: WizardStep::Confirm,
            };
            app.add_line(
                TimelineKind::System,
                "--- Configuration Summary ---".to_string(),
            );
            app.add_line(
                TimelineKind::System,
                format!("Model: {}", app.wizard_model),
            );
            app.add_line(
                TimelineKind::System,
                format!("Provider: {}", resolve_provider_name(&app.wizard_model)),
            );
            app.add_line(
                TimelineKind::System,
                format!("API Base: {}", api_base_display),
            );
            app.add_line(
                TimelineKind::System,
                "Press Enter to save and start, or type /cancel to reconfigure.",
            );
        }
        WizardStep::Confirm => {
            if content == "/cancel" {
                app.enter_config_wizard(None);
                return;
            }
            // Save config
            let file_config = TuiConfigFile {
                model: app.wizard_model.clone(),
                api_key: app.wizard_api_key.clone(),
                api_base: if app.wizard_api_base.is_empty() {
                    None
                } else {
                    Some(app.wizard_api_base.clone())
                },
            };
            if let Err(e) = file_config.save() {
                app.add_line(
                    TimelineKind::Error,
                    format!("Failed to save config: {}", e),
                );
                return;
            }
            // Update manager and start agent
            let new_config = file_config.to_nano_config();
            manager.update_config(new_config.clone());
            if let Err(e) = manager.restart().await {
                app.add_line(
                    TimelineKind::Error,
                    format!("Failed to start agent: {}", e),
                );
                return;
            }
            app.model = app.wizard_model.clone();
            app.provider_name = resolve_provider_name(&app.model);
            app.mode = AppMode::Normal;
            app.add_line(
                TimelineKind::System,
                "Configuration saved. Agent started. Ready to chat!".to_string(),
            );
        }
    }
}