// src/types/patch.rs
use serde::{Deserialize, Serialize};

use crate::parse::ast::FileId;
use similar::TextDiff;

/// A patch that applies multiple edits (hunks) to a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilePatch {
    pub file_id: FileId,
    pub hunks: Vec<PatchHunk>,
}

/// A single edit operation inside a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchHunk {
    pub range: PatchRange,
    pub replacement: String,
}

/// Different ways to specify where a hunk applies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PatchRange {
    /// Insert at this byte offset; no bytes removed.
    InsertAt { byte_offset: usize },

    /// Replace the byte range [start, end) with `replacement`.
    ReplaceBytes { start: usize, end: usize },

    /// Insert a new snippet before the given (1-based) line number.
    InsertBeforeLine { line: u32 },

    /// Insert a new snippet after the given (1-based) line number.
    InsertAfterLine { line: u32 },
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

/// Extract the leading whitespace (indentation) from a line.
fn get_line_indentation(line: &str) -> &str {
    let non_ws = line.find(|c: char| !c.is_whitespace() || c == '\n');
    match non_ws {
        Some(idx) => &line[..idx],
        None => line, // All whitespace
    }
}

/// Apply indentation to each line of a multi-line string.
/// The first line gets no extra indentation (it's inserted at the right position).
/// Subsequent lines get the specified indentation prepended.
fn apply_indentation(text: &str, indent: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return text.to_string();
    }

    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        // Apply indentation to all lines (including first for InsertBeforeLine)
        if !line.is_empty() {
            result.push_str(indent);
        }
        result.push_str(line);
    }

    // Preserve trailing newline if original had one
    if text.ends_with('\n') {
        result.push('\n');
    }

    result
}

/// Apply a `FilePatch` to an in-memory file contents and return the new contents.
///
/// This works in byte offsets (not chars) and:
/// - converts line-based ranges to byte offsets,
/// - applies hunks from right to left so earlier ranges don't shift later ones.
/// - For `InsertBeforeLine`, automatically detects and applies the indentation of the target line.
pub fn apply_file_patch(original: &str, patch: &FilePatch) -> String {
    let mut text = original.to_string();

    // Precompute line-start byte offsets for the *original* text.
    // line 1 -> offset 0, line 2 -> after first '\n', etc.
    let mut line_starts: Vec<usize> = Vec::new();
    line_starts.push(0);
    for (idx, ch) in original.char_indices() {
        if ch == '\n' {
            line_starts.push(idx + 1);
        }
    }
    // A virtual "line after last" to simplify InsertAfterLine on final line.
    line_starts.push(original.len());

    // Normalize all hunks to (start, end, replacement) in byte offsets.
    // For inserts, start == end.
    let mut normalized: Vec<(usize, usize, String)> = Vec::new();

    for h in &patch.hunks {
        match &h.range {
            PatchRange::InsertAt { byte_offset } => {
                let off = (*byte_offset).min(text.len());
                normalized.push((off, off, h.replacement.clone()));
            }

            PatchRange::ReplaceBytes { start, end } => {
                let s = (*start).min(text.len());
                let e = (*end).min(text.len());
                if s <= e {
                    normalized.push((s, e, h.replacement.clone()));
                }
            }

            PatchRange::InsertBeforeLine { line } => {
                // 1-based lines; clamp into our table.
                let idx = line.saturating_sub(1) as usize;
                let off = *line_starts.get(idx).unwrap_or(&text.len());

                // Detect indentation of the target line and apply it to the replacement
                let target_line_end = line_starts.get(idx + 1).copied().unwrap_or(original.len());
                let target_line = &original[off..target_line_end];
                let indent = get_line_indentation(target_line);
                let indented_replacement = apply_indentation(&h.replacement, indent);

                normalized.push((off, off, indented_replacement));
            }

            PatchRange::InsertAfterLine { line } => {
                // line == 0 means "insert at the very beginning of the file"
                if *line == 0 {
                    normalized.push((0, 0, h.replacement.clone()));
                } else {
                    let idx = line.saturating_sub(1) as usize;

                    // Start of the line.
                    let line_start = *line_starts.get(idx).unwrap_or(&text.len());

                    // Find end-of-line: next '\n' or end of text.
                    let end_of_line = original[line_start..]
                        .find('\n')
                        .map(|rel| line_start + rel + 1) // after '\n'
                        .unwrap_or(text.len());

                    // Detect indentation of the current line and apply it to the replacement
                    let current_line = &original[line_start..end_of_line];
                    let indent = get_line_indentation(current_line);
                    let indented_replacement = apply_indentation(&h.replacement, indent);

                    normalized.push((end_of_line, end_of_line, indented_replacement));
                }
            }
        }
    }

    // Apply from right to left so earlier edits don't affect later offsets.
    normalized.sort_by_key(|(start, _end, _)| *start);
    for (start, end, repl) in normalized.into_iter().rev() {
        if start > end || end > text.len() {
            continue; // skip invalid range defensively
        }
        text.replace_range(start..end, &repl);
    }

    text
}

/// Build a unified diff string between `before` and `after` using `similar` 2.7.
pub fn make_unified_diff(path: &str, before: &str, after: &str) -> String {
    TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(path, path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== FilePatch Tests ====================

    #[test]
    fn file_patch_debug_impl() {
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![],
        };
        let debug_str = format!("{:?}", patch);
        assert!(debug_str.contains("FilePatch"));
    }

    #[test]
    fn patch_hunk_debug_impl() {
        let hunk = PatchHunk {
            range: PatchRange::InsertAt { byte_offset: 0 },
            replacement: "test".to_string(),
        };
        let debug_str = format!("{:?}", hunk);
        assert!(debug_str.contains("PatchHunk"));
    }

    // ==================== PatchRange Tests ====================

    #[test]
    fn patch_range_insert_at_debug() {
        let range = PatchRange::InsertAt { byte_offset: 10 };
        let debug_str = format!("{:?}", range);
        assert!(debug_str.contains("InsertAt"));
    }

    #[test]
    fn patch_range_replace_bytes_debug() {
        let range = PatchRange::ReplaceBytes { start: 0, end: 10 };
        let debug_str = format!("{:?}", range);
        assert!(debug_str.contains("ReplaceBytes"));
    }

    #[test]
    fn patch_range_insert_before_line_debug() {
        let range = PatchRange::InsertBeforeLine { line: 5 };
        let debug_str = format!("{:?}", range);
        assert!(debug_str.contains("InsertBeforeLine"));
    }

    #[test]
    fn patch_range_insert_after_line_debug() {
        let range = PatchRange::InsertAfterLine { line: 5 };
        let debug_str = format!("{:?}", range);
        assert!(debug_str.contains("InsertAfterLine"));
    }

    // ==================== apply_file_patch Tests ====================

    #[test]
    fn apply_file_patch_insert_at_beginning() {
        let original = "hello world";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAt { byte_offset: 0 },
                replacement: "prefix ".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "prefix hello world");
    }

    #[test]
    fn apply_file_patch_insert_at_middle() {
        let original = "hello world";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAt { byte_offset: 5 },
                replacement: " there".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "hello there world");
    }

    #[test]
    fn apply_file_patch_insert_at_end() {
        let original = "hello world";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAt { byte_offset: 11 },
                replacement: "!".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "hello world!");
    }

    #[test]
    fn apply_file_patch_insert_at_beyond_end() {
        let original = "hello";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAt { byte_offset: 100 },
                replacement: " world".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        // Should clamp to end of text
        assert_eq!(result, "hello world");
    }

    #[test]
    fn apply_file_patch_replace_bytes() {
        let original = "hello world";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::ReplaceBytes { start: 0, end: 5 },
                replacement: "hi".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "hi world");
    }

    #[test]
    fn apply_file_patch_replace_bytes_beyond_end() {
        let original = "hello";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::ReplaceBytes { start: 0, end: 100 },
                replacement: "hi".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        // Should clamp end to text length
        assert_eq!(result, "hi");
    }

    #[test]
    fn apply_file_patch_insert_before_line() {
        let original = "line1\nline2\nline3";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertBeforeLine { line: 2 },
                replacement: "inserted\n".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "line1\ninserted\nline2\nline3");
    }

    #[test]
    fn apply_file_patch_insert_after_line() {
        let original = "line1\nline2\nline3";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAfterLine { line: 1 },
                replacement: "inserted\n".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "line1\ninserted\nline2\nline3");
    }

    #[test]
    fn apply_file_patch_insert_after_last_line_no_newline() {
        let original = "line1\nline2";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAfterLine { line: 2 },
                replacement: "\nline3".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn apply_file_patch_multiple_hunks() {
        let original = "aaa bbb ccc";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![
                PatchHunk {
                    range: PatchRange::ReplaceBytes { start: 0, end: 3 },
                    replacement: "AAA".to_string(),
                },
                PatchHunk {
                    range: PatchRange::ReplaceBytes { start: 8, end: 11 },
                    replacement: "CCC".to_string(),
                },
            ],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "AAA bbb CCC");
    }

    #[test]
    fn apply_file_patch_empty_original() {
        let original = "";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAt { byte_offset: 0 },
                replacement: "new content".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "new content");
    }

    #[test]
    fn apply_file_patch_empty_hunks() {
        let original = "hello world";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(result, "hello world");
    }

    // ==================== make_unified_diff Tests ====================

    #[test]
    fn make_unified_diff_no_changes() {
        let content = "line1\nline2\nline3\n";
        let diff = make_unified_diff("test.py", content, content);
        // No changes means empty diff (or just header)
        assert!(!diff.contains("@@"));
    }

    #[test]
    fn make_unified_diff_with_changes() {
        let before = "line1\nline2\nline3\n";
        let after = "line1\nmodified\nline3\n";
        let diff = make_unified_diff("test.py", before, after);
        assert!(diff.contains("@@"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn make_unified_diff_includes_path() {
        let before = "a\n";
        let after = "b\n";
        let diff = make_unified_diff("my/path/file.py", before, after);
        assert!(diff.contains("my/path/file.py"));
    }

    // ==================== Indentation Tests ====================

    #[test]
    fn get_line_indentation_spaces() {
        assert_eq!(get_line_indentation("    code"), "    ");
        assert_eq!(get_line_indentation("  code"), "  ");
        assert_eq!(get_line_indentation("code"), "");
    }

    #[test]
    fn get_line_indentation_tabs() {
        assert_eq!(get_line_indentation("\t\tcode"), "\t\t");
        assert_eq!(get_line_indentation("\tcode"), "\t");
    }

    #[test]
    fn get_line_indentation_mixed() {
        assert_eq!(get_line_indentation("  \t  code"), "  \t  ");
    }

    #[test]
    fn apply_indentation_single_line() {
        let result = apply_indentation("comment", "    ");
        assert_eq!(result, "    comment");
    }

    #[test]
    fn apply_indentation_multi_line() {
        let result = apply_indentation("line1\nline2\nline3", "    ");
        assert_eq!(result, "    line1\n    line2\n    line3");
    }

    #[test]
    fn apply_indentation_with_trailing_newline() {
        let result = apply_indentation("line1\nline2\n", "  ");
        assert_eq!(result, "  line1\n  line2\n");
    }

    #[test]
    fn apply_indentation_empty_lines() {
        let result = apply_indentation("line1\n\nline3", "  ");
        assert_eq!(result, "  line1\n\n  line3");
    }

    #[test]
    fn apply_file_patch_insert_before_line_with_indentation() {
        // Simulate typical code with indentation
        let original = "function test() {\n    const x = 1;\n    return x;\n}";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertBeforeLine { line: 3 },
                replacement: "// comment\n".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        // The comment should be indented to match "    return x;"
        assert_eq!(
            result,
            "function test() {\n    const x = 1;\n    // comment\n    return x;\n}"
        );
    }

    #[test]
    fn apply_file_patch_insert_before_line_multi_line_comment() {
        let original = "class Test {\n    method() {\n        doSomething();\n    }\n}";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertBeforeLine { line: 3 },
                replacement: "// Line 1\n// Line 2\n".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        // Both comment lines should get 8-space indentation
        assert_eq!(
            result,
            "class Test {\n    method() {\n        // Line 1\n        // Line 2\n        doSomething();\n    }\n}"
        );
    }

    #[test]
    fn apply_file_patch_insert_after_line_with_indentation() {
        let original = "function test() {\n    const x = 1;\n    return x;\n}";
        let patch = FilePatch {
            file_id: FileId(1),
            hunks: vec![PatchHunk {
                range: PatchRange::InsertAfterLine { line: 2 },
                replacement: "// added after\n".to_string(),
            }],
        };
        let result = apply_file_patch(original, &patch);
        assert_eq!(
            result,
            "function test() {\n    const x = 1;\n    // added after\n    return x;\n}"
        );
    }
}
