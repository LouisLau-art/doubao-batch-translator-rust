//! 测试 ePub 文件解析

use epub::doc::EpubDoc;
use std::path::Path;

fn main() {
    println!("=== ePub 文件解析测试 ===");

    let epub_path = Path::new("/home/louis/test_book_fixed.epub");

    if !epub_path.exists() {
        println!("❌ ePub 文件不存在: {:?}", epub_path);
        return;
    }

    println!("📖 尝试打开 ePub 文件: {:?}", epub_path);

    match EpubDoc::new(epub_path) {
        Ok(mut book) => {
            println!("✅ ePub 文件解析成功");

            // 获取元数据
            let mut book_title = "未知";
            for item in &book.metadata {
                if item.property == "title" {
                    book_title = &item.value;
                    break;
                }
            }
            println!("📚 书名: {}", book_title);

            // 遍历章节
            println!("📖 章节数量: {}", book.spine.len());
            println!("📄 资源数量: {}", book.resources.len());

            // 只测试是否可以打开文件，不遍历内容
            for spine_item in &book.spine {
                println!("📝 章节ID: {}", spine_item.idref);
                // 这里只打印ID，不尝试获取内容以避免借用冲突
            }
        }
        Err(e) => {
            println!("❌ ePub 文件解析失败: {}", e);
            println!("错误类型: {:?}", e);
        }
    }

    println!("\n=== 测试完成 ===");
}