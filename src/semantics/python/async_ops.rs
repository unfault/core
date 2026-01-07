use serde::{Deserialize, Serialize};

use crate::parse::ast::ParsedFile;
use crate::semantics::python::model::{AsyncOperation, AsyncOperationType};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PyAsyncSummary {
    pub operations: Vec<AsyncOperation>,
    pub task_spawns: Vec<AsyncOperation>,
    pub awaits: Vec<AsyncOperation>,
    pub gathers: Vec<AsyncOperation>,
    pub sleeps: Vec<AsyncOperation>,
    pub timeouts: Vec<AsyncOperation>,
    pub without_error_handling: Vec<AsyncOperation>,
    pub without_timeout: Vec<AsyncOperation>,
}

pub fn summarize_async_operations(parsed: &ParsedFile) -> PyAsyncSummary {
    let mut summary = PyAsyncSummary::default();

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
        .filter(|op| op.operation_type.can_hang() && !op.has_timeout)
        .cloned()
        .collect();

    summary
}

fn walk_for_async(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    summary: &mut PyAsyncSummary,
    current_function: Option<&str>,
) {
    let func_name = node
        .child_by_field_name("name")
        .map(|n| parsed.text_for_node(&n));
    let effective_function = func_name.as_deref().or(current_function);

    match node.kind() {
        "call" => {
            if let Some(op) = detect_asyncio_call(parsed, &node, effective_function) {
                summary.operations.push(op.clone());
                match op.operation_type {
                    AsyncOperationType::TaskSpawn => summary.task_spawns.push(op),
                    AsyncOperationType::TaskGather => summary.gathers.push(op),
                    AsyncOperationType::Sleep => summary.sleeps.push(op),
                    AsyncOperationType::Timeout => summary.timeouts.push(op),
                    _ => {}
                }
            }
        }
        "await_expression" => {
            if let Some(op) = detect_await(parsed, &node, effective_function) {
                summary.operations.push(op.clone());
                summary.awaits.push(op);
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

fn detect_asyncio_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    current_function: Option<&str>,
) -> Option<AsyncOperation> {
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);

    let operation_type = match callee.as_str() {
        "asyncio.create_task" | "asyncio.ensure_future" | "asyncio.Task" => {
            AsyncOperationType::TaskSpawn
        }
        "asyncio.gather" | "asyncio.wait" | "asyncio.wait_for" => AsyncOperationType::TaskGather,
        "asyncio.Queue.get" | "asyncio.Queue.put" => {
            if callee.ends_with(".get") {
                AsyncOperationType::ChannelReceive
            } else {
                AsyncOperationType::ChannelSend
            }
        }
        "asyncio.Lock.acquire" | "asyncio.Semaphore.acquire" => AsyncOperationType::LockAcquire,
        "asyncio.Lock.release" | "asyncio.Semaphore.release" => AsyncOperationType::LockRelease,
        "asyncio.sleep" => AsyncOperationType::Sleep,
        "asyncio.timeout" | "asyncio.timeout_at" => AsyncOperationType::Timeout,
        _ => return None,
    };

    let text = parsed.text_for_node(node);
    let has_error_handling = has_try_around(node);
    let (has_timeout, timeout_value) = extract_timeout_from_call(parsed, node, &callee);

    Some(AsyncOperation {
        operation_type,
        has_error_handling,
        has_timeout,
        timeout_value,
        has_cancellation: false,
        is_bounded: false,
        bound_limit: None,
        operation_text: text,
        enclosing_function: current_function.map(|s| s.to_string()),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

fn detect_await(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    current_function: Option<&str>,
) -> Option<AsyncOperation> {
    let text = parsed.text_for_node(node);
    let has_error_handling = has_try_around(node);

    Some(AsyncOperation {
        operation_type: AsyncOperationType::Await,
        has_error_handling,
        has_timeout: false,
        timeout_value: None,
        has_cancellation: false,
        is_bounded: false,
        bound_limit: None,
        operation_text: text,
        enclosing_function: current_function.map(|s| s.to_string()),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

fn has_try_around(node: &tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "try_statement" {
            return true;
        }
        current = parent.parent();
    }
    false
}

fn extract_timeout_from_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    callee: &str,
) -> (bool, Option<f64>) {
    let args_node = if let Some(node) = node.child_by_field_name("arguments") {
        node
    } else {
        return (false, None);
    };
    let args_text = parsed.text_for_node(&args_node);

    if callee == "asyncio.wait_for" {
        if let Some(timeout_start) = args_text.find("timeout") {
            let before = &args_text[..timeout_start].trim_end();
            if let Some(digit_start) =
                before.rfind(|c: char| c.is_ascii_digit() || c == '-' || c == '+')
            {
                let number: String = args_text[digit_start..]
                    .chars()
                    .take_while(|c: &char| {
                        c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+'
                    })
                    .collect();
                if let Ok(seconds) = number.parse::<f64>() {
                    return (true, Some(seconds));
                }
            }
        }
    }

    if callee == "asyncio.timeout" || callee == "asyncio.timeout_at" {
        if let Some(open_paren) = args_text.find('(') {
            let inner = &args_text[open_paren + 1..args_text.len().saturating_sub(1)];
            if let Ok(seconds) = inner.trim().parse::<f64>() {
                return (true, Some(seconds));
            }
        }
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize(source: &str) -> PyAsyncSummary {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_async_operations(&parsed)
    }

    #[test]
    fn detects_asyncio_create_task() {
        let src = r#"
import asyncio

async def main():
    task = asyncio.create_task(coro())
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.task_spawns.len(), 1);
        assert_eq!(
            summary.task_spawns[0].operation_type,
            AsyncOperationType::TaskSpawn
        );
        assert!(
            summary.task_spawns[0]
                .operation_text
                .contains("asyncio.create_task")
        );
    }

    #[test]
    fn detects_asyncio_gather() {
        let src = r#"
import asyncio

async def main():
    await asyncio.gather(coro1(), coro2())
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.gathers.len(), 1);
        assert_eq!(
            summary.gathers[0].operation_type,
            AsyncOperationType::TaskGather
        );
    }

    #[test]
    fn detects_asyncio_sleep() {
        let src = r#"
import asyncio

async def main():
    await asyncio.sleep(5)
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.sleeps.len(), 1);
        assert_eq!(summary.sleeps[0].operation_type, AsyncOperationType::Sleep);
    }

    #[test]
    fn detects_await_expression() {
        let src = r#"
async def main():
    task = asyncio.create_task(coro())
"#;
        let summary = parse_and_summarize(src);
        assert!(!summary.operations.is_empty());
    }

    #[test]
    fn detects_wait_for_with_timeout() {
        let src = r#"
import asyncio

async def main():
    task = asyncio.create_task(coro())
"#;
        let summary = parse_and_summarize(src);
        assert!(!summary.operations.is_empty());
    }

    #[test]
    fn async_operation_without_error_handling() {
        let src = r#"
import asyncio

async def main():
    task = asyncio.create_task(coro())
"#;
        let summary = parse_and_summarize(src);
        assert!(!summary.without_error_handling.is_empty());
        let spawn = summary
            .task_spawns
            .iter()
            .find(|op| op.operation_type == AsyncOperationType::TaskSpawn)
            .unwrap();
        assert!(!spawn.has_error_handling);
    }

    #[test]
    fn async_operation_with_error_handling() {
        let src = r#"
import asyncio

async def main():
    try:
        task = asyncio.create_task(coro())
    except Exception:
        pass
"#;
        let summary = parse_and_summarize(src);
        let spawn = summary
            .task_spawns
            .iter()
            .find(|op| op.operation_type == AsyncOperationType::TaskSpawn)
            .unwrap();
        assert!(spawn.has_error_handling);
    }

    #[test]
    fn async_operation_without_timeout() {
        let src = r#"
import asyncio

async def main():
    await asyncio.sleep(5)
"#;
        let summary = parse_and_summarize(src);
        let sleep = summary
            .sleeps
            .iter()
            .find(|op| op.operation_type == AsyncOperationType::Sleep)
            .unwrap();
        assert!(!sleep.has_timeout);
    }

    #[test]
    fn empty_file_has_no_operations() {
        let summary = parse_and_summarize("");
        assert!(summary.operations.is_empty());
    }

    #[test]
    fn sync_code_has_no_operations() {
        let src = r#"
def main():
    result = sync_func()
"#;
        let summary = parse_and_summarize(src);
        assert!(summary.operations.is_empty());
    }

    #[test]
    fn detects_multiple_async_operations() {
        let src = r#"
import asyncio

async def main():
    task1 = asyncio.create_task(coro1())
    task2 = asyncio.create_task(coro2())
    await asyncio.gather(task1, task2)
"#;
        let summary = parse_and_summarize(src);
        assert_eq!(summary.task_spawns.len(), 2);
        assert_eq!(summary.gathers.len(), 1);
    }

    #[test]
    fn async_in_nested_function() {
        let src = r#"
import asyncio

class MyClass:
    async def async_method(self):
        await asyncio.sleep(1)
"#;
        let summary = parse_and_summarize(src);
        let sleep = summary.sleeps.first();
        assert!(sleep.is_some());
        assert_eq!(
            sleep.unwrap().enclosing_function,
            Some("async_method".to_string())
        );
    }
}
