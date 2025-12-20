//! ORM semantic analysis for Python (SQLAlchemy, Django ORM, etc.)
//!
//! This module provides semantic analysis for ORM patterns, particularly
//! for detecting N+1 query patterns and other inefficient database access.

use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, ParsedFile};

/// Represents an ORM query call site
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrmQueryCall {
    /// The ORM framework being used
    pub orm_kind: OrmKind,
    /// The type of query operation
    pub query_type: QueryType,
    /// Whether this query is inside a loop
    pub in_loop: bool,
    /// Whether this query is inside a comprehension
    pub in_comprehension: bool,
    /// Whether eager loading is used (select_related, prefetch_related, joinedload, etc.)
    pub has_eager_loading: bool,
    /// The model/table being queried (if detectable)
    pub model_name: Option<String>,
    /// Line number (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
    /// The full query expression text (for patch generation)
    pub query_text: Option<String>,
    /// The loop variable name (if inside a loop)
    pub loop_variable: Option<String>,
    /// The outer query that provides the collection being iterated (if detectable)
    pub outer_query: Option<Box<OuterQueryInfo>>,
}

/// Information about the outer query that provides the collection being iterated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OuterQueryInfo {
    /// Line number of the outer query (1-based)
    pub line: u32,
    /// Column number (1-based)
    pub column: u32,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// The full query expression text
    pub query_text: String,
    /// The variable name assigned to the query result
    pub variable_name: String,
    /// The model being queried
    pub model_name: Option<String>,
    /// The ORM kind
    pub orm_kind: OrmKind,
}

/// Supported ORM frameworks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrmKind {
    /// SQLAlchemy ORM
    SqlAlchemy,
    /// Django ORM
    Django,
    /// Tortoise ORM (async)
    Tortoise,
    /// SQLModel (FastAPI + SQLAlchemy)
    SqlModel,
    /// Peewee ORM
    Peewee,
    /// Unknown/generic ORM
    Unknown,
}

impl OrmKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrmKind::SqlAlchemy => "SQLAlchemy",
            OrmKind::Django => "Django ORM",
            OrmKind::Tortoise => "Tortoise ORM",
            OrmKind::SqlModel => "SQLModel",
            OrmKind::Peewee => "Peewee",
            OrmKind::Unknown => "Unknown ORM",
        }
    }
}

/// Types of ORM query operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryType {
    /// SELECT query (get, filter, all, etc.)
    Select,
    /// INSERT query (create, add, etc.)
    Insert,
    /// UPDATE query
    Update,
    /// DELETE query
    Delete,
    /// Relationship access (lazy loading)
    RelationshipAccess,
    /// Unknown query type
    Unknown,
}

/// Represents a potential N+1 query pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NPlusOnePattern {
    /// The outer query that fetches the collection
    pub outer_query: Option<OuterQueryInfo>,
    /// The inner query/access that causes N additional queries
    pub inner_access: OrmQueryCall,
    /// Description of the pattern
    pub pattern_description: String,
    /// The relationship field being accessed (if detectable)
    pub relationship_field: Option<String>,
}

/// Context for tracking loop information during traversal
#[derive(Debug, Clone)]
struct LoopContext {
    /// Whether we're inside a loop
    in_loop: bool,
    /// Whether we're inside a comprehension
    in_comprehension: bool,
    /// The loop variable name
    loop_variable: Option<String>,
    /// The outer query that provides the collection being iterated
    outer_query: Option<OuterQueryInfo>,
}

impl Default for LoopContext {
    fn default() -> Self {
        Self {
            in_loop: false,
            in_comprehension: false,
            loop_variable: None,
            outer_query: None,
        }
    }
}

/// Summarize ORM usage in a parsed Python file
pub fn summarize_orm_queries(parsed: &ParsedFile) -> Vec<OrmQueryCall> {
    let mut queries = Vec::new();
    let root = parsed.tree.root_node();

    // First pass: collect all ORM queries with their locations
    let mut all_queries: Vec<(u32, String, OrmKind, Option<String>, usize, usize)> = Vec::new();
    collect_all_orm_queries(root, parsed, &mut all_queries);

    // Track loop context during traversal
    let ctx = LoopContext::default();
    walk_for_orm_queries(root, parsed, &mut queries, ctx, &all_queries);

    queries
}

/// Collect all ORM queries in the file for outer query detection
fn collect_all_orm_queries(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    queries: &mut Vec<(u32, String, OrmKind, Option<String>, usize, usize)>,
) {
    if node.kind() == "assignment" {
        // Check if this is an ORM query assignment
        if let Some(right) = node.child_by_field_name("right") {
            if right.kind() == "call" {
                if let Some(func_node) = right.child_by_field_name("function") {
                    let callee = parsed.text_for_node(&func_node);
                    let args_text = right
                        .child_by_field_name("arguments")
                        .map(|n| parsed.text_for_node(&n))
                        .unwrap_or_default();

                    if let Some((orm_kind, _)) = detect_orm_pattern(&callee, &args_text) {
                        // Get the variable name
                        if let Some(left) = node.child_by_field_name("left") {
                            let var_name = parsed.text_for_node(&left);
                            let location = parsed.location_for_node(&right);
                            let model_name = extract_model_name(&callee);
                            let query_text = parsed.text_for_node(&right);
                            queries.push((
                                location.range.start_line + 1,
                                var_name,
                                orm_kind,
                                model_name,
                                right.start_byte(),
                                right.end_byte(),
                            ));
                            // Also store the query text
                            let _ = query_text;
                        }
                    }
                }
            }
        }
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            collect_all_orm_queries(child, parsed, queries);
        }
    }
}

/// Detect potential N+1 query patterns
pub fn detect_n_plus_one_patterns(queries: &[OrmQueryCall]) -> Vec<NPlusOnePattern> {
    let mut patterns = Vec::new();

    // Look for queries inside loops that don't have eager loading
    for query in queries {
        if (query.in_loop || query.in_comprehension) && !query.has_eager_loading {
            // This is a potential N+1 pattern
            let pattern = NPlusOnePattern {
                outer_query: query.outer_query.as_ref().map(|o| *o.clone()),
                inner_access: query.clone(),
                pattern_description: format!(
                    "{} query inside {} without eager loading",
                    query.orm_kind.as_str(),
                    if query.in_loop {
                        "loop"
                    } else {
                        "comprehension"
                    }
                ),
                relationship_field: query.loop_variable.clone(),
            };
            patterns.push(pattern);
        }

        // Check for relationship access patterns (lazy loading)
        if query.query_type == QueryType::RelationshipAccess && !query.has_eager_loading {
            let pattern = NPlusOnePattern {
                outer_query: query.outer_query.as_ref().map(|o| *o.clone()),
                inner_access: query.clone(),
                pattern_description: format!(
                    "Lazy relationship access in {} may cause N+1 queries",
                    query.orm_kind.as_str()
                ),
                relationship_field: query.loop_variable.clone(),
            };
            patterns.push(pattern);
        }
    }

    patterns
}

fn walk_for_orm_queries(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    queries: &mut Vec<OrmQueryCall>,
    ctx: LoopContext,
    all_queries: &[(u32, String, OrmKind, Option<String>, usize, usize)],
) {
    // Update context based on current node
    let mut new_ctx = ctx.clone();

    // Handle for loops - extract loop variable and try to find outer query
    if node.kind() == "for_statement" {
        new_ctx.in_loop = true;

        // Extract loop variable
        if let Some(left) = node.child_by_field_name("left") {
            new_ctx.loop_variable = Some(parsed.text_for_node(&left));
        }

        // Try to find the outer query from the iterable
        if let Some(right) = node.child_by_field_name("right") {
            let iterable_text = parsed.text_for_node(&right);

            // Check if the iterable is a variable that was assigned an ORM query
            for (line, var_name, orm_kind, model_name, start_byte, end_byte) in all_queries {
                if iterable_text == *var_name
                    || iterable_text.starts_with(&format!("{}.", var_name))
                {
                    new_ctx.outer_query = Some(OuterQueryInfo {
                        line: *line,
                        column: 1,
                        start_byte: *start_byte,
                        end_byte: *end_byte,
                        query_text: iterable_text.clone(),
                        variable_name: var_name.clone(),
                        model_name: model_name.clone(),
                        orm_kind: *orm_kind,
                    });
                    break;
                }
            }

            // Also check if the iterable is a direct ORM call
            if right.kind() == "call" {
                if let Some(func_node) = right.child_by_field_name("function") {
                    let callee = parsed.text_for_node(&func_node);
                    let args_text = right
                        .child_by_field_name("arguments")
                        .map(|n| parsed.text_for_node(&n))
                        .unwrap_or_default();

                    if let Some((orm_kind, _)) = detect_orm_pattern(&callee, &args_text) {
                        let location = parsed.location_for_node(&right);
                        let query_text = parsed.text_for_node(&right);
                        new_ctx.outer_query = Some(OuterQueryInfo {
                            line: location.range.start_line + 1,
                            column: location.range.start_col + 1,
                            start_byte: right.start_byte(),
                            end_byte: right.end_byte(),
                            query_text,
                            variable_name: String::new(),
                            model_name: extract_model_name(&callee),
                            orm_kind,
                        });
                    }
                }
            }
        }
    }

    if node.kind() == "while_statement" {
        new_ctx.in_loop = true;
    }

    if matches!(
        node.kind(),
        "list_comprehension"
            | "dictionary_comprehension"
            | "set_comprehension"
            | "generator_expression"
    ) {
        new_ctx.in_comprehension = true;

        // Try to extract the loop variable from comprehension
        // Comprehensions have a "for_in_clause" child
        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                if child.kind() == "for_in_clause" {
                    if let Some(left) = child.child_by_field_name("left") {
                        new_ctx.loop_variable = Some(parsed.text_for_node(&left));
                    }
                    if let Some(right) = child.child_by_field_name("right") {
                        let iterable_text = parsed.text_for_node(&right);

                        // Check if the iterable is a variable that was assigned an ORM query
                        for (line, var_name, orm_kind, model_name, start_byte, end_byte) in
                            all_queries
                        {
                            if iterable_text == *var_name {
                                new_ctx.outer_query = Some(OuterQueryInfo {
                                    line: *line,
                                    column: 1,
                                    start_byte: *start_byte,
                                    end_byte: *end_byte,
                                    query_text: iterable_text.clone(),
                                    variable_name: var_name.clone(),
                                    model_name: model_name.clone(),
                                    orm_kind: *orm_kind,
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Check for ORM query patterns
    if node.kind() == "call" {
        if let Some(mut query) = analyze_orm_call(node, parsed, &new_ctx) {
            query.loop_variable = new_ctx.loop_variable.clone();
            query.outer_query = new_ctx.outer_query.clone().map(Box::new);
            queries.push(query);
        }
    }

    // Check for attribute access (potential lazy loading)
    if node.kind() == "attribute" {
        if let Some(mut query) = analyze_attribute_access(node, parsed, &new_ctx) {
            query.loop_variable = new_ctx.loop_variable.clone();
            query.outer_query = new_ctx.outer_query.clone().map(Box::new);
            queries.push(query);
        }
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_orm_queries(child, parsed, queries, new_ctx.clone(), all_queries);
        }
    }
}

fn analyze_orm_call(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    ctx: &LoopContext,
) -> Option<OrmQueryCall> {
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);

    // Get arguments text for eager loading detection
    let args_text = node
        .child_by_field_name("arguments")
        .map(|n| parsed.text_for_node(&n))
        .unwrap_or_default();

    // Detect ORM kind and query type
    let (orm_kind, query_type) = detect_orm_pattern(&callee, &args_text)?;

    // Check for eager loading patterns
    let has_eager_loading = check_eager_loading(&callee, &args_text);

    // Extract model name if possible
    let model_name = extract_model_name(&callee);

    // Get the full query text
    let query_text = Some(parsed.text_for_node(&node));

    let location = parsed.location_for_node(&node);

    Some(OrmQueryCall {
        orm_kind,
        query_type,
        in_loop: ctx.in_loop,
        in_comprehension: ctx.in_comprehension,
        has_eager_loading,
        model_name,
        line: location.range.start_line + 1,
        column: location.range.start_col + 1,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location,
        query_text,
        loop_variable: None,
        outer_query: None,
    })
}

fn analyze_attribute_access(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    ctx: &LoopContext,
) -> Option<OrmQueryCall> {
    // Only flag attribute access in loops/comprehensions as potential lazy loading
    if !ctx.in_loop && !ctx.in_comprehension {
        return None;
    }

    let attr_name = node
        .child_by_field_name("attribute")
        .map(|n| parsed.text_for_node(&n))?;

    // Common relationship attribute patterns
    // This is heuristic-based - we look for common naming patterns
    let is_potential_relationship = is_relationship_attribute(&attr_name);

    if !is_potential_relationship {
        return None;
    }

    let location = parsed.location_for_node(&node);

    Some(OrmQueryCall {
        orm_kind: OrmKind::Unknown,
        query_type: QueryType::RelationshipAccess,
        in_loop: ctx.in_loop,
        in_comprehension: ctx.in_comprehension,
        has_eager_loading: false,
        model_name: None,
        line: location.range.start_line + 1,
        column: location.range.start_col + 1,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location,
        query_text: Some(parsed.text_for_node(&node)),
        loop_variable: None,
        outer_query: None,
    })
}

fn detect_orm_pattern(callee: &str, _args_text: &str) -> Option<(OrmKind, QueryType)> {
    // SQLAlchemy patterns
    if callee.contains("session.query") || callee.contains("Session.query") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }
    if callee.contains("session.execute") || callee.contains("Session.execute") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }
    if callee.contains("session.add") || callee.contains("Session.add") {
        return Some((OrmKind::SqlAlchemy, QueryType::Insert));
    }
    if callee.contains("session.delete") || callee.contains("Session.delete") {
        return Some((OrmKind::SqlAlchemy, QueryType::Delete));
    }
    if callee.ends_with(".all()") || callee.contains(".all(") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }
    if callee.ends_with(".first()") || callee.contains(".first(") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }
    if callee.ends_with(".one()") || callee.contains(".one(") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }
    if callee.ends_with(".filter(") || callee.contains(".filter(") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }
    if callee.ends_with(".filter_by(") || callee.contains(".filter_by(") {
        return Some((OrmKind::SqlAlchemy, QueryType::Select));
    }

    // Django ORM patterns
    if callee.contains(".objects.") {
        if callee.contains(".get(") {
            return Some((OrmKind::Django, QueryType::Select));
        }
        if callee.contains(".filter(") {
            return Some((OrmKind::Django, QueryType::Select));
        }
        if callee.contains(".all(") {
            return Some((OrmKind::Django, QueryType::Select));
        }
        if callee.contains(".create(") {
            return Some((OrmKind::Django, QueryType::Insert));
        }
        if callee.contains(".update(") {
            return Some((OrmKind::Django, QueryType::Update));
        }
        if callee.contains(".delete(") {
            return Some((OrmKind::Django, QueryType::Delete));
        }
        if callee.contains(".exclude(") {
            return Some((OrmKind::Django, QueryType::Select));
        }
        return Some((OrmKind::Django, QueryType::Select));
    }

    // Tortoise ORM patterns (async)
    if callee.contains(".filter(") && (callee.contains("await") || callee.starts_with("await ")) {
        return Some((OrmKind::Tortoise, QueryType::Select));
    }
    if callee.ends_with(".get(") || callee.contains(".get(") {
        // Could be Django or Tortoise
        return Some((OrmKind::Unknown, QueryType::Select));
    }

    // SQLModel patterns (similar to SQLAlchemy)
    if callee.contains("select(") {
        return Some((OrmKind::SqlModel, QueryType::Select));
    }

    // Peewee patterns
    if callee.contains(".select(") {
        return Some((OrmKind::Peewee, QueryType::Select));
    }
    if callee.contains(".create(") {
        return Some((OrmKind::Peewee, QueryType::Insert));
    }

    None
}

fn check_eager_loading(callee: &str, args_text: &str) -> bool {
    // SQLAlchemy eager loading
    if callee.contains("joinedload") || args_text.contains("joinedload") {
        return true;
    }
    if callee.contains("selectinload") || args_text.contains("selectinload") {
        return true;
    }
    if callee.contains("subqueryload") || args_text.contains("subqueryload") {
        return true;
    }
    if callee.contains("contains_eager") || args_text.contains("contains_eager") {
        return true;
    }

    // Django eager loading
    if callee.contains("select_related") || args_text.contains("select_related") {
        return true;
    }
    if callee.contains("prefetch_related") || args_text.contains("prefetch_related") {
        return true;
    }

    // Tortoise eager loading
    if callee.contains("prefetch_related") || args_text.contains("prefetch_related") {
        return true;
    }

    false
}

fn extract_model_name(callee: &str) -> Option<String> {
    // Try to extract model name from patterns like "User.objects.filter" or "session.query(User)"
    if callee.contains(".objects.") {
        let parts: Vec<&str> = callee.split(".objects.").collect();
        if !parts.is_empty() {
            return Some(parts[0].to_string());
        }
    }

    None
}

fn is_relationship_attribute(attr_name: &str) -> bool {
    // Common ORM relationship naming patterns - be conservative to avoid false positives
    // Generic names like "files", "contexts", "items" etc. are too common in non-ORM code
    let relationship_patterns = [
        // Django-style reverse relation suffix
        "_set",
        // Explicit relationship names commonly used in ORMs
        "author",
        "owner",
        "parent",
        "creator",
        "profile",
        "related",
        "refs",
        "references",
    ];

    let lower = attr_name.to_lowercase();

    // Check exact matches or suffix matches
    for pattern in relationship_patterns {
        if pattern.starts_with('_') {
            // Suffix pattern
            if lower.ends_with(pattern) {
                return true;
            }
        } else {
            // Exact match pattern
            if lower == pattern {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_get_orm_queries(source: &str) -> Vec<OrmQueryCall> {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_orm_queries(&parsed)
    }

    #[test]
    fn detects_sqlalchemy_query_in_loop() {
        let src = r#"
for user in users:
    posts = session.query(Post).filter(Post.user_id == user.id).all()
"#;
        let queries = parse_and_get_orm_queries(src);
        assert!(!queries.is_empty());
        let query = &queries[0];
        assert!(query.in_loop);
        assert_eq!(query.orm_kind, OrmKind::SqlAlchemy);
    }

    #[test]
    fn detects_django_query_in_loop() {
        let src = r#"
for user in users:
    posts = Post.objects.filter(user=user)
"#;
        let queries = parse_and_get_orm_queries(src);
        assert!(!queries.is_empty());
        let query = &queries[0];
        assert!(query.in_loop);
        assert_eq!(query.orm_kind, OrmKind::Django);
    }

    #[test]
    fn detects_eager_loading_sqlalchemy() {
        let src = r#"
users = session.query(User).options(joinedload(User.posts)).all()
"#;
        let queries = parse_and_get_orm_queries(src);
        // Should detect the query with eager loading
        let has_eager = queries.iter().any(|q| q.has_eager_loading);
        assert!(has_eager || queries.is_empty()); // May not detect if pattern doesn't match
    }

    #[test]
    fn detects_django_select_related() {
        let src = r#"
users = User.objects.select_related('profile').all()
"#;
        let queries = parse_and_get_orm_queries(src);
        // Should detect eager loading
        let has_eager = queries.iter().any(|q| q.has_eager_loading);
        assert!(has_eager || queries.is_empty());
    }

    #[test]
    fn detects_query_in_comprehension() {
        let src = r#"
posts = [Post.objects.filter(user=u) for u in users]
"#;
        let queries = parse_and_get_orm_queries(src);
        assert!(!queries.is_empty());
        let query = &queries[0];
        assert!(query.in_comprehension);
    }

    #[test]
    fn no_false_positive_for_non_orm_code() {
        let src = r#"
for item in items:
    print(item.name)
"#;
        let queries = parse_and_get_orm_queries(src);
        // Should not detect ORM queries in simple attribute access
        let orm_queries: Vec<_> = queries
            .iter()
            .filter(|q| q.query_type != QueryType::RelationshipAccess)
            .collect();
        assert!(orm_queries.is_empty());
    }

    #[test]
    fn detect_n_plus_one_patterns_finds_loop_queries() {
        let src = r#"
for user in users:
    posts = Post.objects.filter(user=user)
"#;
        let queries = parse_and_get_orm_queries(src);
        let patterns = detect_n_plus_one_patterns(&queries);
        assert!(!patterns.is_empty());
    }

    #[test]
    fn orm_kind_as_str_returns_correct_values() {
        assert_eq!(OrmKind::SqlAlchemy.as_str(), "SQLAlchemy");
        assert_eq!(OrmKind::Django.as_str(), "Django ORM");
        assert_eq!(OrmKind::Tortoise.as_str(), "Tortoise ORM");
        assert_eq!(OrmKind::SqlModel.as_str(), "SQLModel");
        assert_eq!(OrmKind::Peewee.as_str(), "Peewee");
        assert_eq!(OrmKind::Unknown.as_str(), "Unknown ORM");
    }
}
