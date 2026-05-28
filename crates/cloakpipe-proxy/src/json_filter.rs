//! Conservative JSON filtering for generic llm-http request mutation.

use anyhow::Result;
use serde_json::{Map, Value};

pub fn mutate_json_text<F>(value: &mut Value, mutate: &mut F) -> Result<usize>
where
    F: FnMut(&str) -> Result<Option<String>>,
{
    visit_value(value, false, mutate)
}

pub fn is_textual_content_type(content_type: &str) -> bool {
    let content_type = content_type.split(';').next().unwrap_or("").trim();
    content_type.starts_with("text/")
        || content_type == "application/json"
        || content_type.ends_with("+json")
        || content_type == "application/xml"
        || content_type.ends_with("+xml")
        || content_type == "application/javascript"
        || content_type == "application/x-ndjson"
}

pub fn is_json_content_type(content_type: &str) -> bool {
    let content_type = content_type.split(';').next().unwrap_or("").trim();
    content_type == "application/json" || content_type.ends_with("+json")
}

pub fn should_skip_text_rewrite(text: &str) -> bool {
    has_signed_thinking_signature(text)
        || text.contains("\"encrypted_content\"")
        || text.contains("\"ciphertext\"")
        || is_binary_like_string(text)
}

fn visit_value<F>(value: &mut Value, allowed_context: bool, mutate: &mut F) -> Result<usize>
where
    F: FnMut(&str) -> Result<Option<String>>,
{
    match value {
        Value::String(text) if allowed_context && !should_skip_text_rewrite(text) => {
            if let Some(updated) = mutate(text)? {
                *text = updated;
                Ok(1)
            } else {
                Ok(0)
            }
        }
        Value::Array(values) => {
            let mut changed = 0usize;
            for value in values {
                changed += visit_value(value, allowed_context, mutate)?;
            }
            Ok(changed)
        }
        Value::Object(object) => visit_object(object, allowed_context, mutate),
        _ => Ok(0),
    }
}

fn visit_object<F>(
    object: &mut Map<String, Value>,
    allowed_context: bool,
    mutate: &mut F,
) -> Result<usize>
where
    F: FnMut(&str) -> Result<Option<String>>,
{
    if should_skip_object(object) {
        return Ok(0);
    }

    let mut changed = 0usize;
    for (key, value) in object {
        let key = key.to_ascii_lowercase();
        if should_skip_key(&key) {
            continue;
        }

        let child_allowed = allowed_context || is_content_key(&key) || is_tool_result_key(&key);
        changed += visit_value(value, child_allowed, mutate)?;
    }

    Ok(changed)
}

fn should_skip_object(object: &Map<String, Value>) -> bool {
    let is_signed_thinking = object
        .get("type")
        .and_then(Value::as_str)
        .map(|kind| kind == "thinking")
        .unwrap_or(false)
        && object.get("signature").is_some();

    is_signed_thinking
        || object.contains_key("encrypted_content")
        || object.contains_key("ciphertext")
}

fn should_skip_key(key: &str) -> bool {
    matches!(
        key,
        "authorization"
            | "api_key"
            | "x-api-key"
            | "x_api_key"
            | "token"
            | "access_token"
            | "refresh_token"
            | "session_id"
            | "model"
            | "stream"
            | "max_tokens"
            | "temperature"
            | "id"
            | "role"
            | "signature"
            | "encrypted_content"
            | "ciphertext"
            | "metadata"
            | "type"
            | "tool_call_id"
            | "tool_use_id"
            | "cache_control"
            | "name"
    ) || key.ends_with("_id")
        || key.ends_with("_token")
        || key.ends_with("_secret")
        || key.ends_with("_metadata")
        || key.ends_with("_signature")
}

fn is_content_key(key: &str) -> bool {
    matches!(
        key,
        "content"
            | "text"
            | "messages"
            | "system"
            | "input"
            | "instructions"
            | "description"
            | "prompt"
            | "thinking"
    ) || key.ends_with("_content")
}

fn is_tool_result_key(key: &str) -> bool {
    matches!(
        key,
        "tool_result" | "tool_results" | "output_text" | "results"
    ) || key.ends_with("_result")
        || key.ends_with("_results")
}

fn is_binary_like_string(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix("data:") {
        if let Some((meta, _payload)) = rest.split_once(',') {
            let mime = meta
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let is_base64 = meta
                .split(';')
                .any(|part| part.eq_ignore_ascii_case("base64"));
            if is_base64
                && !mime.starts_with("text/")
                && mime != "application/json"
                && !mime.ends_with("+json")
                && mime != "application/xml"
                && !mime.ends_with("+xml")
                && mime != "application/javascript"
            {
                return true;
            }
        }
    }

    if text.len() < 512 {
        return false;
    }

    let cleaned_len = text
        .bytes()
        .filter(|byte| !matches!(*byte, b'\r' | b'\n' | b'\t' | b' '))
        .count();
    cleaned_len >= 512
        && text.bytes().all(|byte| {
            matches!(
                byte,
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' | b'=' | b'\r' | b'\n' | b'\t' | b' '
            )
        })
}

fn has_signed_thinking_signature(text: &str) -> bool {
    (text.contains("\"type\":\"thinking\"") || text.contains("\"type\": \"thinking\""))
        && (text.contains("\"signature\":\"") || text.contains("\"signature\": \""))
}

#[cfg(test)]
mod tests {
    use super::mutate_json_text;
    use serde_json::json;

    fn replace_email(text: &str) -> anyhow::Result<Option<String>> {
        if text.contains("alice@example.com") {
            Ok(Some(text.replace("alice@example.com", "EMAIL_1")))
        } else {
            Ok(None)
        }
    }

    #[test]
    fn mutates_allowed_content_fields_and_skips_protocol_fields() {
        let mut value = json!({
            "messages": [
                {
                    "role": "user",
                    "content": "Email alice@example.com"
                }
            ],
            "authorization": "Bearer secret",
            "model": "gpt-4o",
            "metadata": {
                "content": "alice@example.com"
            }
        });

        let changed = mutate_json_text(&mut value, &mut replace_email).unwrap();

        assert_eq!(changed, 1);
        assert_eq!(value["messages"][0]["content"], "Email EMAIL_1");
        assert_eq!(value["authorization"], "Bearer secret");
        assert_eq!(value["model"], "gpt-4o");
        assert_eq!(value["metadata"]["content"], "alice@example.com");
    }

    #[test]
    fn skips_signed_thinking_blocks() {
        let mut value = json!({
            "content": [
                {
                    "type": "thinking",
                    "thinking": "alice@example.com",
                    "signature": "abc123"
                },
                {
                    "type": "text",
                    "text": "alice@example.com"
                }
            ]
        });

        let changed = mutate_json_text(&mut value, &mut replace_email).unwrap();

        assert_eq!(changed, 1);
        assert_eq!(value["content"][0]["thinking"], "alice@example.com");
        assert_eq!(value["content"][1]["text"], "EMAIL_1");
    }

    #[test]
    fn skips_non_text_data_urls() {
        let mut value = json!({
            "content": "data:application/pdf;base64,JVBERi0xLjcK",
        });

        let changed = mutate_json_text(&mut value, &mut replace_email).unwrap();

        assert_eq!(changed, 0);
        assert_eq!(value["content"], "data:application/pdf;base64,JVBERi0xLjcK");
    }
}
