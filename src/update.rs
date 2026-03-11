use eyre::{Context, Result};
use log::debug;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::config::Config;
use crate::pricing::ModelPricing;
use crate::table;

const EXTRACTION_PROMPT: &str = r#"Extract Claude model pricing from the following markdown and output YAML.

Output format - pricing map keyed by model ID, values in dollars per million tokens:

pricing:
  claude-opus-4-6:
    input_per_mtok: 5.0
    output_per_mtok: 25.0
    cache_5m_write_per_mtok: 6.25
    cache_1h_write_per_mtok: 10.0
    cache_read_per_mtok: 0.50
    input_per_mtok_above_200k: 10.0
    output_per_mtok_above_200k: 37.50
    cache_5m_write_per_mtok_above_200k: 12.50
    cache_1h_write_per_mtok_above_200k: 20.0
    cache_read_per_mtok_above_200k: 1.0

Rules:
- Include ALL Claude models listed on the page
- Use the base model ID without date suffixes (e.g., "claude-opus-4-6" not "claude-opus-4-6-20260101")
- If multiple versions share the same pricing, include each as a separate entry
- Cache write pricing: if a "5-minute" and "1-hour" cache write price are listed, use those.
  If only a single "cache write" price is listed, use it for cache_5m_write_per_mtok and
  set cache_1h_write_per_mtok to cache_5m_write_per_mtok * 1.6 (the standard ratio)
- Cache read pricing: use the listed cache read price
- Long context pricing (>200K input tokens): if the page lists higher rates for extended/long
  context, include them as the _above_200k fields. If a model has no long context pricing,
  omit the _above_200k fields entirely.
- Output ONLY the YAML block, no explanations or markdown fences
"#;

const JINA_URL: &str = "https://r.jina.ai/https://docs.anthropic.com/en/docs/about-claude/models";

/// Run `ccu pricing --update` - fetch pricing, extract via LLM, write config
pub fn run(from: Option<&PathBuf>) -> Result<()> {
    debug!("update::run: from={:?}", from);

    let markdown = match from {
        Some(path) => {
            eprintln!("Reading pricing from: {}", path.display());
            fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?
        }
        None => {
            eprintln!("Fetching pricing from Anthropic docs...");
            fetch_markdown()?
        }
    };

    eprintln!("Extracting pricing via claude...");
    let yaml_output = extract_pricing(&markdown)?;
    let yaml_clean = strip_code_fences(&yaml_output);

    let new_pricing: PricingOnly = serde_yaml::from_str(&yaml_clean).context("Failed to parse LLM output as YAML")?;

    validate_pricing(&new_pricing.pricing)?;

    // Load existing config to preserve non-pricing fields, or start fresh
    let config_path = config_path()?;
    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path).context("Failed to read existing config")?;
        serde_yaml::from_str::<Config>(&content).unwrap_or_default()
    } else {
        Config::default()
    };

    // Show diff if updating existing config
    if !config.pricing.is_empty() {
        show_diff(&config.pricing, &new_pricing.pricing);
    }

    config.pricing = new_pricing.pricing;

    // Write config
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    let yaml = serde_yaml::to_string(&config).context("Failed to serialize config")?;
    fs::write(&config_path, &yaml).with_context(|| format!("Failed to write config to {}", config_path.display()))?;

    eprintln!("Wrote config to: {}", config_path.display());
    print_summary(&config.pricing);

    Ok(())
}

/// Display current pricing table from config
pub fn show(config: &Config) -> Result<()> {
    debug!("update::show: model_count={}", config.pricing.len());

    if config.pricing.is_empty() {
        eyre::bail!("No pricing data in config. Run `ccu pricing --update` to populate.");
    }

    println!("Current pricing (per million tokens):\n");

    let mut models: Vec<_> = config.pricing.iter().collect();
    models.sort_by_key(|(name, _)| (*name).clone());

    let mut rows: Vec<Vec<String>> = Vec::new();
    for (name, p) in &models {
        rows.push(vec![
            name.to_string(),
            format!("${:.2}", p.input_per_mtok),
            format!("${:.2}", p.output_per_mtok),
            format!("${:.2}", p.cache_5m_write_per_mtok),
            format!("${:.2}", p.cache_1h_write_per_mtok),
            format!("${:.2}", p.cache_read_per_mtok),
        ]);
        if p.input_per_mtok_above_200k.is_some() {
            rows.push(vec![
                format!("  (>200K)"),
                format!("${:.2}", p.input_per_mtok_above_200k.unwrap_or(0.0)),
                format!("${:.2}", p.output_per_mtok_above_200k.unwrap_or(0.0)),
                format!("${:.2}", p.cache_5m_write_per_mtok_above_200k.unwrap_or(0.0)),
                format!("${:.2}", p.cache_1h_write_per_mtok_above_200k.unwrap_or(0.0)),
                format!("${:.2}", p.cache_read_per_mtok_above_200k.unwrap_or(0.0)),
            ]);
        }
    }

    println!(
        "{}",
        table::build(
            &["Model", "Input", "Output", "Cache5mW", "Cache1hW", "CacheR"],
            rows,
            &[1, 2, 3, 4, 5],
        )
    );

    Ok(())
}

/// Fetch markdown from jina.ai reader
fn fetch_markdown() -> Result<String> {
    debug!("fetch_markdown: url={}", JINA_URL);

    let output = Command::new("curl")
        .args(["-sS", "--max-time", "30", JINA_URL])
        .output()
        .context("Failed to run curl - is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eyre::bail!("curl failed: {}", stderr);
    }

    let body = String::from_utf8(output.stdout).context("Invalid UTF-8 from curl")?;
    if body.is_empty() {
        eyre::bail!("Empty response from jina.ai");
    }

    Ok(body)
}

/// Extract pricing by spawning `claude -p`
fn extract_pricing(markdown: &str) -> Result<String> {
    debug!("extract_pricing: markdown_length={}", markdown.len());

    let prompt = format!("{}\n\n---\n\n{}", EXTRACTION_PROMPT, markdown);

    let output = Command::new("claude")
        .args(["-p", &prompt])
        .output()
        .context("Failed to run `claude` - is Claude Code CLI installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eyre::bail!("claude -p failed: {}", stderr);
    }

    let result = String::from_utf8(output.stdout).context("Invalid UTF-8 from claude")?;
    if result.trim().is_empty() {
        eyre::bail!("Empty output from claude -p");
    }

    Ok(result)
}

/// Strip markdown code fences if present
fn strip_code_fences(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```yaml")
        && let Some(inner) = rest.strip_suffix("```")
    {
        return inner.trim().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("```yml")
        && let Some(inner) = rest.strip_suffix("```")
    {
        return inner.trim().to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("```")
        && let Some(inner) = rest.strip_suffix("```")
    {
        return inner.trim().to_string();
    }
    trimmed.to_string()
}

/// Validate extracted pricing data
fn validate_pricing(pricing: &HashMap<String, ModelPricing>) -> Result<()> {
    if pricing.is_empty() {
        eyre::bail!("No model pricing entries found in LLM output");
    }

    for (model, p) in pricing {
        if p.input_per_mtok <= 0.0
            || p.output_per_mtok <= 0.0
            || p.cache_5m_write_per_mtok <= 0.0
            || p.cache_1h_write_per_mtok <= 0.0
            || p.cache_read_per_mtok <= 0.0
        {
            eyre::bail!("Model '{}' has non-positive pricing values", model);
        }
    }

    // Warn if known families are missing
    let known_families = ["opus", "sonnet", "haiku"];
    for family in &known_families {
        if !pricing.keys().any(|k| k.contains(family)) {
            eprintln!("Warning: no '{}' model found in extracted pricing", family);
        }
    }

    Ok(())
}

/// Get the config file path
pub fn config_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or_else(|| eyre::eyre!("Cannot determine config directory"))?;
    Ok(config_dir.join("ccu").join("ccu.yml"))
}

/// Show a diff of pricing changes
fn show_diff(old: &HashMap<String, ModelPricing>, new: &HashMap<String, ModelPricing>) {
    eprintln!("\nPricing changes:");
    eprintln!("{}", "-".repeat(60));

    // Models removed
    for name in old.keys() {
        if !new.contains_key(name) {
            eprintln!("  - Removed: {}", name);
        }
    }

    // Models added or changed
    let mut names: Vec<_> = new.keys().collect();
    names.sort();
    for name in names {
        let new_p = &new[name];
        match old.get(name) {
            None => {
                eprintln!(
                    "  + Added: {} (in=${}, out=${})",
                    name, new_p.input_per_mtok, new_p.output_per_mtok
                );
            }
            Some(old_p) => {
                if !pricing_eq(old_p, new_p) {
                    eprintln!("  ~ Changed: {}", name);
                    diff_field("    input", old_p.input_per_mtok, new_p.input_per_mtok);
                    diff_field("    output", old_p.output_per_mtok, new_p.output_per_mtok);
                    diff_field(
                        "    cache_5m_write",
                        old_p.cache_5m_write_per_mtok,
                        new_p.cache_5m_write_per_mtok,
                    );
                    diff_field(
                        "    cache_1h_write",
                        old_p.cache_1h_write_per_mtok,
                        new_p.cache_1h_write_per_mtok,
                    );
                    diff_field("    cache_read", old_p.cache_read_per_mtok, new_p.cache_read_per_mtok);
                }
            }
        }
    }

    eprintln!("{}", "-".repeat(60));
}

fn opt_eq(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => (x - y).abs() < f64::EPSILON,
        (None, None) => true,
        _ => false,
    }
}

fn pricing_eq(a: &ModelPricing, b: &ModelPricing) -> bool {
    (a.input_per_mtok - b.input_per_mtok).abs() < f64::EPSILON
        && (a.output_per_mtok - b.output_per_mtok).abs() < f64::EPSILON
        && (a.cache_5m_write_per_mtok - b.cache_5m_write_per_mtok).abs() < f64::EPSILON
        && (a.cache_1h_write_per_mtok - b.cache_1h_write_per_mtok).abs() < f64::EPSILON
        && (a.cache_read_per_mtok - b.cache_read_per_mtok).abs() < f64::EPSILON
        && opt_eq(a.input_per_mtok_above_200k, b.input_per_mtok_above_200k)
        && opt_eq(a.output_per_mtok_above_200k, b.output_per_mtok_above_200k)
        && opt_eq(
            a.cache_5m_write_per_mtok_above_200k,
            b.cache_5m_write_per_mtok_above_200k,
        )
        && opt_eq(
            a.cache_1h_write_per_mtok_above_200k,
            b.cache_1h_write_per_mtok_above_200k,
        )
        && opt_eq(a.cache_read_per_mtok_above_200k, b.cache_read_per_mtok_above_200k)
}

fn diff_field(label: &str, old: f64, new: f64) {
    if (old - new).abs() > f64::EPSILON {
        eprintln!("{}: ${:.2} -> ${:.2}", label, old, new);
    }
}

/// Print summary of extracted models
fn print_summary(pricing: &HashMap<String, ModelPricing>) {
    let mut models: Vec<_> = pricing.keys().collect();
    models.sort();
    eprintln!("\nExtracted {} models:", models.len());
    for model in &models {
        eprintln!("  - {}", model);
    }
}

/// Helper struct for parsing just the pricing section from LLM output
#[derive(Debug, serde::Deserialize)]
struct PricingOnly {
    pricing: HashMap<String, ModelPricing>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_code_fences_yaml() {
        let input = "```yaml\npricing:\n  model: {}\n```";
        assert_eq!(strip_code_fences(input), "pricing:\n  model: {}");
    }

    #[test]
    fn test_strip_code_fences_plain() {
        let input = "```\npricing:\n  model: {}\n```";
        assert_eq!(strip_code_fences(input), "pricing:\n  model: {}");
    }

    #[test]
    fn test_strip_code_fences_none() {
        let input = "pricing:\n  model: {}";
        assert_eq!(strip_code_fences(input), "pricing:\n  model: {}");
    }

    #[test]
    fn test_validate_pricing_empty() {
        let pricing = HashMap::new();
        assert!(validate_pricing(&pricing).is_err());
    }

    fn test_pricing(input: f64, output: f64, c5m: f64, c1h: f64, cr: f64) -> ModelPricing {
        ModelPricing {
            input_per_mtok: input,
            output_per_mtok: output,
            cache_5m_write_per_mtok: c5m,
            cache_1h_write_per_mtok: c1h,
            cache_read_per_mtok: cr,
            input_per_mtok_above_200k: None,
            output_per_mtok_above_200k: None,
            cache_5m_write_per_mtok_above_200k: None,
            cache_1h_write_per_mtok_above_200k: None,
            cache_read_per_mtok_above_200k: None,
        }
    }

    #[test]
    fn test_validate_pricing_negative_value() {
        let mut pricing = HashMap::new();
        pricing.insert(
            "claude-opus-4-6".to_string(),
            test_pricing(-1.0, 25.0, 6.25, 10.0, 0.50),
        );
        assert!(validate_pricing(&pricing).is_err());
    }

    #[test]
    fn test_validate_pricing_valid() {
        let mut pricing = HashMap::new();
        pricing.insert("claude-opus-4-6".to_string(), test_pricing(5.0, 25.0, 6.25, 10.0, 0.50));
        pricing.insert(
            "claude-sonnet-4-6".to_string(),
            test_pricing(3.0, 15.0, 3.75, 6.0, 0.30),
        );
        pricing.insert("claude-haiku-4-5".to_string(), test_pricing(1.0, 5.0, 1.25, 2.0, 0.10));
        assert!(validate_pricing(&pricing).is_ok());
    }

    #[test]
    fn test_validate_pricing_zero_value() {
        let mut pricing = HashMap::new();
        pricing.insert("claude-opus-4-6".to_string(), test_pricing(5.0, 0.0, 6.25, 10.0, 0.50));
        assert!(validate_pricing(&pricing).is_err());
    }

    #[test]
    fn test_pricing_eq_identical() {
        let p = test_pricing(5.0, 25.0, 6.25, 10.0, 0.50);
        assert!(pricing_eq(&p, &p));
    }

    #[test]
    fn test_pricing_eq_different() {
        let a = test_pricing(5.0, 25.0, 6.25, 10.0, 0.50);
        let b = test_pricing(3.0, 15.0, 3.75, 6.0, 0.30);
        assert!(!pricing_eq(&a, &b));
    }

    #[test]
    fn test_strip_code_fences_yml() {
        let input = "```yml\npricing:\n  model: {}\n```";
        assert_eq!(strip_code_fences(input), "pricing:\n  model: {}");
    }

    #[test]
    fn test_config_path() {
        let path = config_path().expect("should get config path");
        assert!(path.to_string_lossy().contains("ccu"));
        assert!(path.to_string_lossy().ends_with("ccu.yml"));
    }
}
