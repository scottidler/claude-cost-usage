use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
    pub cache_5m_write_per_mtok: f64,
    pub cache_1h_write_per_mtok: f64,
    pub cache_read_per_mtok: f64,
}

pub fn default_pricing_table() -> HashMap<String, ModelPricing> {
    let mut table = HashMap::new();

    // Opus 4.6 / 4.5
    for name in ["claude-opus-4-6", "claude-opus-4-5"] {
        table.insert(
            name.to_string(),
            ModelPricing {
                input_per_mtok: 5.0,
                output_per_mtok: 25.0,
                cache_5m_write_per_mtok: 6.25,
                cache_1h_write_per_mtok: 10.0,
                cache_read_per_mtok: 0.50,
            },
        );
    }

    // Opus 4.1 / 4
    for name in ["claude-opus-4-1", "claude-opus-4"] {
        table.insert(
            name.to_string(),
            ModelPricing {
                input_per_mtok: 15.0,
                output_per_mtok: 75.0,
                cache_5m_write_per_mtok: 18.75,
                cache_1h_write_per_mtok: 30.0,
                cache_read_per_mtok: 1.50,
            },
        );
    }

    // Sonnet 4.6 / 4.5 / 4
    for name in ["claude-sonnet-4-6", "claude-sonnet-4-5", "claude-sonnet-4"] {
        table.insert(
            name.to_string(),
            ModelPricing {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
                cache_5m_write_per_mtok: 3.75,
                cache_1h_write_per_mtok: 6.0,
                cache_read_per_mtok: 0.30,
            },
        );
    }

    // Haiku 4.5
    table.insert(
        "claude-haiku-4-5".to_string(),
        ModelPricing {
            input_per_mtok: 1.0,
            output_per_mtok: 5.0,
            cache_5m_write_per_mtok: 1.25,
            cache_1h_write_per_mtok: 2.0,
            cache_read_per_mtok: 0.10,
        },
    );

    table
}

/// Strip dated model ID suffix (e.g., `claude-opus-4-5-20251101` -> `claude-opus-4-5`)
pub fn normalize_model_id(model_id: &str) -> &str {
    // Check if the last segment is an 8-digit date
    if let Some(pos) = model_id.rfind('-') {
        let suffix = &model_id[pos + 1..];
        if suffix.len() == 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
            return &model_id[..pos];
        }
    }
    model_id
}

/// Calculate cost for a single assistant entry's token usage
pub fn calculate_cost(pricing: &ModelPricing, usage: &crate::parser::TokenUsage) -> f64 {
    let mtok = 1_000_000.0;
    (usage.input_tokens as f64 * pricing.input_per_mtok / mtok)
        + (usage.output_tokens as f64 * pricing.output_per_mtok / mtok)
        + (usage.cache_5m_write_tokens as f64 * pricing.cache_5m_write_per_mtok / mtok)
        + (usage.cache_1h_write_tokens as f64 * pricing.cache_1h_write_per_mtok / mtok)
        + (usage.cache_read_tokens as f64 * pricing.cache_read_per_mtok / mtok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::TokenUsage;

    #[test]
    fn test_normalize_model_id_with_date() {
        assert_eq!(normalize_model_id("claude-opus-4-5-20251101"), "claude-opus-4-5");
        assert_eq!(normalize_model_id("claude-haiku-4-5-20251001"), "claude-haiku-4-5");
    }

    #[test]
    fn test_normalize_model_id_without_date() {
        assert_eq!(normalize_model_id("claude-opus-4-6"), "claude-opus-4-6");
        assert_eq!(normalize_model_id("claude-sonnet-4"), "claude-sonnet-4");
    }

    #[test]
    fn test_pricing_table_has_all_models() {
        let table = default_pricing_table();
        assert!(table.contains_key("claude-opus-4-6"));
        assert!(table.contains_key("claude-opus-4-5"));
        assert!(table.contains_key("claude-opus-4-1"));
        assert!(table.contains_key("claude-opus-4"));
        assert!(table.contains_key("claude-sonnet-4-6"));
        assert!(table.contains_key("claude-sonnet-4-5"));
        assert!(table.contains_key("claude-sonnet-4"));
        assert!(table.contains_key("claude-haiku-4-5"));
    }

    #[test]
    fn test_calculate_cost_basic() {
        let pricing = ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
            cache_5m_write_per_mtok: 6.25,
            cache_1h_write_per_mtok: 10.0,
            cache_read_per_mtok: 0.50,
        };

        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
            cache_5m_write_tokens: 0,
            cache_1h_write_tokens: 0,
            cache_read_tokens: 0,
        };

        let cost = calculate_cost(&pricing, &usage);
        // 1M input * $5/M + 100K output * $25/M = $5 + $2.50 = $7.50
        assert!((cost - 7.50).abs() < 0.001);
    }

    #[test]
    fn test_calculate_cost_with_cache() {
        let pricing = ModelPricing {
            input_per_mtok: 5.0,
            output_per_mtok: 25.0,
            cache_5m_write_per_mtok: 6.25,
            cache_1h_write_per_mtok: 10.0,
            cache_read_per_mtok: 0.50,
        };

        let usage = TokenUsage {
            input_tokens: 3,
            output_tokens: 2,
            cache_5m_write_tokens: 1868,
            cache_1h_write_tokens: 0,
            cache_read_tokens: 21827,
        };

        let cost = calculate_cost(&pricing, &usage);
        // Small but nonzero
        assert!(cost > 0.0);
        assert!(cost < 0.1);
    }
}
