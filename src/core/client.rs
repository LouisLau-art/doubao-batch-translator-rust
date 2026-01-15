//! Async translation client with retry and fallback logic

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::core::errors::{Result, TranslationError};
use crate::core::models::{LaneType, Model, TranslationRequest, TranslationResult};
use crate::core::config::TranslatorConfig;
use crate::core::token_tracker::TokenTracker;

/// Async translation client with smart routing and retry logic
#[derive(Debug, Clone)]
pub struct AsyncTranslator {
    client: reqwest::Client,
    config: Arc<TranslatorConfig>,
    semaphore: Arc<Semaphore>,
    token_tracker: Arc<TokenTracker>,
    current_model: Arc<Mutex<String>>,
}

impl AsyncTranslator {
    /// Create a new async translator
    pub fn new(config: TranslatorConfig) -> Result<Self> {
        config.validate()?;

        let timeout = Duration::from_millis(config.timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            .pool_max_idle_per_host(10)
            .build()?;

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent));
        let token_tracker = Arc::new(TokenTracker::from_env());
        let current_model = Arc::new(Mutex::new(
            config.models
                .first()
                .map(|m| m.id.clone())
                .unwrap_or_default(),
        ));

        Ok(Self {
            client,
            config: Arc::new(config),
            semaphore,
            token_tracker,
            current_model,
        })
    }

    /// Create from environment
    pub fn from_env() -> Result<Self> {
        let config = TranslatorConfig::load()?;
        Self::new(config)
    }

    /// Translate a single request
    pub async fn translate(&self, request: &TranslationRequest) -> Result<TranslationResult> {
        // Check token quota
        let estimated_tokens = request.text.len() / 4; // Rough estimate
        if !self.token_tracker.can_use(estimated_tokens).await {
            return Err(TranslationError::QuotaExceededError);
        }

        // Acquire semaphore for concurrency control
        let _permit = self.semaphore.acquire().await.unwrap();

        // Try slow lane first (free tier)
        let result = self.translate_with_lane(request, LaneType::Slow).await;

        match result {
            Ok(trans_result) => {
                // Track token usage
                if let Err(e) = self
                    .token_tracker
                    .use_tokens(trans_result.tokens_used)
                    .await
                {
                    warn!("Failed to track token usage: {}", e);
                }

                // Update current model
                {
                    let mut current = self.current_model.lock().await;
                    *current = trans_result.model_used.clone();
                }

                Ok(trans_result)
            }
            Err(e) => {
                warn!("Slow lane failed: {}, trying fast lane", e);

                // Fallback to fast lane
                let fast_result = self.translate_with_lane(request, LaneType::Fast).await;

                match fast_result {
                    Ok(trans_result) => {
                        // Track token usage
                        if let Err(e) = self
                            .token_tracker
                            .use_tokens(trans_result.tokens_used)
                            .await
                        {
                            warn!("Failed to track token usage: {}", e);
                        }

                        // Update current model
                        {
                            let mut current = self.current_model.lock().await;
                            *current = trans_result.model_used.clone();
                        }

                        Ok(trans_result)
                    }
                    Err(fast_err) => {
                        // Both lanes failed
                        warn!("Fast lane also failed: {}", fast_err);
                        Err(e)
                    }
                }
            }
        }
    }

    /// Translate with specific lane
    async fn translate_with_lane(
        &self,
        request: &TranslationRequest,
        lane: LaneType,
    ) -> Result<TranslationResult> {
        let models = self.config.get_models_by_lane(lane);

        if models.is_empty() {
            return Err(TranslationError::ConfigError {
                message: format!("No models available for lane: {}", lane),
            });
        }

        // Try each model in the lane
        for model in models {
            match self.translate_with_model(request, model).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!("Model {} failed: {}", model.id, e);
                    continue;
                }
            }
        }

        Err(TranslationError::ConfigError {
            message: format!("All models in lane {} failed", lane),
        })
    }

    /// Translate with specific model
    async fn translate_with_model(
        &self,
        request: &TranslationRequest,
        model: &Model,
    ) -> Result<TranslationResult> {
        let mut last_error = None;

        // Retry logic
        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                debug!("Retry attempt {} for model {}", attempt, model.id);
                sleep(Duration::from_millis(self.config.retry_delay_ms * 2_u64.pow(attempt - 1))).await;
            }

            match self.send_request(request, model).await {
                Ok(result) => {
                    if attempt > 0 {
                        info!("Successfully translated after {} retries", attempt);
                    }
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e);

                    // Don't retry on certain errors
                    match &last_error {
                        Some(TranslationError::QuotaExceededError) => break,
                        Some(TranslationError::InvalidResponseError { .. }) => continue,
                        _ => {}
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    /// Send actual HTTP request
    async fn send_request(
        &self,
        request: &TranslationRequest,
        model: &Model,
    ) -> Result<TranslationResult> {
        let mut body = serde_json::json!({
            "model": model.id,
            "input": [{
                "role": "user",
                "content": [{
                    "type": "input_text",
                    "text": request.text,
                    "translation_options": {
                        "target_language": request.target_lang
                    }
                }]
            }]
        });

        // Add source language if specified
        if let Some(source_lang) = &request.source_lang {
            if let Some(content) = body["input"][0]["content"][0].as_object_mut() {
                if let Some(opts) = content.get_mut("translation_options") {
                    if let Some(obj) = opts.as_object_mut() {
                        obj.insert("source_language".to_string(), serde_json::json!(source_lang));
                    }
                }
            }
        }

        let response = self
            .client
            .post(&self.config.api_endpoint)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| TranslationError::NetworkError {
                message: e.to_string(),
            })?;

        let status = response.status();

        if status.is_success() {
            let json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| TranslationError::InvalidResponseError {
                    message: e.to_string(),
                })?;

            // Parse response
            let translation = json["output"]["choices"]
                .get(0)
                .and_then(|c| c["message"]["content"].as_str())
                .ok_or_else(|| TranslationError::InvalidResponseError {
                    message: "No translation in response".to_string(),
                })?
                .to_string();

            let tokens_used = json["usage"]["total_tokens"]
                .as_u64()
                .unwrap_or(0) as usize;

            let detected_source_lang = json["output"]["choices"]
                .get(0)
                .and_then(|c| c["message"]["detected_source_language"].as_str())
                .map(|s| s.to_string());

            let request_id = json["id"].as_str().map(|s| s.to_string());

            Ok(TranslationResult {
                translation,
                detected_source_lang,
                tokens_used,
                model_used: model.id.clone(),
                request_id,
            })
        } else {
            // Clone status before consuming response
            let status_code = status.as_u16();
            let error_text = response.text().await.unwrap_or_default();

            // Handle rate limiting
            if status_code == 429 {
                // Need to re-parse headers since response was consumed
                // For simplicity, just return generic rate limit error
                return Err(TranslationError::RateLimitError { retry_after: None });
            }

            // Handle quota exceeded
            if error_text.contains("quota") || error_text.contains("limit") {
                return Err(TranslationError::QuotaExceededError);
            }

            Err(TranslationError::ApiError {
                status: status_code,
                message: error_text,
            })
        }
    }

    /// Batch translate multiple requests
    pub async fn translate_batch(
        &self,
        requests: Vec<TranslationRequest>,
    ) -> Vec<Result<TranslationResult>> {
        let mut results = Vec::new();

        for request in requests {
            let result = self.translate(&request).await;
            results.push(result);
        }

        results
    }

    /// Get current token usage
    pub async fn get_token_usage(&self) -> crate::core::models::TokenUsage {
        self.token_tracker.get_stats().await
    }

    /// Get current model
    pub async fn get_current_model(&self) -> String {
        self.current_model.lock().await.clone()
    }

    /// Get available models
    pub fn get_available_models(&self) -> Vec<&Model> {
        self.config.get_enabled_models()
    }

    /// Get model by ID
    pub fn get_model(&self, id: &str) -> Option<&Model> {
        self.config.find_model(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_translator_creation() {
        let config = TranslatorConfig::load().unwrap();
        let translator = AsyncTranslator::new(config);
        assert!(translator.is_ok());
    }

    #[tokio::test]
    async fn test_translator_from_env() {
        // This test requires ARK_API_KEY env var
        std::env::set_var("ARK_API_KEY", "test_key");
        let translator = AsyncTranslator::from_env();
        assert!(translator.is_ok());
    }
}