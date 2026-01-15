use doubao_translator::processors::epub::EpubProcessor;
use dotenvy::dotenv;
use std::path::Path;

#[tokio::main]
async fn main() {
    // 加载环境变量
    dotenv().ok();

    // 初始化日志
    tracing_subscriber::fmt::init();

    println!("=== ePub 翻译功能测试 ===");

    // 测试文件路径
    let input_path = Path::new("/home/louis/test_book_fixed.epub");
    let output_path = Path::new("/home/louis/test_book_fixed_zh.epub");

    // 检查测试文件是否存在
    if !input_path.exists() {
        println!("❌ 测试文件不存在: {:?}", input_path);
        println!("请先创建测试 ePub 文件");
        return;
    }

    println!("输入文件: {:?}", input_path);
    println!("输出文件: {:?}", output_path);

    // 创建 ePub 处理器
    match EpubProcessor::from_env() {
        Ok(processor) => {
            println!("✅ ePub 处理器创建成功");

            // 测试查找 ePub 文件功能
            match processor.find_epub_files(Path::new("/home/louis")) {
                Ok(files) => {
                    println!("✅ 找到 {} 个 ePub 文件", files.len());
                    for file in &files {
                        println!("  - {}", file.display());
                    }
                }
                Err(e) => {
                    println!("❌ 查找 ePub 文件失败: {}", e);
                }
            }

            // 测试漏译检测功能

            match processor.check_untranslated(Path::new("/home/louis")).await {
                Ok(leaks) => {
                    println!("✅ 漏译检测完成，找到 {} 个可能的漏译", leaks.len());
                    if !leaks.is_empty() {
                        for leak in &leaks {
                            println!("  - 书名: {}", leak.book_name);
                            println!("    文件: {}", leak.file_path);
                            println!("    原文: {}...", &leak.original[..std::cmp::min(leak.original.len(), 50)]);
                        }
                    }
                }
                Err(e) => {
                    println!("❌ 漏译检测失败: {}", e);
                }
            }

            // 测试 ePub 翻译功能
            println!("\n--- 开始 ePub 翻译测试 ---");
            match processor.translate_epub(
                input_path,
                output_path,
                "zh",
                Some("en".to_string()),
                true,
            ).await {
                Ok(_) => {
                    println!("✅ ePub 翻译测试成功");

                    // 检查输出文件是否存在
                    if output_path.exists() {
                        println!("✅ 翻译后的 ePub 文件已创建: {:?}", output_path);
                        println!("📊 文件大小: {} bytes", std::fs::metadata(output_path).unwrap().len());
                    } else {
                        println!("⚠️  输出文件不存在，但翻译过程未报错");
                    }
                }
                Err(e) => {
                    println!("❌ ePub 翻译测试失败: {}", e);
                    println!("错误详情: {:?}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ ePub 处理器创建失败: {}", e);
            println!("请检查 ARK_API_KEY 环境变量是否设置正确");
        }
    }

    println!("\n=== 测试完成 ===");
}