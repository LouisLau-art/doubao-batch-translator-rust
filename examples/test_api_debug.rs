//! 调试 API 连接问题

use doubao_translator::core::client::AsyncTranslator;
use doubao_translator::core::models::TranslationRequest;
use dotenvy::dotenv;
use tracing::{info, warn, debug};

#[tokio::main]
async fn main() {
    // 加载环境变量
    dotenv().ok();

    // 初始化日志
    tracing_subscriber::fmt::init();

    info!("=== API 连接调试测试 ===");

    // 检查环境变量
    match std::env::var("ARK_API_KEY") {
        Ok(key) => info!("✅ ARK_API_KEY 已设置: {}...", &key[..10]),
        Err(_) => {
            warn!("❌ ARK_API_KEY 环境变量未设置");
            return;
        }
    }

    let api_endpoint = std::env::var("API_ENDPOINT").unwrap_or_else(|_| "https://ark.cn-beijing.volces.com/api/v3/responses".to_string());
    info!("✅ API_ENDPOINT: {}", api_endpoint);

    // 直接创建翻译器
    let translator = match AsyncTranslator::from_env() {
        Ok(t) => {
            info!("✅ 翻译器创建成功");
            t
        }
        Err(e) => {
            warn!("❌ 翻译器创建失败: {}", e);
            return;
        }
    };

    // 获取可用模型
    let models = translator.get_available_models();
    info!("可用模型: {}", models.len());
    for model in &models {
        info!("  - {} ({} lane, RPM: {})", model.id, model.lane, model.rpm);
    }

    // 准备测试请求
    let test_text = "Hello, world! This is a simple test.";
    let request = TranslationRequest::new(test_text.to_string(), "zh".to_string()).with_source_lang("en");

    debug!("准备翻译文本: {}", test_text);

    // 执行翻译
    info!("开始翻译测试...");
    match translator.translate(&request).await {
        Ok(result) => {
            info!("✅ 翻译成功!");
            info!("   原文: {}", test_text);
            info!("   译文: {}", result.translation);
            info!("   模型: {}", result.model_used);
            info!("   Token 使用: {}", result.tokens_used);
            if let Some(lang) = result.detected_source_lang {
                info!("   检测源语言: {}", lang);
            }
        }
        Err(e) => {
            warn!("❌ 翻译失败: {}", e);
            warn!("   错误类型: {:?}", e);
        }
    }

    // 获取 token 使用情况
    let token_usage = translator.get_token_usage().await;
    info!("Token 使用情况: {}/{}/{} (已用/每日限额/剩余)",
        token_usage.used_today,
        token_usage.daily_limit,
        token_usage.remaining()
    );

    info!("=== 测试完成 ===");
}
