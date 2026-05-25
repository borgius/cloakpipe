//! Response rehydration — replaces pseudo-tokens back with original values.
//!
//! Handles both complete responses and SSE streaming chunks.

use crate::{vault::Vault, RehydratedText};
use anyhow::Result;
use std::collections::HashMap;

pub struct Rehydrator;

impl Rehydrator {
    /// Rehydrate a complete text response, replacing pseudo-tokens with originals.
    pub fn rehydrate(text: &str, vault: &Vault) -> Result<RehydratedText> {
        let mappings = vault.reverse_mappings();
        Self::rehydrate_from_mappings(text, &mappings)
    }

    /// Rehydrate text from exported reverse mappings such as `vault-mappings.json`.
    pub fn rehydrate_from_mappings(
        text: &str,
        mappings: &HashMap<String, String>,
    ) -> Result<RehydratedText> {
        let mut result = text.to_string();
        let mut count = 0;

        // Sort mappings by token length (longest first) to avoid partial matches.
        // e.g., "ORG_12" should be replaced before "ORG_1"
        let mut sorted_mappings: Vec<_> = mappings.iter().collect();
        sorted_mappings.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));

        for (token, original) in sorted_mappings {
            let (updated, replaced) = replace_mapping_value(&result, token, original);
            if replaced {
                result = updated;
                count += 1;
            }
        }

        Ok(RehydratedText {
            text: result,
            rehydrated_count: count,
        })
    }

    /// Rehydrate a single SSE streaming chunk.
    /// Uses a token buffer to handle pseudo-tokens split across chunks.
    pub fn rehydrate_chunk(
        chunk: &str,
        buffer: &mut String,
        vault: &Vault,
    ) -> Result<(String, bool)> {
        buffer.push_str(chunk);

        // Check if buffer contains a complete pseudo-token pattern
        // Pattern: CATEGORY_DIGITS (e.g., ORG_7, AMOUNT_12, PERSON_5)
        let token_pattern = regex::Regex::new(r"[A-Z]+_\d+")?;

        if let Some(mat) = token_pattern.find(buffer) {
            // Check if the match is at the end (might be incomplete)
            if mat.end() == buffer.len() && !chunk.ends_with(' ') && !chunk.ends_with('\n') {
                // Token might continue in next chunk — hold buffer
                return Ok((String::new(), false));
            }

            // Complete token found — rehydrate it
            let token = mat.as_str();
            if let Some(original) = vault.lookup(token) {
                let rehydrated = buffer.replace(token, original);
                buffer.clear();
                return Ok((rehydrated, true));
            }
        }

        // No token pattern found — flush the buffer
        let output = buffer.clone();
        buffer.clear();
        Ok((output, false))
    }
}

fn replace_mapping_value(text: &str, token: &str, original: &str) -> (String, bool) {
    if token.is_empty() {
        return (text.to_string(), false);
    }

    let mut result = String::with_capacity(text.len());
    let mut last_end = 0usize;
    let mut replaced = false;

    for (start, _) in text.match_indices(token) {
        let end = start + token.len();
        if !has_token_boundary(text, token, start, end) {
            continue;
        }
        result.push_str(&text[last_end..start]);
        result.push_str(original);
        last_end = end;
        replaced = true;
    }

    if !replaced {
        return (text.to_string(), false);
    }

    result.push_str(&text[last_end..]);
    (result, true)
}

fn has_token_boundary(text: &str, token: &str, start: usize, end: usize) -> bool {
    let starts_with_word = token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric());
    let ends_with_word = token
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_alphanumeric());

    let before_ok = !starts_with_word
        || text[..start]
            .chars()
            .next_back()
            .map_or(true, |c| !c.is_ascii_alphanumeric());
    let after_ok = !ends_with_word
        || text[end..]
            .chars()
            .next()
            .map_or(true, |c| !c.is_ascii_alphanumeric());

    before_ok && after_ok
}

#[cfg(test)]
mod tests {
    use super::Rehydrator;
    use std::collections::HashMap;

    #[test]
    fn rehydrate_from_mappings_restores_exported_similar_values() {
        let mappings = HashMap::from([
            ("Harper Wright".to_string(), "Avery Collins".to_string()),
            ("07/22/2027".to_string(), "03/14/2026".to_string()),
            ("28".to_string(), "42".to_string()),
        ]);

        let restored = Rehydrator::rehydrate_from_mappings(
            "Patient: Harper Wright\nAge: 28\nNext review date: 07/22/2027\nID: ID-2800",
            &mappings,
        )
        .unwrap();

        assert!(restored.text.contains("Patient: Avery Collins"));
        assert!(restored.text.contains("Age: 42"));
        assert!(restored.text.contains("Next review date: 03/14/2026"));
        assert!(restored.text.contains("ID: ID-2800"));
    }
}
