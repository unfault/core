//! Common HTTP client abstractions for cross-language analysis.
//!
//! This module provides language-agnostic types for HTTP client calls,
//! enabling shared rule logic for timeout, retry, and other HTTP patterns.

use serde::{Deserialize, Serialize};

use super::CommonLocation;

/// HTTP client library classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpClientLibrary {
    // Python
    Requests,
    Httpx,
    Aiohttp,
    Urllib3,

    // Go
    NetHttp,
    Resty,

    // Rust
    Reqwest,
    Hyper,
    Ureq,

    // TypeScript/JavaScript
    Fetch,
    Axios,
    Got,
    NodeFetch,

    // Java
    HttpClient,
    OkHttp,
    RestTemplate,
    WebClient,

    // Generic/Unknown
    Other(String),
}

impl HttpClientLibrary {
    /// Get a human-readable name for the library
    pub fn as_str(&self) -> &str {
        match self {
            Self::Requests => "requests",
            Self::Httpx => "httpx",
            Self::Aiohttp => "aiohttp",
            Self::Urllib3 => "urllib3",
            Self::NetHttp => "net/http",
            Self::Resty => "resty",
            Self::Reqwest => "reqwest",
            Self::Hyper => "hyper",
            Self::Ureq => "ureq",
            Self::Fetch => "fetch",
            Self::Axios => "axios",
            Self::Got => "got",
            Self::NodeFetch => "node-fetch",
            Self::HttpClient => "HttpClient",
            Self::OkHttp => "OkHttp",
            Self::RestTemplate => "RestTemplate",
            Self::WebClient => "WebClient",
            Self::Other(s) => s,
        }
    }

    /// Get the language this library is typically used with
    pub fn typical_language(&self) -> &str {
        match self {
            Self::Requests | Self::Httpx | Self::Aiohttp | Self::Urllib3 => "Python",
            Self::NetHttp | Self::Resty => "Go",
            Self::Reqwest | Self::Hyper | Self::Ureq => "Rust",
            Self::Fetch | Self::Axios | Self::Got | Self::NodeFetch => "TypeScript",
            Self::HttpClient | Self::OkHttp | Self::RestTemplate | Self::WebClient => "Java",
            Self::Other(_) => "Unknown",
        }
    }
}

/// HTTP method classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Request, // Generic request method
    Other(String),
}

impl HttpMethod {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
            Self::Request => "REQUEST",
            Self::Other(s) => s,
        }
    }

    /// Check if this is a mutating method (POST, PUT, PATCH, DELETE)
    pub fn is_mutating(&self) -> bool {
        matches!(self, Self::Post | Self::Put | Self::Patch | Self::Delete)
    }
}

/// Retry mechanism detected for an HTTP call
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryMechanism {
    /// Decorator-based retry (Python: tenacity, backoff, stamina)
    Decorator(String),
    /// Wrapper/middleware retry (Go: hashicorp/go-retryablehttp)
    Middleware(String),
    /// Client-level retry configuration
    ClientConfig,
    /// Manual retry loop
    ManualLoop,
    /// Other retry mechanism
    Other(String),
}

/// A language-agnostic HTTP call site
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCall {
    /// The HTTP client library being used
    pub library: HttpClientLibrary,

    /// The HTTP method being called
    pub method: HttpMethod,

    /// The URL being called (if statically determinable)
    pub url: Option<String>,

    /// Whether this call has an explicit timeout configured
    pub has_timeout: bool,

    /// Timeout value in seconds (if statically determinable)
    pub timeout_value: Option<f64>,

    /// Retry mechanism detected for this call
    pub retry_mechanism: Option<RetryMechanism>,

    /// Full text of the call expression
    pub call_text: String,

    /// Location in source file
    pub location: CommonLocation,

    /// Name of enclosing function/method
    pub enclosing_function: Option<String>,

    /// Whether this call is inside an async context
    pub in_async_context: bool,

    /// Whether this call is inside a loop
    pub in_loop: bool,

    /// Start byte offset in source
    pub start_byte: usize,

    /// End byte offset in source
    pub end_byte: usize,
}

impl HttpCall {
    /// Check if this call needs a timeout
    pub fn needs_timeout(&self) -> bool {
        !self.has_timeout
    }

    /// Check if this call needs retry logic
    pub fn needs_retry(&self) -> bool {
        self.retry_mechanism.is_none()
    }

    /// Get a suggested timeout value based on library defaults
    pub fn suggested_timeout(&self) -> f64 {
        match self.library {
            HttpClientLibrary::Requests | HttpClientLibrary::Httpx => 30.0,
            HttpClientLibrary::NetHttp => 30.0,
            HttpClientLibrary::Reqwest => 30.0,
            HttpClientLibrary::Fetch | HttpClientLibrary::Axios => 30.0,
            HttpClientLibrary::RestTemplate | HttpClientLibrary::WebClient => 30.0,
            _ => 30.0,
        }
    }

    /// Get the timeout parameter name for this library
    pub fn timeout_param_name(&self) -> &'static str {
        match self.library {
            HttpClientLibrary::Requests | HttpClientLibrary::Httpx => "timeout",
            HttpClientLibrary::Aiohttp => "timeout",
            HttpClientLibrary::NetHttp => "Timeout",
            HttpClientLibrary::Reqwest => "timeout",
            HttpClientLibrary::Fetch => "signal", // AbortController timeout
            HttpClientLibrary::Axios => "timeout",
            HttpClientLibrary::RestTemplate => "setConnectTimeout",
            HttpClientLibrary::WebClient => "timeout",
            _ => "timeout",
        }
    }
}

/// Builder for creating HttpCall instances
#[derive(Debug, Default)]
pub struct HttpCallBuilder {
    library: Option<HttpClientLibrary>,
    method: Option<HttpMethod>,
    url: Option<String>,
    has_timeout: bool,
    timeout_value: Option<f64>,
    retry_mechanism: Option<RetryMechanism>,
    call_text: Option<String>,
    location: Option<CommonLocation>,
    enclosing_function: Option<String>,
    in_async_context: bool,
    in_loop: bool,
    start_byte: usize,
    end_byte: usize,
}

impl HttpCallBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn library(mut self, library: HttpClientLibrary) -> Self {
        self.library = Some(library);
        self
    }

    pub fn method(mut self, method: HttpMethod) -> Self {
        self.method = Some(method);
        self
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn has_timeout(mut self, has_timeout: bool) -> Self {
        self.has_timeout = has_timeout;
        self
    }

    pub fn timeout_value(mut self, value: f64) -> Self {
        self.timeout_value = Some(value);
        self.has_timeout = true;
        self
    }

    pub fn retry(mut self, mechanism: RetryMechanism) -> Self {
        self.retry_mechanism = Some(mechanism);
        self
    }

    pub fn call_text(mut self, text: impl Into<String>) -> Self {
        self.call_text = Some(text.into());
        self
    }

    pub fn location(mut self, location: CommonLocation) -> Self {
        self.location = Some(location);
        self
    }

    pub fn enclosing_function(mut self, name: impl Into<String>) -> Self {
        self.enclosing_function = Some(name.into());
        self
    }

    pub fn in_async(mut self, is_async: bool) -> Self {
        self.in_async_context = is_async;
        self
    }

    pub fn in_loop(mut self, in_loop: bool) -> Self {
        self.in_loop = in_loop;
        self
    }

    pub fn byte_range(mut self, start: usize, end: usize) -> Self {
        self.start_byte = start;
        self.end_byte = end;
        self
    }

    pub fn build(self) -> Option<HttpCall> {
        Some(HttpCall {
            library: self.library?,
            method: self.method.unwrap_or(HttpMethod::Get),
            url: self.url,
            has_timeout: self.has_timeout,
            timeout_value: self.timeout_value,
            retry_mechanism: self.retry_mechanism,
            call_text: self.call_text.unwrap_or_default(),
            location: self.location?,
            enclosing_function: self.enclosing_function,
            in_async_context: self.in_async_context,
            in_loop: self.in_loop,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;

    fn make_location() -> CommonLocation {
        CommonLocation {
            file_id: FileId(1),
            line: 10,
            column: 5,
            start_byte: 100,
            end_byte: 150,
        }
    }

    #[test]
    fn http_client_library_as_str() {
        assert_eq!(HttpClientLibrary::Requests.as_str(), "requests");
        assert_eq!(HttpClientLibrary::Reqwest.as_str(), "reqwest");
        assert_eq!(HttpClientLibrary::Fetch.as_str(), "fetch");
        assert_eq!(HttpClientLibrary::Other("custom".into()).as_str(), "custom");
    }

    #[test]
    fn http_method_is_mutating() {
        assert!(!HttpMethod::Get.is_mutating());
        assert!(HttpMethod::Post.is_mutating());
        assert!(HttpMethod::Put.is_mutating());
        assert!(HttpMethod::Delete.is_mutating());
        assert!(!HttpMethod::Head.is_mutating());
    }

    #[test]
    fn http_call_builder_creates_call() {
        let call = HttpCallBuilder::new()
            .library(HttpClientLibrary::Requests)
            .method(HttpMethod::Get)
            .url("https://example.com")
            .has_timeout(false)
            .call_text("requests.get('https://example.com')")
            .location(make_location())
            .build();

        assert!(call.is_some());
        let call = call.unwrap();
        assert!(matches!(call.library, HttpClientLibrary::Requests));
        assert!(!call.has_timeout);
        assert!(call.needs_timeout());
        assert!(call.needs_retry());
    }

    #[test]
    fn http_call_builder_with_timeout() {
        let call = HttpCallBuilder::new()
            .library(HttpClientLibrary::Httpx)
            .timeout_value(30.0)
            .location(make_location())
            .build()
            .unwrap();

        assert!(call.has_timeout);
        assert_eq!(call.timeout_value, Some(30.0));
        assert!(!call.needs_timeout());
    }

    #[test]
    fn http_call_suggested_timeout() {
        let call = HttpCallBuilder::new()
            .library(HttpClientLibrary::Requests)
            .location(make_location())
            .build()
            .unwrap();

        assert_eq!(call.suggested_timeout(), 30.0);
    }
}
