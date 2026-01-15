//! Core data models for translation

use serde::{Deserialize, Serialize};
use std::fmt;

/// Lane type for model selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneType {
    /// Slow lane: doubao-seed-translation-250915 (RPM=5000, 80 concurrent)
    Slow,
    /// Fast lane: deepseek, doubao-pro, etc. (RPM=30000, 500 concurrent)
    Fast,
}

impl fmt::Display for LaneType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaneType::Slow => write!(f, "slow"),
            LaneType::Fast => write!(f, "fast"),
        }
    }
}

/// Translation model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub lane: LaneType,
    pub rpm: u32,
    pub max_concurrent: usize,
    pub enabled: bool,
}

impl Model {
    /// Check if model is suitable for given lane
    pub fn is_compatible(&self, lane: LaneType) -> bool {
        self.lane == lane && self.enabled
    }
}

/// Translation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub context: Option<String>,
}

impl TranslationRequest {
    pub fn new(text: String, target_lang: String) -> Self {
        Self {
            text,
            source_lang: None,
            target_lang,
            context: None,
        }
    }

    pub fn with_source_lang(mut self, source_lang: impl Into<String>) -> Self {
        self.source_lang = Some(source_lang.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

/// Translation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResult {
    pub translation: String,
    pub detected_source_lang: Option<String>,
    pub tokens_used: usize,
    pub model_used: String,
    pub request_id: Option<String>,
}

/// Token usage tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub daily_limit: usize,
    pub used_today: usize,
    pub last_reset: chrono::DateTime<chrono::Utc>,
}

impl TokenUsage {
    pub fn new(daily_limit: usize) -> Self {
        Self {
            daily_limit,
            used_today: 0,
            last_reset: chrono::Utc::now(),
        }
    }

    pub fn remaining(&self) -> usize {
        self.daily_limit.saturating_sub(self.used_today)
    }

    pub fn can_use(&self, tokens: usize) -> bool {
        self.remaining() >= tokens
    }

    pub fn use_tokens(&mut self, tokens: usize) -> anyhow::Result<()> {
        if !self.can_use(tokens) {
            return Err(anyhow::anyhow!("Token quota exceeded"));
        }
        self.used_today += tokens;
        Ok(())
    }

    pub fn reset_if_needed(&mut self) {
        let now = chrono::Utc::now();
        if now.date_naive() != self.last_reset.date_naive() {
            self.used_today = 0;
            self.last_reset = now;
        }
    }
}