//! HTTP API server implementation

use axum::{
    extract::{State, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

use crate::core::client::AsyncTranslator;
use crate::core::models::TranslationRequest;

/// Application state
#[derive(Clone)]
pub struct AppState {
    translator: Arc<AsyncTranslator>,
}

/// Health check response
#[derive(Serialize)]
struct HealthResponse {
    status: String,
    service: String,
    version: String,
}

/// Models list response
#[derive(Serialize)]
struct ModelsResponse {
    object: String,
    data: Vec<ModelInfo>,
}

#[derive(Serialize)]
struct ModelInfo {
    id: String,
    object: String,
    created: i64,
    owned_by: String,
    lane: String,
    rpm: u32,
    max_concurrent: usize,
}

/// OpenAI compatible request
#[derive(Deserialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    pub target_language: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct OpenAIMessage {
    pub role: String,
    pub content: String,
}

/// OpenAI compatible response
#[derive(Serialize)]
pub struct OpenAIResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAIChoice>,
    pub usage: Usage,
}

#[derive(Serialize)]
pub struct OpenAIChoice {
    pub index: i32,
    pub message: OpenAIMessage,
    pub finish_reason: String,
}

#[derive(Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Custom translation request
#[derive(Deserialize)]
pub struct TranslateRequest {
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub text_list: Vec<String>,
}

/// Custom translation response
#[derive(Serialize)]
pub struct TranslateResponse {
    pub translations: Vec<TranslationItem>,
}

#[derive(Serialize)]
pub struct TranslationItem {
    pub detected_source_lang: Option<String>,
    pub text: String,
}

/// Error response
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Serialize)]
pub struct ErrorDetail {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

/// Health check handler
async fn health_check() -> axum::Json<HealthResponse> {
    axum::Json(HealthResponse {
        status: "ok".to_string(),
        service: "doubao-translator".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Get models handler
async fn get_models(State(state): State<Arc<AppState>>) -> axum::Json<ModelsResponse> {
    let models = state.translator.get_available_models();
    let model_infos: Vec<ModelInfo> = models
        .iter()
        .map(|m| ModelInfo {
            id: m.id.clone(),
            object: "model".to_string(),
            created: 1705324800, // Unix timestamp
            owned_by: match m.lane {
                crate::core::models::LaneType::Slow => "ByteDance",
                crate::core::models::LaneType::Fast => "ByteDance/DeepSeek",
            }
            .to_string(),
            lane: m.lane.to_string(),
            rpm: m.rpm,
            max_concurrent: m.max_concurrent,
        })
        .collect();

    axum::Json(ModelsResponse {
        object: "list".to_string(),
        data: model_infos,
    })
}

/// OpenAI compatible translation handler
async fn openai_compatible(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<OpenAIRequest>,
) -> Result<axum::Json<OpenAIResponse>, axum::Json<ErrorResponse>> {
    // Extract text from messages
    let text = payload
        .messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content.clone())
        .collect::<Vec<String>>()
        .join(" ");

    if text.is_empty() {
        return Err(axum::Json(ErrorResponse {
            error: ErrorDetail {
                message: "No text to translate".to_string(),
                code: Some("invalid_request".to_string()),
                r#type: Some("invalid_request_error".to_string()),
            },
        }));
    }

    // Create translation request
    let target_lang = payload.target_language.unwrap_or_else(|| "zh".to_string());
    let request = TranslationRequest::new(text, target_lang);

    // Translate
    match state.translator.translate(&request).await {
        Ok(result) => {
            let response = OpenAIResponse {
                id: format!("chatcmpl-{}", chrono::Utc::now().timestamp()),
                object: "chat.completion".to_string(),
                created: chrono::Utc::now().timestamp(),
                model: result.model_used,
                choices: vec![OpenAIChoice {
                    index: 0,
                    message: OpenAIMessage {
                        role: "assistant".to_string(),
                        content: result.translation,
                    },
                    finish_reason: "stop".to_string(),
                }],
                usage: Usage {
                    prompt_tokens: result.tokens_used / 2, // Rough estimate
                    completion_tokens: result.tokens_used / 2,
                    total_tokens: result.tokens_used,
                },
            };

            Ok(axum::Json(response))
        }
        Err(e) => {
            warn!("Translation failed: {}", e);
            Err(axum::Json(ErrorResponse {
                error: ErrorDetail {
                    message: e.to_string(),
                    code: Some("translation_error".to_string()),
                    r#type: Some("api_error".to_string()),
                },
            }))
        }
    }
}

/// Custom translation handler
async fn translate(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<TranslateRequest>,
) -> Result<axum::Json<TranslateResponse>, axum::Json<ErrorResponse>> {
    if payload.text_list.is_empty() {
        return Err(axum::Json(ErrorResponse {
            error: ErrorDetail {
                message: "text_list cannot be empty".to_string(),
                code: Some("invalid_request".to_string()),
                r#type: Some("invalid_request_error".to_string()),
            },
        }));
    }

    // Convert language codes
    let target_lang = match payload.target_lang.as_str() {
        "zh-CN" => "zh",
        "zh-TW" => "zh-Hant",
        "auto" => "",
        "no" => "nb",
        _ => &payload.target_lang,
    };

    let source_lang = match payload.source_lang.as_deref() {
        Some("zh-CN") => Some("zh".to_string()),
        Some("zh-TW") => Some("zh-Hant".to_string()),
        Some("auto") => None,
        Some("no") => Some("nb".to_string()),
        Some(lang) => Some(lang.to_string()),
        None => None,
    };

    // Translate each text
    let mut translations = Vec::new();
    for text in payload.text_list {
        let mut request = TranslationRequest::new(text.clone(), target_lang.to_string());
        if let Some(ref lang) = source_lang {
            request = request.with_source_lang(lang);
        }

        match state.translator.translate(&request).await {
            Ok(result) => {
                translations.push(TranslationItem {
                    detected_source_lang: result.detected_source_lang,
                    text: result.translation,
                });
            }
            Err(e) => {
                warn!("Translation failed for '{}': {}", text, e);
                // Return original text on error
                translations.push(TranslationItem {
                    detected_source_lang: None,
                    text,
                });
            }
        }
    }

    Ok(axum::Json(TranslateResponse { translations }))
}

/// Run the HTTP server
pub async fn run_server(host: String, port: u16) -> anyhow::Result<()> {
    // Create translator
    let translator = Arc::new(AsyncTranslator::from_env()?);

    // Create app state
    let state = Arc::new(AppState { translator });

    // Create router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/v1/models", get(get_models))
        .route("/v1/chat/completions", post(openai_compatible))
        .route("/translate", post(translate))
        .with_state(state);

    // Bind address
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;

    info!("Starting server on {}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}