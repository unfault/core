use serde::{Deserialize, Serialize};

use crate::parse::ast::ParsedFile;
use crate::semantics::typescript::model::{TsAsyncOperation, TsAsyncOperationType};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TsAsyncSummary {
    pub operations: Vec<TsAsyncOperation>,
    pub promises: Vec<TsAsyncOperation>,
    pub awaits: Vec<TsAsyncOperation>,
    pub promise_combinators: Vec<TsAsyncOperation>,
    pub promise_chains: Vec<TsAsyncOperation>,
    pub timeouts: Vec<TsAsyncOperation>,
    pub cancellations: Vec<TsAsyncOperation>,
    pub without_error_handling: Vec<TsAsyncOperation>,
    pub without_timeout: Vec<TsAsyncOperation>,
}

pub fn summarize_ts_async_operations(parsed: &ParsedFile) -> TsAsyncSummary {
    let mut summary = TsAsyncSummary::default();

    let root = parsed.tree.root_node();
    walk_for_async(root, parsed, &mut summary, None);

    summary.without_error_handling = summary
        .operations
        .iter()
        .filter(|op| !op.has_error_handling)
        .cloned()
        .collect();

    summary.without_timeout = summary
        .operations
        .iter()
        .filter(|op| !op.has_timeout && op.operation_type == TsAsyncOperationType::Await)
        .cloned()
        .collect();

    summary
}

fn walk_for_async(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    summary: &mut TsAsyncSummary,
    current_function: Option<&str>,
) {
    let func_name = node
        .child_by_field_name("name")
        .map(|n| parsed.text_for_node(&n));
    let effective_function = func_name.as_deref().or(current_function);

    match node.kind() {
        "call_expression" => {
            if let Some(op) = detect_ts_async_call(parsed, &node, effective_function) {
                summary.operations.push(op.clone());
                match op.operation_type {
                    TsAsyncOperationType::PromiseConstructor => summary.promises.push(op),
                    TsAsyncOperationType::Await => summary.awaits.push(op),
                    TsAsyncOperationType::PromiseCombinator => summary.promise_combinators.push(op),
                    TsAsyncOperationType::PromiseChain => summary.promise_chains.push(op),
                    TsAsyncOperationType::Timeout => summary.timeouts.push(op),
                    TsAsyncOperationType::Cancellation => summary.cancellations.push(op),
                    _ => {}
                }
            }
        }
        "await_expression" => {
            if let Some(op) = detect_ts_await(parsed, &node, effective_function) {
                summary.operations.push(op.clone());
                summary.awaits.push(op);
            }
        }
        "new_expression" => {
            if let Some(op) = detect_promise_constructor(parsed, &node, effective_function) {
                summary.operations.push(op.clone());
                summary.promises.push(op);
            }
        }
        _ => {}
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_async(child, parsed, summary, effective_function);
        }
    }
}

fn detect_ts_async_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    current_function: Option<&str>,
) -> Option<TsAsyncOperation> {
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);

    let operation_type = match callee.as_str() {
        "Promise.all" | "Promise.allSettled" | "Promise.race" | "Promise.any" => {
            TsAsyncOperationType::PromiseCombinator
        }
        "setTimeout" | "setInterval" | "setImmediate" => TsAsyncOperationType::Timeout,
        "AbortController" => TsAsyncOperationType::Cancellation,
        _ => {
            if callee.contains(".then(") || callee.contains(".catch(") || callee.contains(".finally(") {
                TsAsyncOperationType::PromiseChain
            } else {
                return None;
            }
        }
    };

    let text = parsed.text_for_node(node);
    let has_error_handling = has_try_catch_around(node) || text.contains(".catch(");
    let (has_timeout, timeout_value) = extract_timeout_from_args(parsed, node);
    let has_cancellation = text.contains("AbortController") || text.contains("signal");

    Some(TsAsyncOperation {
        operation_type,
        has_error_handling,
        has_timeout,
        timeout_value,
        has_cancellation,
        operation_text: text,
        enclosing_function: current_function.map(|s| s.to_string()),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

fn detect_ts_await(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    current_function: Option<&str>,
) -> Option<TsAsyncOperation> {
    let text = parsed.text_for_node(node);
    let has_error_handling = has_try_catch_around(node);

    Some(TsAsyncOperation {
        operation_type: TsAsyncOperationType::Await,
        has_error_handling,
        has_timeout: false,
        timeout_value: None,
        has_cancellation: false,
        operation_text: text,
        enclosing_function: current_function.map(|s| s.to_string()),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

fn detect_promise_constructor(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    current_function: Option<&str>,
) -> Option<TsAsyncOperation> {
    let constructor_node = node.child_by_field_name("constructor")?;
    let constructor_name = parsed.text_for_node(&constructor_node);

    if constructor_name != "Promise" {
        return None;
    }

    let text = parsed.text_for_node(node);
    let has_error_handling = text.contains("catch") || has_try_catch_around(node);

    Some(TsAsyncOperation {
        operation_type: TsAsyncOperationType::PromiseConstructor,
        has_error_handling,
        has_timeout: false,
        timeout_value: None,
        has_cancellation: false,
        operation_text: text,
        enclosing_function: current_function.map(|s| s.to_string()),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

fn has_try_catch_around(node: &tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "try_statement" {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn extract_timeout_from_args(parsed: &ParsedFile, node: &tree_sitter::Node) -> (bool, Option<f64>) {
    if let Some(args_node) = node.child_by_field_name("arguments") {
        let text = parsed.text_for_node(&args_node);

        if let Some(timeout_idx) = text.find("timeout") {
            let before_timeout = &text[..timeout_idx.saturating_sub(50)];
            if let Some(number_start) = before_timeout.chars().rev().position(|c| c.is_ascii_digit() || c == '.') {
                let start = timeout_idx - number_start;
                let number: String = text[start..]
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if let Ok(value) = number.parse::<f64>() {
                    let timeout_seconds = value / 1000.0;
                    return (true, Some(timeout_seconds));
                }
            }
        }
    }
    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize(source: &str) -> TsAsyncSummary {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_ts_async_operations(&parsed)
    }

    #[test]
    fn detects_promise_all() {
        let src = r#"
async function main() {
    const results = await Promise.all([fetch1, fetch2]);
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.promise_combinators.len(), 1);
        assert_eq!(
            summary.promise_combinators[0].operation_type,
            TsAsyncOperationType::PromiseCombinator
        );
    }

    #[test]
    fn detects_promise_all_settled() {
        let src = r#"
async function main() {
    const results = await Promise.allSettled([fetch1, fetch2]);
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.promise_combinators.len(), 1);
    }

    #[test]
    fn detects_promise_race() {
        let src = r#"
async function main() {
    const result = await Promise.race([fetch1, fetch2]);
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.promise_combinators.len(), 1);
    }

    #[test]
    fn detects_await_expression() {
        let src = r#"
async function main() {
    const data = await fetchData();
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.awaits.len(), 1);
        assert_eq!(summary.awaits[0].operation_type, TsAsyncOperationType::Await);
    }

    #[test]
    fn detects_promise_constructor() {
        let src = r#"
async function main() {
    const promise = new Promise((resolve, reject) => {
        resolve(42);
    });
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.promises.len(), 1);
        assert_eq!(
            summary.promises[0].operation_type,
            TsAsyncOperationType::PromiseConstructor
        );
    }

    #[test]
    fn detects_set_timeout() {
        let src = r#"
function main() {
    setTimeout(() => {
        console.log('delayed');
    }, 1000);
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.timeouts.len(), 1);
        assert_eq!(summary.timeouts[0].operation_type, TsAsyncOperationType::Timeout);
    }

    #[test]
    fn detects_promise_chain() {
        let src = r#"
async function main() {
    const result = fetchData()
        .then(data => process(data))
        .catch(error => handleError(error));
}
"#;
        let summary = parse_and_summarize(src);
        assert!(!summary.promise_chains.is_empty());
    }

    #[test]
    fn async_operation_without_error_handling() {
        let src = r#"
async function main() {
    const data = await fetchData();
}
"#;
        let summary = parse_and_summarize(src);
        assert!(!summary.without_error_handling.is_empty());
    }

    #[test]
    fn async_operation_with_error_handling() {
        let src = r#"
async function main() {
    try {
        const data = await fetchData();
    } catch (e) {
        handleError(e);
    }
}
"#;
        let summary = parse_and_summarize(src);
        let await_op = summary.awaits.first();
        assert!(await_op.is_some());
        assert!(await_op.unwrap().has_error_handling);
    }

    #[test]
    fn empty_file_has_no_operations() {
        let summary = parse_and_summarize("");
        assert!(summary.operations.is_empty());
    }

    #[test]
    fn sync_code_has_no_operations() {
        let src = r#"
function main() {
    const result = syncFunction();
}
"#;
        let summary = parse_and_summarize(src);
        assert!(summary.operations.is_empty());
    }

    #[test]
    fn detects_multiple_async_operations() {
        let src = r#"
async function main() {
    const p1 = new Promise((resolve) => resolve(1));
    const p2 = fetchData();
    const result = await Promise.all([p1, p2]);
    const data = await otherFetch();
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.promises.len(), 1);
        assert_eq!(summary.promise_combinators.len(), 1);
        assert_eq!(summary.awaits.len(), 2);
    }

    #[test]
    fn async_in_arrow_function() {
        let src = r#"
const fetchData = async () => {
    const result = await api.call();
    return result;
};
"#;
        let summary = parse_and_summarize(src);
        let await_op = summary.awaits.first();
        assert!(await_op.is_some());
    }

    #[test]
    fn async_in_class_method() {
        let src = r#"
class MyService {
    async fetchData() {
        const result = await this.api.call();
        return result;
    }
}
"#;
        let summary = parse_and_summarize(src);
        let await_op = summary.awaits.first();
        assert!(await_op.is_some());
    }

    #[test]
    fn async_in_for_loop() {
        let src = r#"
async function processItems() {
    for (const item of items) {
        await process(item);
    }
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.awaits.len(), 1);
    }

    #[test]
    fn nested_async_operations() {
        let src = r#"
async function outer() {
    async function inner() {
        await step1();
        await step2();
    }
    await inner();
}
"#;
        let summary = parse_and_summarize(src);
        assert!(summary.awaits.len() >= 3);
    }

    #[test]
    fn promise_any_detection() {
        let src = r#"
async function main() {
    const result = await Promise.any([fetch1, fetch2]);
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.promise_combinators.len(), 1);
    }

    #[test]
    fn set_interval_detection() {
        let src = r#"
function startPolling() {
    const interval = setInterval(() => {
        checkStatus();
    }, 5000);
}
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.timeouts.len(), 1);
    }
}
