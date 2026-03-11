//! Core formatting logic for .ai workflow files

use super::error::FormatterError;
use super::FormatterResult;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct SchemaBlockInfo {
    prefix: String,
    properties_content: String,
}

/// Configuration options for the formatter
#[derive(Debug, Clone)]
pub struct FormatterConfig {
    /// Number of spaces for indentation
    pub indent_size: usize,
    /// Maximum line length before wrapping
    pub max_line_length: usize,
    /// Whether to trim trailing whitespace
    pub trim_trailing_whitespace: bool,
    /// Whether to ensure files end with a newline
    pub ensure_final_newline: bool,
}

impl Default for FormatterConfig {
    fn default() -> Self {
        Self {
            indent_size: 4,
            max_line_length: 100,
            trim_trailing_whitespace: true,
            ensure_final_newline: true,
        }
    }
}

/// Result of a formatting operation
#[derive(Debug, Clone, PartialEq)]
pub struct FormatResult {
    /// The formatted content
    pub content: String,
    /// Whether the content was changed
    pub changed: bool,
}

/// Main formatter for .ai workflow files
pub struct Formatter {
    config: FormatterConfig,
}

impl Formatter {
    /// Create a new formatter with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: FormatterConfig::default(),
        }
    }

    /// Create a new formatter with custom configuration
    #[must_use]
    pub fn with_config(config: FormatterConfig) -> Self {
        Self { config }
    }

    /// Format all .ai files in a directory
    pub fn format_directory(&self, directory_path: &Path) -> FormatterResult<Vec<PathBuf>> {
        if !directory_path.exists() {
            return Err(FormatterError::DirectoryNotFound {
                path: directory_path.to_path_buf(),
            });
        }

        if !directory_path.is_dir() {
            return Err(FormatterError::NotADirectory {
                path: directory_path.to_path_buf(),
            });
        }

        let ai_files = self.find_ai_files(directory_path)?;

        if ai_files.is_empty() {
            return Err(FormatterError::NoFilesFound {
                path: directory_path.to_path_buf(),
            });
        }

        let mut formatted_files = Vec::new();

        for file_path in ai_files {
            match self.format_file(&file_path) {
                Ok(result) => {
                    if result.changed {
                        self.write_file(&file_path, &result.content)?;
                        formatted_files.push(file_path);
                    }
                }
                Err(error) => {
                    log::warn!("Failed to format {}: {}", file_path.display(), error);
                }
            }
        }

        Ok(formatted_files)
    }

    /// Check formatting of all .ai files in a directory without modifying them
    pub fn check_directory(&self, directory_path: &Path) -> FormatterResult<Vec<PathBuf>> {
        if !directory_path.exists() {
            return Err(FormatterError::DirectoryNotFound {
                path: directory_path.to_path_buf(),
            });
        }

        if !directory_path.is_dir() {
            return Err(FormatterError::NotADirectory {
                path: directory_path.to_path_buf(),
            });
        }

        let ai_files = self.find_ai_files(directory_path)?;

        if ai_files.is_empty() {
            return Err(FormatterError::NoFilesFound {
                path: directory_path.to_path_buf(),
            });
        }

        let mut unformatted_files = Vec::new();

        for file_path in ai_files {
            match self.format_file(&file_path) {
                Ok(result) => {
                    if result.changed {
                        unformatted_files.push(file_path);
                    }
                }
                Err(error) => {
                    log::warn!("Failed to check {}: {}", file_path.display(), error);
                }
            }
        }

        Ok(unformatted_files)
    }

    /// Format a single file and return the result
    pub fn format_file(&self, file_path: &Path) -> FormatterResult<FormatResult> {
        if !self.is_ai_file(file_path) {
            return Err(FormatterError::InvalidExtension {
                path: file_path.to_path_buf(),
            });
        }

        let content = self.read_file(file_path)?;
        let formatted_content = self.format_content(&content)?;

        let changed = content != formatted_content;

        Ok(FormatResult {
            content: formatted_content,
            changed,
        })
    }

    /// Format content string
    pub fn format_content(&self, content: &str) -> FormatterResult<String> {
        let mut lines: Vec<String> = content.lines().map(std::string::ToString::to_string).collect();

        // Apply formatting rules
        self.normalize_indentation(&mut lines);
        self.normalize_operator_spacing(&mut lines);
        self.normalize_colon_spacing(&mut lines);
        self.normalize_schema_properties(&mut lines);
        self.normalize_long_strings(&mut lines);
        self.normalize_triple_quote_indentation(&mut lines);
        self.trim_trailing_whitespace(&mut lines);
        self.normalize_blank_lines(&mut lines);

        let mut result = lines.join("\n");

        // Ensure final newline if configured
        if self.config.ensure_final_newline && !result.ends_with('\n') && !result.is_empty() {
            result.push('\n');
        }

        Ok(result)
    }

    /// Find all .ai files in a directory recursively
    fn find_ai_files(&self, directory_path: &Path) -> FormatterResult<Vec<PathBuf>> {
        let mut ai_files = Vec::new();

        fn visit_dir(directory: &Path, ai_files: &mut Vec<PathBuf>) -> std::io::Result<()> {
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    visit_dir(&path, ai_files)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("ai") {
                    ai_files.push(path);
                }
            }
            Ok(())
        }

        visit_dir(directory_path, &mut ai_files).map_err(FormatterError::Io)?;

        ai_files.sort();
        Ok(ai_files)
    }

    /// Check if a file has .ai extension
    fn is_ai_file(&self, file_path: &Path) -> bool {
        file_path.extension().and_then(|s| s.to_str()) == Some("ai")
    }

    /// Read file content
    fn read_file(&self, file_path: &Path) -> FormatterResult<String> {
        fs::read_to_string(file_path).map_err(|source| FormatterError::FileRead {
            path: file_path.to_path_buf(),
            source,
        })
    }

    /// Write content to file
    pub fn write_file(&self, file_path: &Path, content: &str) -> FormatterResult<()> {
        fs::write(file_path, content).map_err(|source| FormatterError::FileWrite {
            path: file_path.to_path_buf(),
            source,
        })
    }

    /// Normalize indentation using spaces
    fn normalize_indentation(&self, lines: &mut Vec<String>) {
        let mut indent_levels = Vec::new();
        let mut current_level: usize = 0;

        for line in lines.iter() {
            if line.trim().is_empty() {
                indent_levels.push(current_level);
                continue;
            }

            let content = line.trim();

            // Check if this line should decrease indentation (closing braces)
            if content.starts_with('}') {
                current_level = current_level.saturating_sub(1);
            }

            indent_levels.push(current_level);

            // Check if this line should increase indentation for next lines
            if content.ends_with('{') {
                current_level += 1;
            }
        }

        // Apply normalized indentation
        for (line, &level) in lines.iter_mut().zip(indent_levels.iter()) {
            if line.trim().is_empty() {
                *line = String::new();
                continue;
            }

            let content = line.trim();
            let normalized_indent = " ".repeat(level * self.config.indent_size);
            *line = format!("{normalized_indent}{content}");
        }
    }

    /// Trim trailing whitespace from lines
    fn trim_trailing_whitespace(&self, lines: &mut Vec<String>) {
        if !self.config.trim_trailing_whitespace {
            return;
        }

        for line in lines.iter_mut() {
            *line = line.trim_end().to_string();
        }
    }

    /// Normalize operator spacing (ensure spaces around <- operator)
    fn normalize_operator_spacing(&self, lines: &mut [String]) {
        for line in lines.iter_mut() {
            // Skip empty lines and comments
            if line.trim().is_empty() || line.trim_start().starts_with("//") {
                continue;
            }

            // Handle <- operator spacing
            if line.contains("<-") {
                // Preserve leading whitespace
                let leading_whitespace = line.len() - line.trim_start().len();
                let leading_spaces = &line[..leading_whitespace];
                let content = line.trim_start();

                let result = if content.starts_with("<-") {
                    // Special case: if line starts with <-, only add space after
                    let after_arrow = &content[2..].trim_start();
                    format!("<- {}", after_arrow)
                } else {
                    // Regular case: ensure space before and after <-
                    // Use a more precise approach to avoid adding extra spaces
                    let mut result = String::new();
                    let content_chars: Vec<char> = content.chars().collect();
                    let mut i = 0;

                    while i < content_chars.len() {
                        if i < content_chars.len() - 1 && content_chars[i] == '<' && content_chars[i + 1] == '-' {
                            // Found <- operator
                            // Add space before if not already present
                            if !result.is_empty() && !result.ends_with(' ') {
                                result.push(' ');
                            }
                            result.push_str("<-");
                            i += 2;

                            // Add space after if not already present and there's more content
                            if i < content_chars.len() && content_chars[i] != ' ' {
                                result.push(' ');
                            }
                        } else {
                            result.push(content_chars[i]);
                            i += 1;
                        }
                    }

                    result
                };

                *line = format!("{leading_spaces}{result}");
            }
        }
    }

    /// Normalize colon spacing in schema properties (e.g., "key: value")
    fn normalize_colon_spacing(&self, lines: &mut [String]) {
        for line in lines.iter_mut() {
            // Skip empty lines and comments
            if line.trim().is_empty() || line.trim_start().starts_with("//") {
                continue;
            }

            // Handle colon spacing, but avoid URLs and other non-schema contexts
            if line.contains(':') {
                // Preserve leading whitespace
                let leading_whitespace = line.len() - line.trim_start().len();
                let leading_spaces = &line[..leading_whitespace];
                let content = line.trim_start();

                // Skip if this looks like a URL (contains ://)
                if content.contains("://") {
                    continue;
                }

                // Process colon spacing, but avoid colons inside quoted strings
                let mut result = String::new();
                let chars: Vec<char> = content.chars().collect();
                let mut i = 0;
                let mut in_quotes = false;
                let mut quote_char = '"';

                while i < chars.len() {
                    let ch = chars[i];

                    // Track if we're inside quotes
                    if (ch == '"' || ch == '\'') && (i == 0 || chars[i - 1] != '\\') {
                        if !in_quotes {
                            in_quotes = true;
                            quote_char = ch;
                        } else if ch == quote_char {
                            in_quotes = false;
                        }
                    }

                    if ch == ':' && !in_quotes {
                        // Remove any spaces before the colon
                        while !result.is_empty() && result.chars().last() == Some(' ') {
                            result.pop();
                        }

                        // Add the colon
                        result.push(':');

                        // Skip any existing spaces after the colon
                        i += 1;
                        while i < chars.len() && chars[i] == ' ' {
                            i += 1;
                        }

                        // Add exactly one space after the colon (if there's content after)
                        if i < chars.len() {
                            result.push(' ');
                        }
                    } else {
                        result.push(ch);
                        i += 1;
                    }
                }

                *line = format!("{leading_spaces}{result}");
            }
        }
    }

    /// Normalize schema properties to multiline format and fix description spacing
    fn normalize_schema_properties(&self, lines: &mut Vec<String>) {
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i].clone();

            // Skip empty lines and comments
            if line.trim().is_empty() || line.trim_start().starts_with("//") {
                i += 1;
                continue;
            }

            // Look for schema blocks: output <- { properties }
            if let Some(schema_info) = self.parse_schema_block(line) {
                if self.contains_multiple_schema_properties(&schema_info.properties_content) {
                    let leading_whitespace = line.len() - line.trim_start().len();
                    let leading_spaces = &line[..leading_whitespace];

                    // Split properties
                    let properties = self.split_schema_properties(&schema_info.properties_content);

                    if properties.len() > 1 {
                        // Create multiline block structure
                        let mut new_lines = Vec::new();

                        // Opening line: output <- {
                        new_lines.push(format!("{}{} <- {{", leading_spaces, schema_info.prefix));

                        // Properties with additional indentation
                        for property in properties {
                            new_lines.push(format!("{}    {}", leading_spaces, property));
                        }

                        // Closing brace
                        new_lines.push(format!("{}}}", leading_spaces));

                        // Replace current line with new multiline structure
                        lines.splice(i..=i, new_lines.clone());
                        i += new_lines.len();
                        continue;
                    }
                }
            }

            // Look for individual property lines that need description spacing fixes
            if self.contains_multiple_schema_properties(line) {
                let leading_whitespace = line.len() - line.trim_start().len();
                let leading_spaces = &line[..leading_whitespace];
                let content = line.trim_start();

                // Split properties and convert to multiline
                let properties = self.split_schema_properties(content);

                if properties.len() > 1 {
                    // Replace the current line with the first property
                    lines[i] = format!("{}{}", leading_spaces, properties[0]);

                    // Insert additional properties as new lines
                    for (idx, property) in properties.iter().skip(1).enumerate() {
                        lines.insert(i + 1 + idx, format!("{}{}", leading_spaces, property));
                    }

                    // Skip the newly inserted lines
                    i += properties.len();
                } else {
                    // Single property, just fix description spacing
                    lines[i] = format!("{}{}", leading_spaces, self.fix_description_spacing(content));
                    i += 1;
                }
            } else {
                // Check for description spacing issues on single properties
                let leading_whitespace = line.len() - line.trim_start().len();
                let leading_spaces = &line[..leading_whitespace];
                let content = line.trim_start();

                if self.has_description_spacing_issue(content) {
                    lines[i] = format!("{}{}", leading_spaces, self.fix_description_spacing(content));
                }
                i += 1;
            }
        }
    }

    /// Parse a schema block line and extract components
    fn parse_schema_block(&self, line: &str) -> Option<SchemaBlockInfo> {
        let content = line.trim_start();

        // Look for pattern: prefix <- { properties }
        if let Some(arrow_pos) = content.find("<-") {
            let prefix = content[..arrow_pos].trim();
            let after_arrow = content[arrow_pos + 2..].trim();

            if after_arrow.starts_with('{') && after_arrow.ends_with('}') {
                let properties_content = &after_arrow[1..after_arrow.len()-1].trim();

                return Some(SchemaBlockInfo {
                    prefix: prefix.to_string(),
                    properties_content: properties_content.to_string(),
                });
            }
        }

        None
    }

    /// Check if a line contains multiple schema properties
    fn contains_multiple_schema_properties(&self, line: &str) -> bool {
        let content = line.trim_start();

        // Skip if this doesn't look like a schema line
        if !content.contains(':') {
            return false;
        }

        // Count property patterns: word: type (optionally followed by description)
        // Look for pattern: identifier: type [description] identifier: type [description]
        let mut property_count = 0;
        let mut in_quotes = false;
        let mut quote_char = '"';
        let chars: Vec<char> = content.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let ch = chars[i];

            // Track quotes
            if (ch == '"' || ch == '\'') && (i == 0 || chars[i - 1] != '\\') {
                if !in_quotes {
                    in_quotes = true;
                    quote_char = ch;
                } else if ch == quote_char {
                    in_quotes = false;
                }
            }

            // Look for property pattern: identifier:
            if !in_quotes && ch.is_alphabetic() {
                // Found start of potential identifier
                let _start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }

                // Skip whitespace
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }

                // Check if followed by colon
                if i < chars.len() && chars[i] == ':' {
                    // Skip if this is part of a URL (://)
                    if i + 2 < chars.len() && chars[i + 1] == '/' && chars[i + 2] == '/' {
                        i += 3;
                        continue;
                    }

                    property_count += 1;
                    i += 1; // Skip the colon

                    // Skip the type and optional description to find next property
                    while i < chars.len() {
                        let ch = chars[i];

                        // Track quotes in type/description
                        if (ch == '"' || ch == '\'') && (i == 0 || chars[i - 1] != '\\') {
                            if !in_quotes {
                                in_quotes = true;
                                quote_char = ch;
                            } else if ch == quote_char {
                                in_quotes = false;
                            }
                        }

                        // If we're not in quotes and find a letter that could start next property
                        if !in_quotes && ch.is_alphabetic() {
                            // Look ahead to see if this is identifier:
                            let mut j = i;
                            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                                j += 1;
                            }
                            while j < chars.len() && chars[j] == ' ' {
                                j += 1;
                            }
                            if j < chars.len() && chars[j] == ':' {
                                // This is the start of next property
                                break;
                            }
                        }

                        i += 1;
                    }
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        property_count > 1
    }

    /// Split schema properties into individual lines
    fn split_schema_properties(&self, content: &str) -> Vec<String> {
        let mut properties = Vec::new();
        let mut current_property = String::new();
        let mut in_quotes = false;
        let mut quote_char = '"';
        let mut brace_depth = 0;
        let chars: Vec<char> = content.chars().collect();

        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];

            // Track quotes
            if (ch == '"' || ch == '\'') && (i == 0 || chars[i - 1] != '\\') {
                if !in_quotes {
                    in_quotes = true;
                    quote_char = ch;
                } else if ch == quote_char {
                    in_quotes = false;
                }
            }

            // Track braces for nested objects
            if !in_quotes {
                if ch == '{' {
                    brace_depth += 1;
                } else if ch == '}' {
                    brace_depth -= 1;
                }
            }

            current_property.push(ch);

            // Look for property boundaries (space followed by identifier:)
            if !in_quotes && brace_depth == 0 && ch == ' ' {
                // Look ahead to see if this is a property boundary
                let mut j = i + 1;
                while j < chars.len() && chars[j] == ' ' {
                    j += 1;
                }

                // Check if we have identifier: pattern
                if j < chars.len() && chars[j].is_alphabetic() {
                    let mut k = j;
                    while k < chars.len() && (chars[k].is_alphanumeric() || chars[k] == '_') {
                        k += 1;
                    }
                    if k < chars.len() && chars[k] == ':' {
                        // This is a property boundary
                        properties.push(self.fix_description_spacing(&current_property.trim()));
                        current_property.clear();
                        i = j - 1; // Will be incremented at end of loop
                    }
                }
            }

            i += 1;
        }

        // Add the last property
        if !current_property.trim().is_empty() {
            properties.push(self.fix_description_spacing(&current_property.trim()));
        }

        properties
    }

    /// Check if content has description spacing issues
    fn has_description_spacing_issue(&self, content: &str) -> bool {
        // Look for patterns where quotes immediately follow types without space
        content.contains("]\"") || content.contains("]'") ||
        content.contains("string\"") || content.contains("string'") ||
        content.contains("number\"") || content.contains("number'") ||
        content.contains("boolean\"") || content.contains("boolean'")
    }

    /// Fix spacing between type and description
    fn fix_description_spacing(&self, content: &str) -> String {
        let mut result = content.to_string();

        // Don't modify content inside quoted strings
        if content.contains("\"") && (content.starts_with("\"") || content.contains(" \"")) {
            // This looks like it contains quoted string content, be more careful
            // Only apply fixes to patterns that are clearly type declarations
            if content.contains(": ") {
                // Fix ]"comment" -> ] "comment"
                result = result.replace("]\"", "] \"");
                result = result.replace("]'", "] '");

                // Fix type"comment" -> type "comment" for common types
                // Only when they appear after a colon (type declaration context)
                result = result.replace(": string\"", ": string \"");
                result = result.replace(": string'", ": string '");
                result = result.replace(": number\"", ": number \"");
                result = result.replace(": number'", ": number '");
                result = result.replace(": boolean\"", ": boolean \"");
                result = result.replace(": boolean'", ": boolean '");
            }
        } else {
            // No quoted strings, safe to apply all fixes
            // Fix ]"comment" -> ] "comment"
            result = result.replace("]\"", "] \"");
            result = result.replace("]'", "] '");

            // Fix type"comment" -> type "comment" for common types
            result = result.replace("string\"", "string \"");
            result = result.replace("string'", "string '");
            result = result.replace("number\"", "number \"");
            result = result.replace("number'", "number '");
            result = result.replace("boolean\"", "boolean \"");
            result = result.replace("boolean'", "boolean '");
        }

        result
    }

    /// Convert long strings (>120 chars) to multiline format using triple quotes
    fn normalize_long_strings(&self, lines: &mut Vec<String>) {
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];

            // Skip empty lines and comments
            if line.trim().is_empty() || line.trim_start().starts_with("//") {
                i += 1;
                continue;
            }

            // Check if line is longer than 120 characters
            if line.len() > 120 {
                if let Some(converted) = self.convert_long_string_to_multiline(line) {
                    // Store the length before moving converted
                    let converted_len = converted.len();

                    // Replace the current line with the multiline version
                    lines.remove(i);
                    for (j, new_line) in converted.into_iter().enumerate() {
                        lines.insert(i + j, new_line);
                    }
                    // Skip past the newly inserted lines
                    i += converted_len;
                    continue;
                }
            }
            i += 1;
        }
    }

    /// Convert a line with a long string to multiline format
    fn convert_long_string_to_multiline(&self, line: &str) -> Option<Vec<String>> {
        // Preserve leading whitespace
        let leading_whitespace = line.len() - line.trim_start().len();
        let leading_spaces = &line[..leading_whitespace];
        let content = line.trim_start();

        // Look for string patterns: key <- "long string" or key: "long string"
        if let Some(quote_start) = content.find('"') {
            if let Some(quote_end) = content.rfind('"') {
                if quote_start != quote_end {
                    let before_string = &content[..quote_start];
                    let string_content = &content[quote_start + 1..quote_end];
                    let after_string = &content[quote_end + 1..];

                    // Only convert if the string itself is reasonably long (>80 chars)
                    // to avoid converting short strings on long lines
                    if string_content.len() > 80 {
                        let mut result = Vec::new();

                        // First line: key <- """
                        result.push(format!("{leading_spaces}{before_string}\"\"\""));

                        // Content lines (with additional indentation)
                        let content_indent = format!("{leading_spaces}    ");
                        result.push(format!("{content_indent}{string_content}"));

                        // Last line: """ + any trailing content
                        result.push(format!("{leading_spaces}\"\"\"{after_string}"));

                        return Some(result);
                    }
                }
            }
        }

        None
    }

    /// Normalize indentation of content inside existing triple quotes
    fn normalize_triple_quote_indentation(&self, lines: &mut Vec<String>) {
        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];

            // Look for opening triple quotes
            if line.trim_end().ends_with("\"\"\"") && !line.trim_start().starts_with("\"\"\"") {
                // Found opening triple quote line
                let leading_whitespace = line.len() - line.trim_start().len();
                let expected_content_indent = format!("{}{}", &line[..leading_whitespace], "    ");

                // Process content lines until closing triple quotes
                let mut j = i + 1;
                while j < lines.len() {
                    let content_line = &lines[j];

                    // Check if this is the closing triple quote line
                    if content_line.trim_start().starts_with("\"\"\"") {
                        break;
                    }

                    // Skip empty lines
                    if content_line.trim().is_empty() {
                        j += 1;
                        continue;
                    }

                    // Check if content line needs re-indentation
                    let current_content = content_line.trim_start();
                    let current_indent_len = content_line.len() - current_content.len();
                    let expected_indent_len = expected_content_indent.len();

                    // Only re-indent if the current indentation is different from expected
                    if current_indent_len != expected_indent_len {
                        lines[j] = format!("{}{}", expected_content_indent, current_content);
                    }

                    j += 1;
                }

                // Skip past the processed triple quote block
                i = j + 1;
            } else {
                i += 1;
            }
        }
    }

    /// Normalize blank lines (remove excessive blank lines)
    fn normalize_blank_lines(&self, lines: &mut Vec<String>) {
        let mut result = Vec::new();
        let mut consecutive_blank_lines = 0;

        for line in lines.iter() {
            if line.trim().is_empty() {
                consecutive_blank_lines += 1;
                // Allow maximum of 1 consecutive blank line
                if consecutive_blank_lines <= 1 {
                    result.push(line.clone());
                }
            } else {
                consecutive_blank_lines = 0;
                result.push(line.clone());
            }
        }

        // Remove trailing blank lines
        while let Some(last_line) = result.last() {
            if last_line.trim().is_empty() {
                result.pop();
            } else {
                break;
            }
        }

        *lines = result;
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}
