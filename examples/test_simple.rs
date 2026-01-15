//! 简单的 API 测试

use reqwest::Client;
use serde_json::json;
use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    dotenv().ok();

    let api_key = std::env::var("ARK_API_KEY").expect("ARK_API_KEY must be set");
    let api_endpoint = std::env::var("API_ENDPOINT").unwrap_or_else(|_| "https://ark.cn-beijing.volces.com/api/v3/responses".to_string());

    println!("=== 简单 API 测试 ===");
    println!("API Endpoint: {}", api_endpoint);

    let client = Client::new();

    // 测试请求 - 使用最简单的模型
    let request_body = json!({
        "model": "doubao-seed-translation-250915",
        "input": [{
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": "Hello, world!",
                "translation_options": {
                    "target_language": "zh"
                }
            }]
        }]
    });

    println!("\n发送请求到: {}", api_endpoint);
    println!("请求体: {}", serde_json::to_string_pretty(&request_body).unwrap());

    let response = client
        .post(&api_endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await;

    match response {
        Ok(resp) => {
            let status = resp.status();
            println!("\n响应状态: {}", status);

            let text = resp.text().await.unwrap_or_default();
            println!("响应内容: {}", text);

            if status.is_success() {
                println!("\n✅ API 调用成功！");
            } else {
                println!("\n❌ API 调用失败，状态码: {}", status);
            }
        }
        Err(e) => {
            println!("\n❌ 请求失败: {}", e);
        }
    }

    println!("\n=== 测试完成 ===");
}
