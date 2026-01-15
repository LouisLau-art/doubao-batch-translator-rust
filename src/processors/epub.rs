//! ePub file processor with translation and leak detection

use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::core::client::AsyncTranslator;
use crate::core::errors::{Result, TranslationError};
use crate::core::models::TranslationRequest;

/// ePub processor for translation and leak detection
#[derive(Debug, Clone)]
pub struct EpubProcessor {
    translator: AsyncTranslator,
}

impl EpubProcessor {
    /// Create a new ePub processor
    pub fn new(translator: AsyncTranslator) -> Self {
        Self { translator }
    }

    /// Create from environment configuration
    pub fn from_env() -> Result<Self> {
        let translator = AsyncTranslator::from_env()?;
        Ok(Self::new(translator))
    }

    /// Find ePub files in directory
    pub fn find_epub_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        if !dir.is_dir() {
            return Err(TranslationError::FileError {
                path: dir.display().to_string(),
                message: "Not a directory".to_string(),
            });
        }

        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "epub").unwrap_or(false) {
                files.push(path.to_path_buf());
            }
        }

        Ok(files)
    }

    /// Translate ePub file
    pub async fn translate_epub(
        &self,
        input: &Path,
        output: &Path,
        target_lang: &str,
        source_lang: Option<String>,
        _auto_approve: bool,
    ) -> Result<()> {
        debug!("Translating ePub: {}", input.display());

        // 打开并解析 ePub 文件
        let mut book = epub::doc::EpubDoc::new(input)?;

        // 获取书的元数据
        let book_title = book.get_title().unwrap_or_else(|| "Unknown Title".to_string());
        info!("Translating book: {}", book_title);

        // 获取所有章节
        let spine = book.spine.clone();
        info!("Found {} chapters", spine.len());

        // 翻译每个章节
        let mut translated_chapters = Vec::with_capacity(spine.len());
        for (i, item) in spine.iter().enumerate() {
            debug!("Translating chapter {}: {}", i + 1, item.idref);

            // 获取章节内容
            if let Some((content, _mime)) = book.get_resource(&item.idref) {
                let content_str = String::from_utf8_lossy(&content).to_string();

                // 翻译章节内容
                let translated_content = self.translate_html_content(
                    &content_str,
                    target_lang,
                    source_lang.as_deref(),
                ).await?;

                translated_chapters.push((item.idref.clone(), translated_content));
            } else {
                warn!("Failed to get content for chapter: {}", item.idref);
            }
        }

        // 重新打包 ePub
        self.repack_epub(input, output, &translated_chapters, &book.resources).await?;

        info!("ePub translation complete: {} -> {}", input.display(), output.display());
        Ok(())
    }

    /// 翻译 HTML 内容
    async fn translate_html_content(
        &self,
        html: &str,
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<String> {
        // 简单的 HTML 标签保留逻辑（实际项目中应该使用更复杂的解析）
        // 这里使用简单的方法：保留标签，翻译文本内容

        let mut translated = String::new();
        let mut in_tag = false;
        let mut buffer = String::new();

        for c in html.chars() {
            match c {
                '<' => {
                    in_tag = true;
                    if !buffer.is_empty() {
                        // 翻译缓冲的文本
                        let translated_text = self.translate_text(&buffer, target_lang, source_lang).await?;
                        translated.push_str(&translated_text);
                        buffer.clear();
                    }
                    translated.push(c);
                }
                '>' => {
                    in_tag = false;
                    translated.push(c);
                }
                _ => {
                    if in_tag {
                        translated.push(c);
                    } else {
                        buffer.push(c);
                    }
                }
            }
        }

        // 翻译最后剩余的文本
        if !buffer.is_empty() {
            let translated_text = self.translate_text(&buffer, target_lang, source_lang).await?;
            translated.push_str(&translated_text);
        }

        Ok(translated)
    }

    /// 翻译纯文本内容
    async fn translate_text(
        &self,
        text: &str,
        target_lang: &str,
        source_lang: Option<&str>,
    ) -> Result<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Ok(text.to_string());
        }

        let request = TranslationRequest::new(trimmed.to_string(), target_lang.to_string())
            .with_source_lang(source_lang.unwrap_or("auto"));

        let result = self.translator.translate(&request).await?;
        Ok(text.replace(trimmed, &result.translation))
    }

    /// 重新打包 ePub 文件
    async fn repack_epub(
        &self,
        input: &Path,
        output: &Path,
        translated_chapters: &[(String, String)],
        resources: &std::collections::HashMap<String, epub::doc::ResourceItem>,
    ) -> Result<()> {
        // 读取原始 ePub 文件
        let file = tokio::fs::read(input).await?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(file))?;

        // 创建新的 ePub 文件（使用标准库的 File）
        let file = std::fs::File::create(output)?;
        let mut writer = zip::ZipWriter::new(file);

        // 复制所有文件，替换翻译后的章节
        for i in 0..zip.len() {
            let mut file = zip.by_index(i)?;
            let file_name = file.name().to_string();

            // 获取资源路径
            let mut resource_path = None;
            for (id, content) in translated_chapters.iter() {
                // 查找资源对应的路径
                let mut found = false;
                if let Some(resource) = resources.get(id) {
                    let resource_str = resource.path.to_str().unwrap_or("");
                    if file_name.contains(resource_str) {
                        resource_path = Some((id, content));
                        found = true;
                    }
                }
                if found {
                    break;
                }
            }

            if let Some((_, content)) = resource_path {
                debug!("Replacing: {}", file_name);
                let options = zip::write::FileOptions::default()
                    .compression_method(file.compression());

                writer.start_file(file_name, options)?;
                writer.write_all(content.as_bytes())?;
            } else {
                // 复制原始文件
                let options = zip::write::FileOptions::default()
                    .compression_method(file.compression());

                writer.start_file(file_name, options)?;
                let mut buffer = Vec::new();
                std::io::copy(&mut file, &mut buffer)?;
                writer.write_all(&buffer)?;
            }
        }

        writer.finish()?;
        Ok(())
    }

    /// Generate leak report
    pub async fn generate_leak_report(
        &self,
        dir: &Path,
        _target_lang: &str,
    ) -> Result<()> {
        info!("Generating leak report for: {}", dir.display());
        // TODO: Implement leak detection
        Ok(())
    }

    /// Check for untranslated content
    pub async fn check_untranslated(&self, dir: &Path) -> Result<Vec<LeakInfo>> {
        info!("Checking untranslated in: {}", dir.display());

        let mut leaks = Vec::new();

        // 检查目录中的所有 ePub 文件
        let epub_files = self.find_epub_files(dir)?;
        for file_path in epub_files {
            if let Ok(mut book) = epub::doc::EpubDoc::new(&file_path) {
                let book_name = book.get_title().unwrap_or_else(|| {
                    file_path.file_name().unwrap_or_default().to_string_lossy().to_string()
                });

                // 克隆 spine 避免借用冲突
                let spine = book.spine.clone();

                // 检查每个章节
                for (_i, item) in spine.iter().enumerate() {
                    if let Some((content, _mime)) = book.get_resource(&item.idref) {
                        let content_str = String::from_utf8_lossy(&content).to_string();

                        // 简单的漏译检测：检查是否包含大量英文内容（可以根据需要调整）
                        if self.has_untranslated_content(&content_str) {
                            leaks.push(LeakInfo {
                                book_name: book_name.clone(),
                                file_path: file_path.display().to_string(),
                                original: content_str.clone(),
                                translation: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(leaks)
    }

    /// 提取纯文本内容（移除 HTML/XML 标签）
    fn extract_text_content(&self, html_content: &str) -> String {
        // 简单的文本提取，移除标签
        let mut result = String::new();
        let mut in_tag = false;
        let mut chars = html_content.chars().peekable();

        while let Some(ch) = chars.next() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ => {
                    if !in_tag && !ch.is_whitespace() {
                        result.push(ch);
                    }
                }
            }
        }

        // 如果简单方法不够好，可以考虑使用 html5ever 或其他 HTML 解析库
        result
    }

    /// 检查内容是否包含未翻译的内容
    fn has_untranslated_content(&self, content: &str) -> bool {
        // 更准确的漏译检测逻辑：
        // 1. 忽略 XML 标签和属性
        // 2. 只检查文本内容
        // 3. 使用更合理的阈值

        // 提取纯文本内容（移除 HTML/XML 标签）
        let text_content = self.extract_text_content(content);

        // 如果没有文本内容，返回 false
        if text_content.trim().is_empty() {
            return false;
        }

        // 计算英文单词的比例
        let words: Vec<&str> = text_content.split_whitespace().collect();
        let english_word_count = words.iter()
            .filter(|&&word| {
                // 只计算纯英文字母组成的单词
                !word.is_empty() &&
                word.chars().all(|c| c.is_ascii_alphabetic()) &&
                word.len() > 1 // 忽略单个字母
            })
            .count();

        let total_word_count = words.len();

        if total_word_count == 0 {
            return false;
        }

        // 如果英文单词比例超过 70% 且总单词数大于 5，则认为可能是未翻译的内容
        let english_ratio = english_word_count as f64 / total_word_count as f64;
        english_ratio > 0.7 && total_word_count > 5
    }

    /// Save leak report to JSON
    pub async fn save_leak_report(&self, leaks: &[LeakInfo], path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(leaks)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }

    /// Apply fixes from JSON file
    pub async fn apply_fixes(&self, json_path: &Path) -> Result<usize> {
        info!("Applying fixes from: {}", json_path.display());

        // 读取修复文件
        let content = tokio::fs::read_to_string(json_path).await?;
        let leaks: Vec<LeakInfo> = serde_json::from_str(&content)?;

        let mut fixed_count = 0;

        // 遍历所有漏译信息
        for leak in leaks.iter() {
            if let Some(_translation) = &leak.translation {
                // 这里应该实现将翻译内容应用到 ePub 文件的逻辑
                // 目前只是一个占位符
                debug!("Applying fix to book: {}, file: {}", leak.book_name, leak.file_path);
                fixed_count += 1;
            }
        }

        Ok(fixed_count)
    }
}

/// Leak information for manual translation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LeakInfo {
    pub book_name: String,
    pub file_path: String,
    pub original: String,
    pub translation: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_epub_processor_creation() {
        // 测试从环境变量创建处理器
        let result = EpubProcessor::from_env();
        // 这可能会失败，因为需要 ARK_API_KEY 环境变量
        // 如果没有设置环境变量，我们会得到错误，但这是预期的
        debug!("EpubProcessor::from_env() result: {:?}", result);
    }

    #[tokio::test]
    async fn test_find_epub_files() {
        // 创建临时目录
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 创建测试文件
        std::fs::File::create(temp_path.join("test1.epub")).unwrap();
        std::fs::File::create(temp_path.join("test2.epub")).unwrap();
        std::fs::File::create(temp_path.join("not_epub.txt")).unwrap();

        // 创建子目录
        let sub_dir = temp_path.join("subdir");
        std::fs::create_dir(&sub_dir).unwrap();
        std::fs::File::create(sub_dir.join("test3.epub")).unwrap();

        // 创建处理器（使用空配置）
        let translator = crate::core::client::AsyncTranslator::from_env().unwrap();
        let processor = EpubProcessor::new(translator);

        // 测试查找 ePub 文件
        let files = processor.find_epub_files(temp_path).unwrap();
        assert_eq!(files.len(), 3);

        // 检查文件名是否正确
        let file_names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(file_names.contains(&"test1.epub".to_string()));
        assert!(file_names.contains(&"test2.epub".to_string()));
        assert!(file_names.contains(&"test3.epub".to_string()));
    }

    #[tokio::test]
    async fn test_has_untranslated_content() {
        let translator = crate::core::client::AsyncTranslator::from_env().unwrap();
        let processor = EpubProcessor::new(translator);

        // 测试英文内容
        let english_content = "This is a test paragraph. It contains only English text.";
        assert!(processor.has_untranslated_content(english_content));

        // 测试中文内容
        let chinese_content = "这是一个测试段落。它只包含中文文本。";
        assert!(!processor.has_untranslated_content(chinese_content));

        // 测试混合内容
        let mixed_content = "This is a test 测试段落. It contains both English and 中文.";
        assert!(processor.has_untranslated_content(mixed_content));
    }

    #[tokio::test]
    async fn test_check_untranslated() {
        // 创建临时目录
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // 创建处理器（使用空配置）
        let translator = crate::core::client::AsyncTranslator::from_env().unwrap();
        let processor = EpubProcessor::new(translator);

        // 测试空目录
        let leaks = processor.check_untranslated(temp_path).await.unwrap();
        assert!(leaks.is_empty());
    }
}