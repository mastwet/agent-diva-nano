//! Agent lifecycle manager.

use agent_diva_nano::{Agent, NanoConfig, NanoError};

pub struct AgentManager {
    pub config: NanoConfig,
    agent: Option<Agent>,
}

impl AgentManager {
    pub fn new(config: NanoConfig) -> Self {
        Self { config, agent: None }
    }

    pub async fn start(&mut self) -> Result<(), NanoError> {
        let mut agent = Agent::new(self.config.clone()).build()?;
        agent.start().await?;
        self.agent = Some(agent);
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(mut agent) = self.agent.take() {
            agent.stop().await;
        }
    }

    pub async fn restart(&mut self) -> Result<(), NanoError> {
        self.stop().await;
        self.start().await
    }

    pub async fn switch_model(&mut self, model: String) -> Result<(), NanoError> {
        self.stop().await;
        self.config.model = model;
        self.start().await
    }

    pub fn update_config(&mut self, config: NanoConfig) {
        self.config = config;
    }

    pub fn agent(&self) -> Option<&Agent> {
        self.agent.as_ref()
    }
}