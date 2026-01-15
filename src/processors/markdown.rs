//! Markdown file processor with intelligent translation

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

use crate::core::client::AsyncTranslator;
use crate::core::errors::{Result, TranslationError};
use crate::core::models::TranslationRequest;

/// Markdown processor that preserves code blocks and links
#[derive(Debug, Clone)]
pub struct MarkdownProcessor {
    translator: AsyncTranslator,
}

impl MarkdownProcessor {
    /// Create a new markdown processor
    pub fn new(translator: AsyncTranslator) -> Self {
        Self { translator }
    }

    /// Create from environment configuration
    pub fn from_env() -> Result<Self> {
        let translator = AsyncTranslator::from_env()?;
        Ok(Self::new(translator))
    }

    /// Find Markdown files in directory
    pub fn find_files(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        if !dir.is_dir() {
            return Err(TranslationError::FileError {
                path: dir.display().to_string(),
                message: "Not a directory".to_string(),
            });
        }

        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() && self.is_markdown_file(&path) {
                files.push(path);
            }
        }

        Ok(files)
    }

    /// Find Markdown files recursively
    pub fn find_files_recursive(&self, dir: &Path) -> Result<Vec<PathBuf>> {
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
            if path.is_file() && self.is_markdown_file(path) {
                files.push(path.to_path_buf());
            }
        }

        Ok(files)
    }

    /// Check if file is Markdown
    fn is_markdown_file(&self, path: &Path) -> bool {
        path.extension()
            .map(|ext| {
                let ext = ext.to_string_lossy().to_lowercase();
                ext == "md" || ext == "markdown"
            })
            .unwrap_or(false)
    }

    /// Translate a single Markdown file
    pub async fn translate_file(
        &self,
        input: &Path,
        output: &Path,
        target_lang: &str,
        source_lang: Option<String>,
    ) -> Result<()> {
        debug!("Translating: {}", input.display());

        // Read file content
        let content = tokio::fs::read_to_string(input)
            .await
            .map_err(|e| TranslationError::FileError {
                path: input.display().to_string(),
                message: e.to_string(),
            })?;

        // Parse and translate
        let translated = self
            .translate_content(&content, target_lang, source_lang.clone())
            .await?;

        // Ensure output directory exists
        if let Some(parent) = output.parent() {
            if !parent.exists() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| TranslationError::FileError {
                        path: parent.display().to_string(),
                        message: e.to_string(),
                    })?;
            }
        }

        // Write translated content
        tokio::fs::write(output, translated)
            .await
            .map_err(|e| TranslationError::FileError {
                path: output.display().to_string(),
                message: e.to_string(),
            })?;

        info!("Translated: {} -> {}", input.display(), output.display());
        Ok(())
    }

    /// Translate Markdown content
    async fn translate_content(
        &self,
        content: &str,
        target_lang: &str,
        source_lang: Option<String>,
    ) -> Result<String> {
        // Extract special elements
        let mut extractor = MarkdownExtractor::new(content);
        extractor.extract();

        // Translate regular text segments
        let mut translated_segments = Vec::new();
        for segment in &extractor.text_segments {
            let request = TranslationRequest::new(segment.clone(), target_lang.to_string())
                .with_source_lang(source_lang.clone().unwrap_or_else(|| "auto".to_string()));

            match self.translator.translate(&request).await {
                Ok(result) => {
                    translated_segments.push(result.translation);
                }
                Err(e) => {
                    warn!("Translation failed for segment '{}': {}", segment, e);
                    // Keep original text if translation fails
                    translated_segments.push(segment.clone());
                }
            }
        }

        // Extract elements before translation to avoid borrow issues
        let elements = extractor.elements.clone();
        let link_texts: Vec<String> = elements
            .iter()
            .filter_map(|e| {
                if let MarkdownElement::Link(start, end) = e {
                    Some(extractor.get_link_text(*start, *end))
                } else {
                    None
                }
            })
            .collect();

        // Reconstruct content
        let mut result = String::new();
        let mut text_idx = 0;
        let mut char_idx = 0;
        let mut link_idx = 0;

        for element in &elements {
            match element {
                MarkdownElement::Text(_start, end) => {
                    if text_idx < translated_segments.len() {
                        result.push_str(&translated_segments[text_idx]);
                        text_idx += 1;
                    }
                    char_idx = *end;
                }
                MarkdownElement::CodeBlock(start, end) | MarkdownElement::InlineCode(start, end) => {
                    result.push_str(&content[*start..*end]);
                    char_idx = *end;
                }
                MarkdownElement::Link(start, end) => {
                    // Keep URL, translate text
                    let link_text = if link_idx < link_texts.len() {
                        &link_texts[link_idx]
                    } else {
                        ""
                    };
                    link_idx += 1;

                    if !link_text.is_empty() && text_idx < translated_segments.len() {
                        // Replace link text with translation
                        let original_text = &content[*start..*end];
                        let translated = &translated_segments[text_idx];
                        let new_link = original_text.replace(link_text, translated);
                        result.push_str(&new_link);
                        text_idx += 1;
                    } else {
                        result.push_str(&content[*start..*end]);
                    }
                    char_idx = *end;
                }
                MarkdownElement::YamlFrontmatter(start, end) => {
                    let yaml_content = &content[*start..*end];
                    let translated_yaml = self
                        .translate_yaml_frontmatter(yaml_content, target_lang, source_lang.clone())
                        .await?;
                    result.push_str(&translated_yaml);
                    char_idx = *end;
                }
            }
        }

        // Append any remaining text
        if char_idx < content.len() {
            result.push_str(&content[char_idx..]);
        }

        Ok(result)
    }

    /// Translate YAML frontmatter
    async fn translate_yaml_frontmatter(
        &self,
        yaml_content: &str,
        target_lang: &str,
        source_lang: Option<String>,
    ) -> Result<String> {
        // Parse YAML
        let yaml: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(yaml_content)
            .map_err(|e| TranslationError::InvalidFormat {
                format: format!("YAML: {}", e),
            })?;

        let mut translated = HashMap::new();

        for (key, value) in yaml {
            // Fields to translate
            let fields_to_translate = ["title", "description", "summary", "name", "alt"];

            if fields_to_translate.contains(&key.as_str()) {
                if let serde_yaml::Value::String(text) = value {
                    let request = TranslationRequest::new(text.clone(), target_lang.to_string())
                        .with_source_lang(source_lang.clone().unwrap_or_else(|| "auto".to_string()));

                    if let Ok(result) = self.translator.translate(&request).await {
                        translated.insert(key, serde_yaml::Value::String(result.translation));
                    } else {
                        translated.insert(key, serde_yaml::Value::String(text));
                    }
                } else {
                    translated.insert(key, value);
                }
            } else {
                // Keep other fields unchanged
                translated.insert(key, value);
            }
        }

        // Re-serialize
        let result = serde_yaml::to_string(&translated)
            .map_err(|e| TranslationError::InvalidFormat {
                format: format!("YAML serialization: {}", e),
            })?;

        Ok(format!("---\n{}---\n", result))
    }
}

/// Markdown element types
#[derive(Debug, Clone, Copy)]
enum MarkdownElement {
    Text(usize, usize),
    CodeBlock(usize, usize),
    InlineCode(usize, usize),
    Link(usize, usize),
    YamlFrontmatter(usize, usize),
}

/// Markdown extractor for parsing content
struct MarkdownExtractor<'a> {
    content: &'a str,
    elements: Vec<MarkdownElement>,
    text_segments: Vec<String>,
}

impl<'a> MarkdownExtractor<'a> {
    fn new(content: &'a str) -> Self {
        Self {
            content,
            elements: Vec::new(),
            text_segments: Vec::new(),
        }
    }

    fn extract(&mut self) {
        let mut pos = 0;
        let chars: Vec<char> = self.content.chars().collect();

        // Check for YAML frontmatter
        if self.content.starts_with("---\n") {
            if let Some(end) = self.content.find("\n---\n") {
                let yaml_end = end + 5;
                self.elements
                    .push(MarkdownElement::YamlFrontmatter(0, yaml_end));
                pos = yaml_end;
            }
        }

        while pos < chars.len() {
            // Code block
            if pos + 2 < chars.len() && chars[pos] == '`' && chars[pos + 1] == '`' && chars[pos + 2] == '`' {
                if let Some(end) = self.content[pos..].find("```") {
                    let end_pos = pos + end + 3;
                    self.elements
                        .push(MarkdownElement::CodeBlock(pos, end_pos));
                    pos = end_pos;
                    continue;
                }
            }

            // Inline code
            if chars[pos] == '`' {
                if let Some(end) = self.content[pos + 1..].find('`') {
                    let end_pos = pos + end + 2;
                    self.elements
                        .push(MarkdownElement::InlineCode(pos, end_pos));
                    pos = end_pos;
                    continue;
                }
            }

            // Links
            if chars[pos] == '[' {
                if let Some(link_end) = self.content[pos..].find(']') {
                    let bracket_end = pos + link_end + 1;
                    if bracket_end < chars.len() && chars[bracket_end] == '(' {
                        if let Some(url_end) = self.content[bracket_end..].find(')') {
                            let link_end_pos = bracket_end + url_end + 1;
                            self.elements
                                .push(MarkdownElement::Link(pos, link_end_pos));

                            // Extract link text for translation
                            let text_start = pos + 1;
                            let text_end = bracket_end - 1;
                            if text_end > text_start {
                                let text = self.content[text_start..text_end].trim().to_string();
                                if !text.is_empty() {
                                    self.text_segments.push(text);
                                }
                            }

                            pos = link_end_pos;
                            continue;
                        }
                    }
                }
            }

            // Regular text - find next special element
            let mut text_end = pos;
            let mut has_special = false;

            for i in pos..chars.len() {
                if (i + 2 < chars.len()
                    && chars[i] == '`'
                    && chars[i + 1] == '`'
                    && chars[i + 2] == '`')
                    || chars[i] == '`'
                    || chars[i] == '['
                {
                    text_end = i;
                    has_special = true;
                    break;
                }
            }

            if !has_special {
                text_end = chars.len();
            }

            if text_end > pos {
                let text = self.content[pos..text_end].trim();
                if !text.is_empty() {
                    self.elements
                        .push(MarkdownElement::Text(pos, text_end));
                    self.text_segments.push(text.to_string());
                }
                pos = text_end;
            } else {
                pos += 1;
            }
        }
    }

    fn get_link_text(&self, start: usize, end: usize) -> String {
        let content = &self.content[start..end];
        if let Some(bracket_end) = content.find(']') {
            if bracket_end > 1 {
                return content[1..bracket_end].to_string();
            }
        }
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_extractor() {
        let content = r#"---
title: "Test"
description: "A test document"
---

# Hello World

This is a `code` block.

```rust
fn main() {
    println!("Hello");
}
```

Check out [this link](https://example.com) for more info."#;

        let mut extractor = MarkdownExtractor::new(content);
        extractor.extract();

        // Should have extracted YAML, text segments, code blocks, and links
        assert!(!extractor.elements.is_empty());
        assert!(!extractor.text_segments.is_empty());
    }

    #[test]
    fn test_is_markdown_file() {
        let processor = MarkdownProcessor::new(
            AsyncTranslator::new(crate::core::config::TranslatorConfig::default()).unwrap(),
        );

        assert!(processor.is_markdown_file(Path::new("test.md")));
        assert!(processor.is_markdown_file(Path::new("test.MD")));
        assert!(processor.is_markdown_file(Path::new("test.markdown")));
        assert!(!processor.is_markdown_file(Path::new("test.txt")));
    }
}