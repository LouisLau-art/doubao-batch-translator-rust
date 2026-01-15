//! Token usage tracking and quota management

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::core::models::TokenUsage;

/// Token tracker for managing daily quota
#[derive(Debug, Clone)]
pub struct TokenTracker {
    usage: Arc<RwLock<TokenUsage>>,
}

impl TokenTracker {
    /// Create a new token tracker
    pub fn new(daily_limit: usize) -> Self {
        Self {
            usage: Arc::new(RwLock::new(TokenUsage::new(daily_limit))),
        }
    }

    /// Create from environment variable
    pub fn from_env() -> Self {
        let daily_limit = std::env::var("DAILY_TOKEN_LIMIT")
            .unwrap_or_else(|_| "2000000".to_string()) // 2M default
            .parse::<usize>()
            .unwrap_or(2000000);

        Self::new(daily_limit)
    }

    /// Check if enough tokens are available
    pub async fn can_use(&self, tokens: usize) -> bool {
        let mut usage = self.usage.write().await;
        usage.reset_if_needed();
        usage.can_use(tokens)
    }

    /// Use tokens from quota
    pub async fn use_tokens(&self, tokens: usize) -> anyhow::Result<()> {
        // First check if we can use the tokens
        {
            let mut usage = self.usage.write().await;
            usage.reset_if_needed();
            usage.use_tokens(tokens)?;
        }

        // Log success
        {
            let usage = self.usage.read().await;
            debug!("Used {} tokens, remaining: {}", tokens, usage.remaining());
        }

        Ok(())
    }

    /// Get current usage statistics
    pub async fn get_stats(&self) -> TokenUsage {
        let usage = self.usage.read().await;
        usage.clone()
    }

    /// Get remaining tokens
    pub async fn remaining(&self) -> usize {
        let usage = self.usage.read().await;
        usage.remaining()
    }

    /// Check if quota is low (less than 10% remaining)
    pub async fn is_low(&self) -> bool {
        let usage = self.usage.read().await;
        let remaining = usage.remaining();
        let daily_limit = usage.daily_limit;
        drop(usage);
        remaining < daily_limit / 10
    }

    /// Reset quota (for testing or manual reset)
    pub async fn reset(&self) {
        let mut usage = self.usage.write().await;
        usage.used_today = 0;
        usage.last_reset = chrono::Utc::now();
        info!("Token quota reset");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_tracker() {
        let tracker = TokenTracker::new(1000);

        // Should be able to use tokens
        assert!(tracker.can_use(500).await);
        tracker.use_tokens(500).await.unwrap();

        // Should still have tokens remaining
        assert_eq!(tracker.remaining().await, 500);

        // Should not be able to use more than available
        assert!(!tracker.can_use(600).await);

        // Should be able to use remaining tokens
        assert!(tracker.can_use(500).await);
        tracker.use_tokens(500).await.unwrap();

        // Should have no tokens left
        assert_eq!(tracker.remaining().await, 0);
    }

    #[tokio::test]
    async fn test_low_quota_detection() {
        let tracker = TokenTracker::new(1000);
        tracker.use_tokens(950).await.unwrap();
        assert!(tracker.is_low().await);

        tracker.reset().await;
        assert!(!tracker.is_low().await);
    }
}