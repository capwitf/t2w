use std::pin::Pin;

use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::prompt::PromptRequest;

pub type HtmlChunkStream = Pin<Box<dyn Stream<Item = Result<String, ProviderError>> + Send>>;

pub trait LlmProvider: Send + Sync {
    fn stream_html(&self, request: PromptRequest) -> HtmlChunkStream;
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("invalid SSE payload: {0}")]
    Json(#[from] serde_json::Error),
    #[error("provider returned an error: {0}")]
    Remote(String),
}

#[derive(Debug, Clone)]
pub struct MockProvider {
    chunks: Vec<String>,
    delay_ms: u64,
}

impl MockProvider {
    pub fn new(chunks: Vec<String>) -> Self {
        Self {
            chunks,
            delay_ms: 0,
        }
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }
}

impl LlmProvider for MockProvider {
    fn stream_html(&self, _request: PromptRequest) -> HtmlChunkStream {
        let chunks = self.chunks.clone();
        let delay_ms = self.delay_ms;
        Box::pin(async_stream::try_stream! {
            for chunk in chunks {
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                yield chunk;
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
    pub max_tokens: u32,
}

impl AnthropicConfig {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            base_url: "https://api.anthropic.com".to_string(),
            max_tokens: 8_192,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    client: Client,
    config: AnthropicConfig,
}

impl AnthropicProvider {
    pub fn new(client: Client, config: AnthropicConfig) -> Self {
        Self { client, config }
    }
}

impl LlmProvider for AnthropicProvider {
    fn stream_html(&self, request: PromptRequest) -> HtmlChunkStream {
        let client = self.client.clone();
        let config = self.config.clone();

        Box::pin(async_stream::try_stream! {
            let response = client
                .post(format!("{}/v1/messages", config.base_url.trim_end_matches('/')))
                .header("x-api-key", config.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .json(&AnthropicMessagesRequest {
                    model: if request.model.is_empty() {
                        config.model.clone()
                    } else {
                        request.model.clone()
                    },
                    max_tokens: config.max_tokens,
                    stream: true,
                    system: request.system_prompt,
                    messages: vec![AnthropicMessage {
                        role: "user".to_string(),
                        content: request.user_prompt,
                    }],
                })
                .send()
                .await?
                .error_for_status()?;

            let mut decoder = AnthropicSseDecoder::default();
            let mut sanitizer = HtmlStreamSanitizer::default();
            let mut body = response.bytes_stream();
            while let Some(chunk) = body.next().await {
                let bytes = chunk?;
                for delta in decoder.push_bytes(&bytes)? {
                    if let Some(clean) = sanitizer.push(&delta)
                        && !clean.is_empty()
                    {
                        yield clean;
                    }
                }
            }
        })
    }
}

#[derive(Debug, Default)]
pub struct AnthropicSseDecoder {
    buffer: String,
    pending_bytes: Vec<u8>,
}

impl AnthropicSseDecoder {
    pub fn push_bytes(&mut self, fragment: &[u8]) -> Result<Vec<String>, ProviderError> {
        self.pending_bytes.extend_from_slice(fragment);

        match std::str::from_utf8(&self.pending_bytes) {
            Ok(valid) => {
                self.buffer.push_str(&valid.replace("\r\n", "\n"));
                self.pending_bytes.clear();
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&self.pending_bytes[..valid_up_to])
                        .expect("valid UTF-8 prefix");
                    self.buffer.push_str(&valid.replace("\r\n", "\n"));
                    self.pending_bytes.drain(..valid_up_to);
                }

                if error.error_len().is_some() {
                    return Err(ProviderError::Remote(
                        "invalid UTF-8 received from Anthropic SSE stream".to_string(),
                    ));
                }
            }
        }

        self.drain_events()
    }

    pub fn push(&mut self, fragment: &str) -> Result<Vec<String>, ProviderError> {
        self.buffer.push_str(&fragment.replace("\r\n", "\n"));
        self.drain_events()
    }

    fn drain_events(&mut self) -> Result<Vec<String>, ProviderError> {
        let mut deltas = Vec::new();

        while let Some(index) = self.buffer.find("\n\n") {
            let event = self.buffer[..index].to_string();
            self.buffer = self.buffer[index + 2..].to_string();

            if let Some(text) = parse_sse_event(&event)? {
                deltas.push(text);
            }
        }

        Ok(deltas)
    }
}

#[derive(Debug, Default)]
pub struct HtmlStreamSanitizer {
    started: bool,
    prefix: String,
    html: String,
}

impl HtmlStreamSanitizer {
    pub fn push(&mut self, fragment: &str) -> Option<String> {
        let normalized = strip_fence_tokens(fragment);

        if self.started {
            if normalized.is_empty() {
                return None;
            }
            self.html.push_str(&normalized);
            return Some(normalized);
        }

        self.prefix.push_str(&normalized);
        let lower = self.prefix.to_ascii_lowercase();
        let start_index = lower
            .find("<!doctype")
            .or_else(|| lower.find("<!do"))
            .or_else(|| lower.find("<html"))?;
        let html = self.prefix[start_index..].to_string();
        self.started = true;
        self.prefix.clear();
        self.html.push_str(&html);
        Some(html)
    }

    pub fn finish(&self) -> String {
        strip_fence_tokens(&self.html).trim().to_string()
    }
}

fn parse_sse_event(event: &str) -> Result<Option<String>, ProviderError> {
    let data = event
        .lines()
        .filter_map(|line| line.strip_prefix("data:").map(str::trim))
        .collect::<Vec<_>>()
        .join("\n");

    if data.is_empty() || data == "[DONE]" {
        return Ok(None);
    }

    let payload: AnthropicEvent = serde_json::from_str(&data)?;
    if payload.kind == "error" {
        let message = payload
            .error
            .and_then(|error| error.message)
            .unwrap_or_else(|| "unknown Anthropic streaming error".to_string());
        return Err(ProviderError::Remote(message));
    }

    if payload.kind == "content_block_delta"
        && let Some(delta) = payload.delta
        && delta.kind == "text_delta"
    {
        return Ok(delta.text);
    }

    Ok(None)
}

fn strip_fence_tokens(value: &str) -> String {
    value
        .replace("```html", "")
        .replace("```HTML", "")
        .replace("```", "")
}

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    max_tokens: u32,
    stream: bool,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicEvent {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    delta: Option<AnthropicDelta>,
    #[serde(default)]
    error: Option<AnthropicErrorPayload>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicErrorPayload {
    #[serde(default)]
    message: Option<String>,
}

#[cfg(test)]
mod tests {
    use futures::TryStreamExt;

    use crate::prompt::PromptRequest;
    use crate::provider::{AnthropicSseDecoder, HtmlStreamSanitizer, LlmProvider, MockProvider};

    #[tokio::test]
    async fn mock_provider_streams_configured_chunks_in_order() {
        let provider = MockProvider::new(vec![
            "<!DOCTYPE html><html>".to_string(),
            "<body>hello</body>".to_string(),
            "</html>".to_string(),
        ]);

        let chunks = provider
            .stream_html(PromptRequest {
                model: "mock".to_string(),
                system_prompt: "system".to_string(),
                user_prompt: "user".to_string(),
            })
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(
            chunks,
            vec!["<!DOCTYPE html><html>", "<body>hello</body>", "</html>"]
        );
    }

    #[test]
    fn anthropic_sse_decoder_emits_only_text_delta_events() {
        let mut decoder = AnthropicSseDecoder::default();
        let output = decoder
            .push(
                "event: message_start\n\
                 data: {\"type\":\"message_start\"}\n\n\
                 event: content_block_delta\n\
                 data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"<!DO\"}}\n\n\
                 event: ping\n\
                 data: {\"type\":\"ping\"}\n\n\
                 event: content_block_delta\n\
                 data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"CTYPE html>\"}}\n\n",
            )
            .unwrap();

        assert_eq!(output, vec!["<!DO".to_string(), "CTYPE html>".to_string()]);
    }

    #[test]
    fn anthropic_sse_decoder_handles_utf8_split_across_byte_chunks() {
        let mut decoder = AnthropicSseDecoder::default();
        let first = b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"\xE4\xBB";
        let second = b"\xAA\"}}\n\n";

        let no_output = decoder.push_bytes(first).unwrap();
        let output = decoder.push_bytes(second).unwrap();

        assert!(no_output.is_empty());
        assert_eq!(output, vec!["仪".to_string()]);
    }

    #[test]
    fn html_stream_sanitizer_drops_preamble_and_code_fences() {
        let mut sanitizer = HtmlStreamSanitizer::default();

        assert_eq!(
            sanitizer.push("Here is your artifact\n```html\n<!DO"),
            Some("<!DO".to_string())
        );
        assert_eq!(
            sanitizer.push("CTYPE html><html><body>Hi</body></html>\n```"),
            Some("CTYPE html><html><body>Hi</body></html>\n".to_string())
        );
        assert_eq!(
            sanitizer.finish(),
            "<!DOCTYPE html><html><body>Hi</body></html>"
        );
    }
}
