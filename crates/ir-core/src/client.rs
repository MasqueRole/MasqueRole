//! Provider-agnostic LLM client.
//!
//! All model access goes through [`LlmProvider`]. [`OpenAiCompatible`] implements it for
//! any endpoint that speaks the OpenAI chat-completions protocol. Provider names live
//! only in [`PRESETS`], as data.

use std::collections::VecDeque;
use std::fmt;
use std::time::Duration;

use futures_util::future::BoxFuture;
use futures_util::stream::{self, BoxStream};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};

use crate::settings::ApiSettings;

/// Longest error body kept in an error message.
const MAX_ERROR_BODY: usize = 300;

/// Everything that can go wrong talking to a provider.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LlmError {
    /// The key is missing, wrong, or not allowed to use the model (HTTP 401 or 403).
    #[error("authentication failed: {0}")]
    Auth(String),
    /// Too many requests (HTTP 429).
    #[error("rate limited{}", match .retry_after_secs {
        Some(s) => format!(", retry in {s}s"),
        None => String::new(),
    })]
    RateLimit {
        /// Seconds the provider asked us to wait, if it said.
        retry_after_secs: Option<u64>,
    },
    /// The request never completed: DNS, connection, TLS, timeout, or a dropped stream.
    #[error("network error: {0}")]
    Network(String),
    /// The provider answered, but not with something we understand.
    #[error("bad response: {0}")]
    BadResponse(String),
}

/// Who wrote a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Instructions to the model.
    System,
    /// The player.
    User,
    /// The model.
    Assistant,
}

/// One chat message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Who wrote it.
    pub role: Role,
    /// The text.
    pub content: String,
}

impl ChatMessage {
    /// A system message.
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// A user message.
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// An assistant message.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// Sampling parameters for one request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GenerationParams {
    /// Sampling temperature.
    pub temperature: f32,
    /// Maximum tokens in the response.
    pub max_tokens: u32,
}

impl From<&ApiSettings> for GenerationParams {
    fn from(api: &ApiSettings) -> Self {
        Self {
            temperature: api.temperature,
            max_tokens: api.max_tokens,
        }
    }
}

/// A stream of text chunks, in order. Ends after the last chunk; an `Err` item ends it early.
pub type TextStream = BoxStream<'static, Result<String, LlmError>>;

/// A source of model responses.
///
/// Methods return boxed futures so the trait can be used as `dyn LlmProvider`.
pub trait LlmProvider: Send + Sync {
    /// Start a streaming response. Errors the provider reports up front (bad key, rate
    /// limit) come back as `Err`; errors during the stream arrive as stream items.
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        params: GenerationParams,
    ) -> BoxFuture<'_, Result<TextStream, LlmError>>;

    /// Get a whole response at once, for summaries and fact extraction.
    fn complete(
        &self,
        messages: Vec<ChatMessage>,
        params: GenerationParams,
    ) -> BoxFuture<'_, Result<String, LlmError>>;
}

/// A provider preset: connection defaults, kept as data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderPreset {
    /// Identifier stored in `settings.toml` as `api.provider`.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Default endpoint. Empty means the user must supply one.
    pub base_url: &'static str,
    /// Whether the provider needs an API key.
    pub key_required: bool,
}

/// Built-in presets. Add new OpenAI-compatible providers here, not in code.
pub const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openai-compatible",
        name: "Custom (OpenAI-compatible)",
        base_url: "",
        // Most custom endpoints are local servers; one that wants a key answers 401.
        key_required: false,
    },
    ProviderPreset {
        id: "xai",
        name: "xAI",
        base_url: "https://api.x.ai/v1",
        key_required: true,
    },
    ProviderPreset {
        id: "openai",
        name: "OpenAI",
        base_url: "https://api.openai.com/v1",
        key_required: true,
    },
    ProviderPreset {
        id: "anthropic",
        name: "Anthropic (OpenAI compatibility endpoint)",
        base_url: "https://api.anthropic.com/v1",
        key_required: true,
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        base_url: "https://openrouter.ai/api/v1",
        key_required: true,
    },
    ProviderPreset {
        id: "ollama",
        name: "Ollama (local)",
        base_url: "http://localhost:11434/v1",
        key_required: false,
    },
    ProviderPreset {
        id: "lmstudio",
        name: "LM Studio (local)",
        base_url: "http://localhost:1234/v1",
        key_required: false,
    },
];

/// Whether `base_url` points at this computer or its local network, where a model server
/// is usually free to use.
fn is_local_url(base_url: &str) -> bool {
    let rest = base_url
        .split_once("://")
        .map_or(base_url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = if let Some(inner) = authority.strip_prefix('[') {
        inner.split(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    }
    .to_ascii_lowercase();
    let private_172 = host
        .strip_prefix("172.")
        .and_then(|rest| rest.split('.').next()?.parse::<u8>().ok())
        .is_some_and(|second| (16..=31).contains(&second));
    host == "localhost"
        || host == "::1"
        || host == "0.0.0.0"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.starts_with("127.")
        || host.starts_with("10.")
        || host.starts_with("192.168.")
        || private_172
}

/// Whether using this provider can cost money: a hosted provider that needs an API key, or
/// a custom endpoint that is not on this computer or its local network. An empty URL is
/// not paid, because nothing is sent anywhere yet.
pub fn is_paid_provider(provider: &str, base_url: &str) -> bool {
    match find_preset(provider) {
        Some(preset) if preset.key_required => true,
        Some(preset) if !preset.base_url.is_empty() => false,
        _ => !base_url.trim().is_empty() && !is_local_url(base_url.trim()),
    }
}

/// Look up a preset by its identifier.
pub fn find_preset(id: &str) -> Option<&'static ProviderPreset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Connection details for [`OpenAiCompatible`].
#[derive(Clone)]
pub struct ProviderConfig {
    /// Endpoint without the `/chat/completions` suffix.
    pub base_url: String,
    /// Model name.
    pub model: String,
    /// API key, if the endpoint needs one.
    pub api_key: Option<String>,
}

impl ProviderConfig {
    /// Build from settings plus a key from the keychain.
    pub fn from_settings(api: &ApiSettings, api_key: Option<String>) -> Self {
        Self {
            base_url: api.base_url.clone(),
            model: api.model.clone(),
            api_key,
        }
    }
}

impl fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

/// A client for any endpoint implementing the OpenAI chat-completions protocol.
#[derive(Clone)]
pub struct OpenAiCompatible {
    http: reqwest::Client,
    url: String,
    model: String,
    api_key: Option<String>,
}

impl fmt::Debug for OpenAiCompatible {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpenAiCompatible")
            .field("url", &self.url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    max_tokens: u32,
    stream: bool,
}

impl OpenAiCompatible {
    /// Create a client. Fails only if the HTTP stack cannot be set up.
    pub fn new(config: ProviderConfig) -> Result<Self, LlmError> {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| LlmError::Network(e.to_string()))?;
        let api_key = config.api_key.filter(|k| !k.is_empty());
        if let Some(key) = &api_key {
            crate::redact::register_secret(key);
        }
        Ok(Self {
            http,
            url: format!("{}/chat/completions", config.base_url.trim_end_matches('/')),
            model: config.model,
            api_key,
        })
    }

    async fn send(
        &self,
        messages: &[ChatMessage],
        params: GenerationParams,
        stream: bool,
        timeout: Option<Duration>,
    ) -> Result<reqwest::Response, LlmError> {
        let body = ChatRequest {
            model: &self.model,
            messages,
            temperature: params.temperature,
            max_tokens: params.max_tokens,
            stream,
        };
        let mut request = self.http.post(&self.url).json(&body);
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        tracing::debug!(url = %self.url, model = %self.model, stream, messages = messages.len(), "sending chat request");
        let response = request.send().await.map_err(|e| {
            let error = network_error(e);
            tracing::warn!(%error, "chat request failed");
            error
        })?;
        let status = response.status();
        tracing::debug!(status = status.as_u16(), "chat response");
        if status.is_success() {
            return Ok(response);
        }
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse().ok());
        let body = response.text().await.unwrap_or_default();
        let error = error_from_status(status.as_u16(), retry_after, &body);
        tracing::warn!(status = status.as_u16(), %error, "provider rejected the request");
        Err(error)
    }
}

impl LlmProvider for OpenAiCompatible {
    fn stream_chat(
        &self,
        messages: Vec<ChatMessage>,
        params: GenerationParams,
    ) -> BoxFuture<'_, Result<TextStream, LlmError>> {
        Box::pin(async move {
            let response = self.send(&messages, params, true, None).await?;
            Ok(decode_stream(response.bytes_stream().boxed()))
        })
    }

    fn complete(
        &self,
        messages: Vec<ChatMessage>,
        params: GenerationParams,
    ) -> BoxFuture<'_, Result<String, LlmError>> {
        Box::pin(async move {
            let response = self
                .send(&messages, params, false, Some(Duration::from_secs(120)))
                .await?;
            let body = response.text().await.map_err(network_error)?;
            parse_completion(&body)
        })
    }
}

fn network_error(e: reqwest::Error) -> LlmError {
    // `without_url` keeps query strings, and anything else in the URL, out of messages.
    LlmError::Network(e.without_url().to_string())
}

/// Map a failed HTTP response to a typed error.
fn error_from_status(status: u16, retry_after_secs: Option<u64>, body: &str) -> LlmError {
    let detail = error_message(body);
    match status {
        401 | 403 => LlmError::Auth(detail),
        429 => LlmError::RateLimit { retry_after_secs },
        _ => LlmError::BadResponse(format!("HTTP {status}: {detail}")),
    }
}

/// The provider's own error message if the body has one, else a trimmed copy of the body.
fn error_message(body: &str) -> String {
    #[derive(Deserialize)]
    struct Envelope {
        error: Option<Detail>,
    }
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Detail {
        Object { message: String },
        Text(String),
    }

    let parsed = serde_json::from_str::<Envelope>(body)
        .ok()
        .and_then(|e| e.error)
        .map(|d| match d {
            Detail::Object { message } | Detail::Text(message) => message,
        });
    let text = parsed.unwrap_or_else(|| body.trim().to_string());
    if text.is_empty() {
        return "no details".into();
    }
    text.chars().take(MAX_ERROR_BODY).collect()
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<CompletionChoice>,
}

#[derive(Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

#[derive(Deserialize)]
struct CompletionMessage {
    content: Option<String>,
}

fn parse_completion(body: &str) -> Result<String, LlmError> {
    let completion: Completion = serde_json::from_str(body)
        .map_err(|e| LlmError::BadResponse(format!("unexpected response: {e}")))?;
    completion
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.message.content)
        .ok_or_else(|| LlmError::BadResponse("response has no message content".into()))
}

/// Incremental parser for server-sent events. Feed it bytes as they arrive; it returns the
/// `data` payload of each completed event. Tolerates chunks that split lines or UTF-8
/// characters, and both `\n` and `\r\n` line endings.
#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    data: Vec<String>,
}

impl SseParser {
    /// Add bytes and return the payloads of events completed by them.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            self.line(line.trim_end_matches(['\r', '\n']), &mut events);
        }
        events
    }

    /// Call when the connection ends, to flush a final event that lacked a blank line.
    pub fn finish(&mut self) -> Vec<String> {
        let mut events = Vec::new();
        if !self.buffer.is_empty() {
            let rest = std::mem::take(&mut self.buffer);
            self.line(
                String::from_utf8_lossy(&rest).trim_end_matches('\r'),
                &mut events,
            );
        }
        self.dispatch(&mut events);
        events
    }

    fn line(&mut self, line: &str, events: &mut Vec<String>) {
        if line.is_empty() {
            self.dispatch(events);
        } else if let Some(value) = line.strip_prefix("data:") {
            self.data
                .push(value.strip_prefix(' ').unwrap_or(value).to_string());
        }
        // Comments (`:`) and other fields (`event:`, `id:`, `retry:`) carry nothing we use.
    }

    fn dispatch(&mut self, events: &mut Vec<String>) {
        if !self.data.is_empty() {
            events.push(self.data.join("\n"));
            self.data.clear();
        }
    }
}

/// What one SSE payload means for the text stream.
#[derive(Debug, PartialEq, Eq)]
enum Chunk {
    Text(String),
    Skip,
    Done,
}

fn parse_chunk(data: &str) -> Result<Chunk, LlmError> {
    #[derive(Deserialize)]
    struct Event {
        #[serde(default)]
        choices: Vec<Choice>,
        error: Option<serde_json::Value>,
    }
    #[derive(Deserialize)]
    struct Choice {
        delta: Option<Delta>,
    }
    #[derive(Deserialize)]
    struct Delta {
        content: Option<String>,
    }

    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Chunk::Done);
    }
    let event: Event = serde_json::from_str(data)
        .map_err(|e| LlmError::BadResponse(format!("unreadable stream event: {e}")))?;
    if let Some(error) = event.error {
        return Err(LlmError::BadResponse(error_message(
            &serde_json::json!({ "error": error }).to_string(),
        )));
    }
    let text = event
        .choices
        .into_iter()
        .next()
        .and_then(|c| c.delta)
        .and_then(|d| d.content)
        .filter(|t| !t.is_empty());
    Ok(text.map_or(Chunk::Skip, Chunk::Text))
}

struct DecodeState {
    bytes: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    parser: SseParser,
    pending: VecDeque<Result<String, LlmError>>,
    done: bool,
}

impl DecodeState {
    fn handle(&mut self, payloads: Vec<String>) {
        for payload in payloads {
            match parse_chunk(&payload) {
                Ok(Chunk::Text(t)) => self.pending.push_back(Ok(t)),
                Ok(Chunk::Skip) => {}
                Ok(Chunk::Done) => {
                    self.done = true;
                    return;
                }
                Err(e) => {
                    self.pending.push_back(Err(e));
                    self.done = true;
                    return;
                }
            }
        }
    }
}

/// Turn a raw byte stream of SSE into a stream of text chunks.
fn decode_stream(bytes: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>) -> TextStream {
    let state = DecodeState {
        bytes,
        parser: SseParser::default(),
        pending: VecDeque::new(),
        done: false,
    };
    stream::unfold(state, |mut state| async move {
        loop {
            if let Some(item) = state.pending.pop_front() {
                return Some((item, state));
            }
            if state.done {
                return None;
            }
            match state.bytes.next().await {
                Some(Ok(chunk)) => {
                    let payloads = state.parser.push(&chunk);
                    state.handle(payloads);
                }
                Some(Err(e)) => {
                    state.done = true;
                    return Some((Err(network_error(e)), state));
                }
                None => {
                    let payloads = state.parser.finish();
                    state.handle(payloads);
                    state.done = true;
                }
            }
        }
    })
    .boxed()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::TryStreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::task::JoinHandle;

    use super::*;

    const PARAMS: GenerationParams = GenerationParams {
        temperature: 0.5,
        max_tokens: 64,
    };

    /// Serve exactly one HTTP response on a local port. The body is written in the given
    /// pieces with short pauses, so tests can split events across network reads. Returns
    /// the base URL and a handle yielding the raw request the server received.
    async fn mock_server(
        status: &str,
        headers: &[&str],
        body_pieces: &[&str],
    ) -> (String, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let head = format!(
            "HTTP/1.1 {status}\r\n{}Connection: close\r\n\r\n",
            headers
                .iter()
                .map(|h| format!("{h}\r\n"))
                .collect::<String>()
        );
        let pieces: Vec<String> = body_pieces.iter().map(|s| s.to_string()).collect();

        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = socket.read(&mut buf).await.unwrap();
                request.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&request);
                if let Some(split) = text.find("\r\n\r\n") {
                    let length = text[..split]
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(str::to_owned)
                        })
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if request.len() >= split + 4 + length {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            socket.write_all(head.as_bytes()).await.unwrap();
            for piece in pieces {
                socket.write_all(piece.as_bytes()).await.unwrap();
                socket.flush().await.unwrap();
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let _ = socket.shutdown().await;
            String::from_utf8_lossy(&request).into_owned()
        });
        (base_url, handle)
    }

    fn client(base_url: &str, key: Option<&str>) -> OpenAiCompatible {
        OpenAiCompatible::new(ProviderConfig {
            base_url: base_url.into(),
            model: "test-model".into(),
            api_key: key.map(Into::into),
        })
        .unwrap()
    }

    fn sse(text: &str) -> String {
        format!(
            "data: {}\n\n",
            serde_json::json!({ "choices": [{ "delta": { "content": text } }] })
        )
    }

    #[test]
    fn sse_parser_handles_split_chunks_and_crlf() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: hel").is_empty());
        assert!(p.push(b"lo\r\n").is_empty());
        assert_eq!(p.push(b"\r\ndata: a\ndata: b\n\n"), ["hello", "a\nb"]);
    }

    #[test]
    fn sse_parser_ignores_comments_and_other_fields() {
        let mut p = SseParser::default();
        assert_eq!(p.push(b": ping\nevent: message\nid: 7\ndata:x\n\n"), ["x"]);
    }

    #[test]
    fn sse_parser_handles_split_utf8() {
        let bytes = "data: h\u{e9}llo \u{1f600}\n\n".as_bytes();
        let (a, b) = bytes.split_at(8); // inside the two-byte "\u{e9}"
        let mut p = SseParser::default();
        assert!(p.push(a).is_empty());
        assert_eq!(p.push(b), ["h\u{e9}llo \u{1f600}"]);
    }

    #[test]
    fn sse_parser_finish_flushes_last_event() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: last").is_empty());
        assert_eq!(p.finish(), ["last"]);
        assert!(p.finish().is_empty());
    }

    #[test]
    fn chunk_parsing() {
        assert_eq!(parse_chunk("[DONE]"), Ok(Chunk::Done));
        assert_eq!(
            parse_chunk(r#"{"choices":[{"delta":{"content":"Hi"}}]}"#),
            Ok(Chunk::Text("Hi".into()))
        );
        // Role-only first chunk, empty content, and usage-only chunks carry no text.
        assert_eq!(
            parse_chunk(r#"{"choices":[{"delta":{"role":"assistant"}}]}"#),
            Ok(Chunk::Skip)
        );
        assert_eq!(
            parse_chunk(r#"{"choices":[{"delta":{"content":""}}]}"#),
            Ok(Chunk::Skip)
        );
        assert_eq!(parse_chunk(r#"{"choices":[],"usage":{}}"#), Ok(Chunk::Skip));
        assert!(matches!(
            parse_chunk("not json"),
            Err(LlmError::BadResponse(_))
        ));
        assert_eq!(
            parse_chunk(r#"{"error":{"message":"overloaded"}}"#),
            Err(LlmError::BadResponse("overloaded".into()))
        );
    }

    #[test]
    fn status_mapping() {
        assert!(matches!(
            error_from_status(401, None, ""),
            LlmError::Auth(_)
        ));
        assert!(matches!(
            error_from_status(403, None, ""),
            LlmError::Auth(_)
        ));
        assert_eq!(
            error_from_status(429, Some(7), ""),
            LlmError::RateLimit {
                retry_after_secs: Some(7)
            }
        );
        assert_eq!(
            error_from_status(500, None, r#"{"error":{"message":"boom"}}"#),
            LlmError::BadResponse("HTTP 500: boom".into())
        );
        assert_eq!(
            error_from_status(502, None, "<html>bad gateway</html>"),
            LlmError::BadResponse("HTTP 502: <html>bad gateway</html>".into())
        );
        let long = "x".repeat(10_000);
        assert!(error_message(&long).len() <= MAX_ERROR_BODY);
    }

    #[test]
    fn rate_limit_message_mentions_retry() {
        let e = LlmError::RateLimit {
            retry_after_secs: Some(3),
        };
        assert_eq!(e.to_string(), "rate limited, retry in 3s");
    }

    #[test]
    fn hosted_providers_are_paid_and_local_ones_are_not() {
        for id in ["xai", "openai", "anthropic", "openrouter"] {
            let preset = find_preset(id).unwrap();
            assert!(is_paid_provider(id, preset.base_url), "{id}");
        }
        for id in ["ollama", "lmstudio"] {
            let preset = find_preset(id).unwrap();
            assert!(!is_paid_provider(id, preset.base_url), "{id}");
        }
    }

    #[test]
    fn a_custom_endpoint_is_paid_unless_it_is_local() {
        let custom = |url| is_paid_provider("openai-compatible", url);
        assert!(!custom(""));
        assert!(!custom("http://localhost:8080/v1"));
        assert!(!custom("http://127.0.0.1:5000"));
        assert!(!custom("http://[::1]:5000/v1"));
        assert!(!custom("http://192.168.1.20:1234/v1"));
        assert!(!custom("http://10.0.0.5/v1"));
        assert!(!custom("http://172.20.0.5/v1"));
        assert!(!custom("http://gpu-box.local:8000/v1"));
        assert!(custom("https://api.together.xyz/v1"));
        assert!(custom("http://172.32.0.5/v1"));
        assert!(custom("https://localhost.example.com/v1"));
        assert!(custom("https://user@api.example.com/v1"));
    }

    #[test]
    fn presets_are_well_formed() {
        let mut ids: Vec<_> = PRESETS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PRESETS.len(), "preset ids must be unique");
        assert!(PRESETS
            .iter()
            .all(|p| p.base_url.is_empty() || p.base_url.starts_with("http")));
        assert!(!find_preset("ollama").unwrap().key_required);
        assert!(find_preset("xai").unwrap().key_required);
        assert!(!find_preset("openai-compatible").unwrap().key_required);
        assert!(find_preset("nope").is_none());
        // The default provider setting must be a known preset.
        assert!(find_preset(&ApiSettings::default().provider).is_some());
    }

    #[test]
    fn params_come_from_settings() {
        let p = GenerationParams::from(&ApiSettings::default());
        assert_eq!(p.temperature, 0.9);
        assert_eq!(p.max_tokens, 800);
    }

    #[test]
    fn debug_output_redacts_the_key() {
        let c = client("http://localhost:1/v1", Some("super-secret-key"));
        assert!(!format!("{c:?}").contains("super-secret-key"));
        let config = ProviderConfig {
            base_url: "http://x".into(),
            model: "m".into(),
            api_key: Some("super-secret-key".into()),
        };
        assert!(!format!("{config:?}").contains("super-secret-key"));
    }

    #[tokio::test]
    async fn complete_returns_the_message_and_sends_auth() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"A summary."}}]}"#;
        let (url, request) =
            mock_server("200 OK", &["Content-Type: application/json"], &[body]).await;
        let text = client(&format!("{url}/"), Some("secret"))
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap();
        assert_eq!(text, "A summary.");

        let request = request.await.unwrap();
        assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(request
            .to_lowercase()
            .contains("authorization: bearer secret"));
        assert!(request.contains(r#""model":"test-model""#));
        assert!(request.contains(r#""stream":false"#));
        assert!(request.contains(r#""role":"user""#));
    }

    #[tokio::test]
    async fn no_auth_header_without_a_key() {
        let body = r#"{"choices":[{"message":{"content":"ok"}}]}"#;
        let (url, request) = mock_server("200 OK", &[], &[body]).await;
        client(&url, None)
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap();
        assert!(!request
            .await
            .unwrap()
            .to_lowercase()
            .contains("authorization"));
    }

    #[tokio::test]
    async fn complete_rejects_a_malformed_body() {
        let (url, _) = mock_server("200 OK", &[], &[r#"{"nope":1}"#]).await;
        let err = client(&url, None)
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::BadResponse(_)));
    }

    #[tokio::test]
    async fn streams_text_in_order_across_split_reads() {
        let first = sse("Hel");
        let second = sse("lo");
        let (cut_a, cut_b) = second.split_at(10); // splits an event mid-line
        let (url, request) = mock_server(
            "200 OK",
            &["Content-Type: text/event-stream"],
            &[
                "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n",
                &first,
                cut_a,
                cut_b,
                &sse(" world"),
                "data: [DONE]\n\n",
                &sse("ignored after done"),
            ],
        )
        .await;

        let stream = client(&url, Some("k"))
            .stream_chat(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap();
        let chunks: Vec<String> = stream.try_collect().await.unwrap();
        assert_eq!(chunks.concat(), "Hello world");
        assert_eq!(chunks.first().map(String::as_str), Some("Hel"));

        assert!(request.await.unwrap().contains(r#""stream":true"#));
    }

    #[tokio::test]
    async fn stream_ends_cleanly_without_done_marker() {
        let (url, _) = mock_server("200 OK", &[], &[&sse("a"), &sse("b")]).await;
        let stream = client(&url, None)
            .stream_chat(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap();
        assert_eq!(stream.try_collect::<Vec<_>>().await.unwrap().concat(), "ab");
    }

    #[tokio::test]
    async fn in_stream_error_is_reported_then_stream_ends() {
        let (url, _) = mock_server(
            "200 OK",
            &[],
            &[
                &sse("a"),
                "data: {\"error\":{\"message\":\"cut off\"}}\n\n",
                &sse("b"),
            ],
        )
        .await;
        let mut stream = client(&url, None)
            .stream_chat(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap();
        assert_eq!(stream.next().await, Some(Ok("a".into())));
        assert_eq!(
            stream.next().await,
            Some(Err(LlmError::BadResponse("cut off".into())))
        );
        assert_eq!(stream.next().await, None);
    }

    #[tokio::test]
    async fn bad_key_is_an_auth_error_before_streaming() {
        let (url, _) = mock_server(
            "401 Unauthorized",
            &["Content-Type: application/json"],
            &[r#"{"error":{"message":"Invalid API key"}}"#],
        )
        .await;
        let result = client(&url, Some("wrong"))
            .stream_chat(vec![ChatMessage::user("hi")], PARAMS)
            .await;
        assert_eq!(result.err(), Some(LlmError::Auth("Invalid API key".into())));
    }

    #[tokio::test]
    async fn rate_limit_carries_retry_after() {
        let (url, _) = mock_server("429 Too Many Requests", &["Retry-After: 12"], &["{}"]).await;
        let err = client(&url, None)
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap_err();
        assert_eq!(
            err,
            LlmError::RateLimit {
                retry_after_secs: Some(12)
            }
        );
    }

    #[tokio::test]
    async fn server_error_is_a_bad_response() {
        let (url, _) = mock_server("500 Internal Server Error", &[], &["oops"]).await;
        let err = client(&url, None)
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap_err();
        assert_eq!(err, LlmError::BadResponse("HTTP 500: oops".into()));
    }

    #[tokio::test]
    async fn unreachable_server_is_a_network_error() {
        // Bind to get a free port, then close it so nothing is listening.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        drop(listener);
        let err = client(&url, None)
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap_err();
        assert!(matches!(err, LlmError::Network(_)));
    }

    #[tokio::test]
    async fn works_as_a_trait_object() {
        let body = r#"{"choices":[{"message":{"content":"dyn ok"}}]}"#;
        let (url, _) = mock_server("200 OK", &[], &[body]).await;
        let provider: Box<dyn LlmProvider> = Box::new(client(&url, None));
        let text = provider
            .complete(vec![ChatMessage::user("hi")], PARAMS)
            .await
            .unwrap();
        assert_eq!(text, "dyn ok");
    }
}
