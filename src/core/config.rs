//! Configuration management

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

use crate::core::models::{LaneType, Model};

/// Configuration for translator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatorConfig {
    pub api_key: String,
    pub api_endpoint: String,
    pub models: Vec<Model>,
    pub max_concurrent: usize,
    pub max_rps: f64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub max_input_tokens: usize,
    pub timeout_ms: u64,
}

impl Default for TranslatorConfig {
    fn default() -> Self {
        Self {
            api_key: std::env::var("ARK_API_KEY").unwrap_or_default(),
            api_endpoint: std::env::var("API_ENDPOINT")
                .unwrap_or_else(|_| "https://ark.cn-beijing.volces.com/api/v3/responses".to_string()),
            models: vec![],
            max_concurrent: 20,
            max_rps: 10.0,
            max_retries: 3,
            retry_delay_ms: 1000,
            max_input_tokens: 900,
            timeout_ms: 30000,
        }
    }
}

/// Default models configuration
const DEFAULT_MODELS: &[(&str, LaneType, u32, usize)] = &[
    // Slow lane (free tier)
    ("doubao-seed-translation-250915", LaneType::Slow, 5000, 80),
    // Fast lane (high performance)
    ("deepseek-v3-250324", LaneType::Fast, 30000, 500),
    ("doubao-seed-1-6-251015", LaneType::Fast, 30000, 500),
    ("doubao-1-5-vision-pro-32k-250115", LaneType::Fast, 30000, 500),
    // ModelScope models
    ("deepseek-ai/DeepSeek-V3.2", LaneType::Fast, 30000, 500),
];

impl TranslatorConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("ARK_API_KEY")
            .map_err(|_| anyhow::anyhow!("ARK_API_KEY environment variable is required"))?;

        let api_endpoint = std::env::var("API_ENDPOINT")
            .unwrap_or_else(|_| "https://ark.cn-beijing.volces.com/api/v3/responses".to_string());

        let max_concurrent = std::env::var("MAX_CONCURRENT")
            .unwrap_or_else(|_| "20".to_string())
            .parse::<usize>()?;

        let max_rps = std::env::var("MAX_RPS")
            .unwrap_or_else(|_| "10.0".to_string())
            .parse::<f64>()?;

        let max_retries = std::env::var("MAX_RETRIES")
            .unwrap_or_else(|_| "3".to_string())
            .parse::<u32>()?;

        let retry_delay_ms = std::env::var("RETRY_DELAY_MS")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()?;

        let max_input_tokens = std::env::var("MAX_INPUT_TOKENS")
            .unwrap_or_else(|_| "900".to_string())
            .parse::<usize>()?;

        let timeout_ms = std::env::var("REQUEST_TIMEOUT_MS")
            .unwrap_or_else(|_| "30000".to_string())
            .parse::<u64>()?;

        Ok(Self {
            api_key,
            api_endpoint,
            models: vec![],
            max_concurrent,
            max_rps,
            max_retries,
            retry_delay_ms,
            max_input_tokens,
            timeout_ms,
        })
    }

    /// Load configuration with default models
    pub fn load() -> anyhow::Result<Self> {
        let mut config = Self::from_env()?;

        // Load default models if none specified
        if config.models.is_empty() {
            config.models = DEFAULT_MODELS
                .iter()
                .map(|(id, lane, rpm, max_concurrent)| Model {
                    id: id.to_string(),
                    lane: *lane,
                    rpm: *rpm,
                    max_concurrent: *max_concurrent,
                    enabled: true,
                })
                .collect();

            info!("Loaded {} default models", config.models.len());
        }

        Ok(config)
    }

    /// Load from JSON file
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn to_file<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Validate configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.api_key.is_empty() {
            return Err(anyhow::anyhow!("API key is required"));
        }

        if self.api_endpoint.is_empty() {
            return Err(anyhow::anyhow!("API endpoint is required"));
        }

        if self.models.is_empty() {
            warn!("No models configured");
        }

        if self.max_concurrent == 0 {
            return Err(anyhow::anyhow!("max_concurrent must be greater than 0"));
        }

        if self.max_rps <= 0.0 {
            return Err(anyhow::anyhow!("max_rps must be greater than 0"));
        }

        Ok(())
    }

    /// Get models by lane type
    pub fn get_models_by_lane(&self, lane: LaneType) -> Vec<&Model> {
        self.models.iter().filter(|m| m.lane == lane && m.enabled).collect()
    }

    /// Get all enabled models
    pub fn get_enabled_models(&self) -> Vec<&Model> {
        self.models.iter().filter(|m| m.enabled).collect()
    }

    /// Find model by ID
    pub fn find_model(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Get model for lane
    pub fn get_model_for_lane(&self, lane: LaneType) -> Option<&Model> {
        self.models
            .iter()
            .find(|m| m.lane == lane && m.enabled)
    }

    /// Get all available model IDs
    pub fn get_model_ids(&self) -> Vec<String> {
        self.models.iter().map(|m| m.id.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_validation() {
        let mut config = TranslatorConfig::default();
        config.api_key = "test_key".to_string();
        config.api_endpoint = "https://test.com".to_string();
        config.models = vec![Model {
            id: "test".to_string(),
            lane: LaneType::Slow,
            rpm: 5000,
            max_concurrent: 80,
            enabled: true,
        }];

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_missing_key() {
        let config = TranslatorConfig {
            api_key: "".to_string(),
            api_endpoint: "https://test.com".to_string(),
            models: vec![],
            ..Default::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_get_models_by_lane() {
        let config = TranslatorConfig::load().unwrap();
        let slow_models = config.get_models_by_lane(LaneType::Slow);
        let fast_models = config.get_models_by_lane(LaneType::Fast);

        assert!(!slow_models.is_empty());
        assert!(!fast_models.is_empty());
    }
}