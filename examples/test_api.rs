//! 测试 API 连接

use doubao_translator::core::{client::AsyncTranslator, config::TranslatorConfig};
use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    // 加载环境变量
    dotenv().ok();

    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== API 连接测试 ===");

    // 检查环境变量
    match std::env::var("ARK_API_KEY") {
        Ok(key) => println!("✅ ARK_API_KEY 已设置: {}...", &key[..10]),
        Err(_) => {
            println!("❌ ARK_API_KEY 环境变量未设置");
            return;
        }
    }

    match std::env::var("API_ENDPOINT") {
        Ok(endpoint) => println!("✅ API_ENDPOINT: {}", endpoint),
        Err(_) => println!("⚠️  API_ENDPOINT 未设置，使用默认值"),
    }

    // 加载配置
    println!("\n--- 加载配置 ---");
    let config = match TranslatorConfig::load() {
        Ok(cfg) => {
            println!("✅ 配置加载成功");
            println!("   API Endpoint: {}", cfg.api_endpoint);
            println!("   可用模型数: {}", cfg.models.len());
            cfg
        }
        Err(e) => {
            println!("❌ 配置加载失败: {}", e);
            return;
        }
    };

    // 创建翻译器
    println!("\n--- 创建翻译器 ---");
    let translator = match AsyncTranslator::new(config) {
        Ok(t) => {
            println!("✅ 翻译器创建成功");
            t
        }
        Err(e) => {
            println!("❌ 翻译器创建失败: {}", e);
            return;
        }
    };

    // 显示可用模型
    println!("\n--- 可用模型 ---");
    let models = translator.get_available_models();
    for model in &models {
        println!("   {} ({} lane, RPM: {})", model.id, model.lane, model.rpm);
    }

    // 测试简单翻译
    println!("\n--- 测试简单翻译 ---");
    let test_request = doubao_translator::core::models::TranslationRequest::new(
        "Hello, world!".to_string(),
        "zh".to_string(),
    )
    .with_source_lang("en");

    match translator.translate(&test_request).await {
        Ok(result) => {
            println!("✅ 翻译成功!");
            println!("   原文: {}", test_request.text);
            println!("   译文: {}", result.translation);
            println!("   模型: {}", result.model_used);
            println!("   Token 使用: {}", result.tokens_used);
            if let Some(lang) = result.detected_source_lang {
                println!("   检测源语言: {}", lang);
            }
        }
        Err(e) => {
            println!("❌ 翻译失败: {}", e);
            println!("   错误详情: {:?}", e);

            // 尝试快车道模型
            println!("\n--- 尝试快车道模型 ---");
            let fast_request = doubao_translator::core::models::TranslationRequest::new(
                "Hello, world!".to_string(),
                "zh".to_string(),
            ).with_source_lang("en");

            // 直接使用快车道模型
            let models = translator.get_available_models();
            for model in models {
                if model.lane == doubao_translator::core::models::LaneType::Fast {
                    println!("尝试模型: {}", model.id);
                    let fast_result = translator.translate(&fast_request).await;
                    match fast_result {
                        Ok(result) => {
                            println!("✅ 快车道翻译成功!");
                            println!("   译文: {}", result.translation);
                            println!("   模型: {}", result.model_used);
                            break;
                        }
                        Err(e) => {
                            println!("❌ 模型 {} 失败: {}", model.id, e);
                        }
                    }
                }
            }
        }
    }

    println!("\n=== 测试完成 ===");
}
