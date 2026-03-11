use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use eyre::Result;
use log::warn;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_5m_write_tokens: u64,
    pub cache_1h_write_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Debug, Clone)]
pub struct AssistantEntry {
    pub session_id: String,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub usage: TokenUsage,
}

// Serde structures for JSONL parsing - only the fields we need
#[derive(Deserialize)]
struct RawEntry {
    #[serde(rename = "type")]
    entry_type: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    timestamp: Option<String>,
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation: Option<RawCacheCreation>,
}

#[derive(Deserialize)]
struct RawCacheCreation {
    ephemeral_5m_input_tokens: Option<u64>,
    ephemeral_1h_input_tokens: Option<u64>,
}

/// Parse a JSONL file and yield assistant entries
pub fn parse_jsonl_file(path: &Path) -> Result<Vec<AssistantEntry>> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut entries = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                warn!("Error reading line from {}: {}", path.display(), e);
                continue;
            }
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Quick check before full parse
        if !line.contains("\"assistant\"") {
            continue;
        }

        match serde_json::from_str::<RawEntry>(line) {
            Ok(raw) => {
                if let Some(entry) = convert_raw_entry(raw) {
                    entries.push(entry);
                }
            }
            Err(e) => {
                warn!("Failed to parse JSON line in {}: {}", path.display(), e);
            }
        }
    }

    Ok(entries)
}

fn convert_raw_entry(raw: RawEntry) -> Option<AssistantEntry> {
    if raw.entry_type.as_deref() != Some("assistant") {
        return None;
    }

    let session_id = raw.session_id?;
    let timestamp_str = raw.timestamp?;
    let message = raw.message?;
    let model = message.model?;
    let usage = message.usage?;

    let timestamp = timestamp_str.parse::<DateTime<Utc>>().ok()?;

    let (cache_5m, cache_1h) = if let Some(cc) = &usage.cache_creation {
        (
            cc.ephemeral_5m_input_tokens.unwrap_or(0),
            cc.ephemeral_1h_input_tokens.unwrap_or(0),
        )
    } else {
        // No breakdown: treat all cache_creation_input_tokens as 5m
        (usage.cache_creation_input_tokens.unwrap_or(0), 0)
    };

    Some(AssistantEntry {
        session_id,
        timestamp,
        model,
        usage: TokenUsage {
            input_tokens: usage.input_tokens.unwrap_or(0),
            output_tokens: usage.output_tokens.unwrap_or(0),
            cache_5m_write_tokens: cache_5m,
            cache_1h_write_tokens: cache_1h,
            cache_read_tokens: usage.cache_read_input_tokens.unwrap_or(0),
        },
    })
}

/// Get the local date for a UTC timestamp
pub fn local_date(ts: &DateTime<Utc>) -> NaiveDate {
    let local = chrono::Local.from_utc_datetime(&ts.naive_utc());
    local.date_naive()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_jsonl_file(lines: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("create temp file");
        for line in lines {
            writeln!(file, "{}", line).expect("write line");
        }
        file
    }

    #[test]
    fn test_parse_assistant_entry() {
        let jsonl = make_jsonl_file(&[
            r#"{"type":"assistant","sessionId":"abc-123","timestamp":"2026-03-10T14:23:01.025Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":200,"cache_read_input_tokens":1000,"cache_creation":{"ephemeral_5m_input_tokens":200,"ephemeral_1h_input_tokens":0}}}}"#,
        ]);

        let entries = parse_jsonl_file(jsonl.path()).expect("parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model, "claude-opus-4-6");
        assert_eq!(entries[0].usage.input_tokens, 100);
        assert_eq!(entries[0].usage.output_tokens, 50);
        assert_eq!(entries[0].usage.cache_5m_write_tokens, 200);
        assert_eq!(entries[0].usage.cache_read_tokens, 1000);
    }

    #[test]
    fn test_skip_non_assistant_lines() {
        let jsonl = make_jsonl_file(&[
            r#"{"type":"system","content":"hello"}"#,
            r#"{"type":"user","content":"world"}"#,
            r#"{"type":"assistant","sessionId":"abc","timestamp":"2026-03-10T14:23:01.025Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":10,"output_tokens":5}}}"#,
        ]);

        let entries = parse_jsonl_file(jsonl.path()).expect("parse");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_skip_malformed_lines() {
        let jsonl = make_jsonl_file(&[
            r#"{"type":"assistant", this is broken json"#,
            r#"{"type":"assistant","sessionId":"abc","timestamp":"2026-03-10T14:23:01.025Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":10,"output_tokens":5}}}"#,
        ]);

        let entries = parse_jsonl_file(jsonl.path()).expect("parse");
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_cache_fallback_no_breakdown() {
        let jsonl = make_jsonl_file(&[
            r#"{"type":"assistant","sessionId":"abc","timestamp":"2026-03-10T14:23:01.025Z","message":{"model":"claude-opus-4-6","usage":{"input_tokens":10,"output_tokens":5,"cache_creation_input_tokens":500,"cache_read_input_tokens":1000}}}"#,
        ]);

        let entries = parse_jsonl_file(jsonl.path()).expect("parse");
        assert_eq!(entries.len(), 1);
        // No cache_creation breakdown, so all goes to 5m
        assert_eq!(entries[0].usage.cache_5m_write_tokens, 500);
        assert_eq!(entries[0].usage.cache_1h_write_tokens, 0);
    }

    #[test]
    fn test_local_date() {
        let ts: DateTime<Utc> = "2026-03-10T14:23:01.025Z".parse().expect("parse");
        let date = local_date(&ts);
        // This depends on the local timezone, but should be either Mar 10 or Mar 11
        assert!(date.month() == 3);
    }
}
