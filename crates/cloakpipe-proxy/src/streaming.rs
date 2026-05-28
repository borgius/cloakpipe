//! SSE streaming rehydration for chat completion responses.

use crate::json_filter;
use bytes::Bytes;
use cloakpipe_core::{rehydrator::Rehydrator, vault::Vault};
use futures::{stream::Stream, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Consume an upstream SSE response and produce a rehydrated SSE stream.
pub async fn rehydrate_stream(
    response: reqwest::Response,
    vault: Arc<Mutex<Vault>>,
    request_id: String,
) -> impl Stream<Item = Result<String, std::io::Error>> {
    let mut buffer = String::new();

    async_stream::stream! {
        let byte_stream = response.text().await.unwrap_or_default();

        // Split SSE response into lines and process events
        for line in byte_stream.lines() {
            if let Some(data) = line.strip_prefix("data: ") {

                if data == "[DONE]" {
                    yield Ok("data: [DONE]\n\n".to_string());
                    continue;
                }

                // Parse the SSE JSON chunk
                if let Ok(mut chunk) = serde_json::from_str::<serde_json::Value>(data) {
                    // Extract delta content
                    if let Some(content) = chunk
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                        .map(|s| s.to_string())
                    {
                        let vault_guard = vault.lock().await;
                        let (rehydrated, _) = Rehydrator::rehydrate_chunk(
                            &content,
                            &mut buffer,
                            &vault_guard,
                        )
                        .unwrap_or((content.clone(), false));

                        if !rehydrated.is_empty() {
                            // Update the delta content with rehydrated text
                            if let Some(choices) = chunk.get_mut("choices").and_then(|c| c.as_array_mut()) {
                                if let Some(first) = choices.first_mut() {
                                    if let Some(delta) = first.get_mut("delta") {
                                        delta["content"] = serde_json::Value::String(rehydrated);
                                    }
                                }
                            }

                            let serialized = serde_json::to_string(&chunk).unwrap_or_default();
                            yield Ok(format!("data: {}\n\n", serialized));
                        }
                    } else {
                        // Non-content chunk (role, finish_reason, etc.) — pass through
                        yield Ok(format!("data: {}\n\n", data));
                    }
                } else {
                    // Unparseable data — pass through
                    yield Ok(format!("data: {}\n\n", data));
                }
            } else if !line.is_empty() {
                yield Ok(format!("{}\n", line));
            }
        }

        // Flush any remaining buffer
        if !buffer.is_empty() {
            tracing::debug!(request_id = %request_id, "Flushing remaining stream buffer");
        }
    }
}

/// Rehydrate a raw textual byte stream without assuming any provider-specific SSE shape.
pub fn rehydrate_text_stream(
    response: reqwest::Response,
    mappings: HashMap<String, String>,
    skip_rehydration: bool,
    request_id: String,
) -> impl Stream<Item = Result<Bytes, std::io::Error>> {
    let overlap = stream_overlap_bytes(&mappings);

    async_stream::stream! {
        let mut leftover = String::new();
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    if skip_rehydration || mappings.is_empty() {
                        yield Ok(bytes);
                        continue;
                    }

                    let text = match String::from_utf8(bytes.to_vec()) {
                        Ok(text) => text,
                        Err(error) => {
                            if !leftover.is_empty() {
                                yield Ok(Bytes::from(std::mem::take(&mut leftover)));
                            }
                            yield Ok(Bytes::from(error.into_bytes()));
                            continue;
                        }
                    };

                    let mut combined = std::mem::take(&mut leftover);
                    combined.push_str(&text);

                    if json_filter::should_skip_text_rewrite(&combined) {
                        yield Ok(Bytes::from(combined));
                        continue;
                    }

                    if combined.len() <= overlap {
                        leftover = combined;
                        continue;
                    }

                    let split_at = previous_char_boundary(&combined, combined.len().saturating_sub(overlap));
                    let (safe, tail) = combined.split_at(split_at);
                    leftover = tail.to_string();

                    let out = match Rehydrator::rehydrate_from_mappings(safe, &mappings) {
                        Ok(rehydrated) => rehydrated.text,
                        Err(_) => safe.to_string(),
                    };

                    if !out.is_empty() {
                        yield Ok(Bytes::from(out));
                    }
                }
                Err(error) => {
                    yield Err(std::io::Error::other(error));
                    return;
                }
            }
        }

        if !leftover.is_empty() {
            tracing::debug!(request_id = %request_id, "Flushing remaining llm-http stream buffer");
            let flushed = if skip_rehydration || mappings.is_empty() || json_filter::should_skip_text_rewrite(&leftover) {
                leftover
            } else {
                Rehydrator::rehydrate_from_mappings(&leftover, &mappings)
                    .map(|rehydrated| rehydrated.text)
                    .unwrap_or(leftover)
            };

            yield Ok(Bytes::from(flushed));
        }
    }
}

fn stream_overlap_bytes(mappings: &HashMap<String, String>) -> usize {
    let longest_mapping = mappings.keys().map(String::len).max().unwrap_or(0);
    longest_mapping.clamp(128, 1024)
}

fn previous_char_boundary(text: &str, mut index: usize) -> usize {
    index = index.min(text.len());
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::previous_char_boundary;

    #[test]
    fn previous_char_boundary_moves_back_to_valid_utf8_boundary() {
        let text = "aéz";
        let invalid_index = 2;

        assert_eq!(previous_char_boundary(text, invalid_index), 1);
    }
}
