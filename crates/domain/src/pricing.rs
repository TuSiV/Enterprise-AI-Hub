// Copyright 2026 YONGZHE CHEN
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! 预置主流模型价格库（按厂商分组）。
//! 价格来源：各厂商官方定价页面，截至 2026 年 9 月。
//!
//! 使用方式：
//! ```rust
//! use aihub_domain::pricing::{find_preset, list_providers};
//!
//! if let Some(preset) = find_preset("gpt-4o") {
//!     println!("GPT-4o pricing: {:?}", preset.pricing);
//! }
//! ```

use std::sync::LazyLock;

use crate::cost::Pricing;

fn usd(input: Option<f64>, output: Option<f64>) -> Pricing {
    Pricing {
        currency: "USD".into(),
        unit_tokens: 1_000_000,
        input,
        output,
        cached_input: None,
        reasoning: None,
    }
}

fn usd_full(
    input: Option<f64>,
    output: Option<f64>,
    cached_input: Option<f64>,
    reasoning: Option<f64>,
) -> Pricing {
    Pricing {
        currency: "USD".into(),
        unit_tokens: 1_000_000,
        input,
        output,
        cached_input,
        reasoning,
    }
}

/// 预置价格条目（与 admin API 其它 DTO 一致，camelCase 序列化）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingPreset {
    pub model_key: &'static str,
    pub display_name: &'static str,
    pub provider: &'static str,
    pub pricing: Pricing,
    pub context_window: Option<i64>,
    pub max_output: Option<i64>,
}

macro_rules! preset {
    ($key:expr, $name:expr, $provider:expr, $pricing:expr) => {
        PricingPreset {
            model_key: $key,
            display_name: $name,
            provider: $provider,
            pricing: $pricing,
            context_window: None,
            max_output: None,
        }
    };
    ($key:expr, $name:expr, $provider:expr, $pricing:expr, $ctx:expr, $out:expr) => {
        PricingPreset {
            model_key: $key,
            display_name: $name,
            provider: $provider,
            pricing: $pricing,
            context_window: Some($ctx),
            max_output: Some($out),
        }
    };
}

/// 主流模型预置价格库
pub static PRESETS: LazyLock<Vec<PricingPreset>> = LazyLock::new(|| {
    vec![
        // ==================== OpenAI ====================
        preset!(
            "gpt-4o",
            "GPT-4o",
            "OpenAI",
            usd_full(Some(2.50), Some(10.00), Some(1.25), None),
            128_000,
            16_384
        ),
        preset!(
            "gpt-4o-mini",
            "GPT-4o mini",
            "OpenAI",
            usd_full(Some(0.15), Some(0.60), Some(0.075), None),
            128_000,
            16_384
        ),
        preset!(
            "o3",
            "o3",
            "OpenAI",
            usd_full(Some(2.00), Some(8.00), Some(0.50), Some(8.00)),
            200_000,
            100_000
        ),
        preset!(
            "o3-mini",
            "o3-mini",
            "OpenAI",
            usd_full(Some(1.10), Some(4.40), Some(0.275), Some(4.40)),
            200_000,
            100_000
        ),
        preset!(
            "o4-mini",
            "o4-mini",
            "OpenAI",
            usd_full(Some(0.55), Some(2.20), Some(0.1375), Some(2.20)),
            200_000,
            100_000
        ),
        preset!(
            "gpt-5.6-sol",
            "GPT-5.6 Sol",
            "OpenAI",
            usd_full(Some(5.00), Some(30.00), Some(0.50), None),
            1_000_000,
            100_000
        ),
        preset!(
            "gpt-5.6-terra",
            "GPT-5.6 Terra",
            "OpenAI",
            usd_full(Some(2.00), Some(12.00), Some(0.20), None),
            1_000_000,
            100_000
        ),
        preset!(
            "gpt-5.6-luna",
            "GPT-5.6 Luna",
            "OpenAI",
            usd_full(Some(0.20), Some(1.20), Some(0.02), None),
            1_000_000,
            100_000
        ),
        preset!(
            "text-embedding-3-small",
            "text-embedding-3-small",
            "OpenAI",
            usd(Some(0.02), None),
            8_191,
            0
        ),
        preset!(
            "text-embedding-3-large",
            "text-embedding-3-large",
            "OpenAI",
            usd(Some(0.13), None),
            8_191,
            0
        ),
        // ==================== Anthropic ====================
        preset!(
            "claude-opus-4-8",
            "Claude Opus 4.8",
            "Anthropic",
            usd_full(Some(5.00), Some(25.00), Some(0.50), None),
            1_000_000,
            128_000
        ),
        preset!(
            "claude-sonnet-4-6",
            "Claude Sonnet 4.6",
            "Anthropic",
            usd_full(Some(3.00), Some(15.00), Some(0.30), None),
            1_000_000,
            64_000
        ),
        preset!(
            "claude-haiku-4-5",
            "Claude Haiku 4.5",
            "Anthropic",
            usd_full(Some(1.00), Some(5.00), Some(0.10), None),
            200_000,
            64_000
        ),
        preset!(
            "claude-fable-5-1",
            "Claude Fable 5.1",
            "Anthropic",
            usd_full(Some(10.00), Some(50.00), Some(0.25), None),
            1_000_000,
            128_000
        ),
        preset!(
            "claude-sonnet-5",
            "Claude Sonnet 5",
            "Anthropic",
            usd_full(Some(2.00), Some(10.00), Some(0.20), None),
            1_000_000,
            128_000
        ),
        // ==================== Google Gemini ====================
        preset!(
            "gemini-3-1-pro",
            "Gemini 3.1 Pro",
            "Google",
            usd_full(Some(2.00), Some(12.00), Some(0.20), None),
            1_000_000,
            65_536
        ),
        preset!(
            "gemini-3-5-flash",
            "Gemini 3.5 Flash",
            "Google",
            usd_full(Some(1.50), Some(9.00), Some(0.15), None),
            1_000_000,
            65_536
        ),
        preset!(
            "gemini-3-flash",
            "Gemini 3 Flash",
            "Google",
            usd_full(Some(0.50), Some(3.00), Some(0.05), None),
            1_000_000,
            65_536
        ),
        preset!(
            "gemini-3-1-flash-lite",
            "Gemini 3.1 Flash-Lite",
            "Google",
            usd_full(Some(0.25), Some(1.50), Some(0.025), None),
            1_000_000,
            65_536
        ),
        preset!(
            "gemini-2-5-pro",
            "Gemini 2.5 Pro",
            "Google",
            usd_full(Some(1.25), Some(10.00), Some(0.125), None),
            1_000_000,
            65_536
        ),
        preset!(
            "gemini-2-5-flash",
            "Gemini 2.5 Flash",
            "Google",
            usd_full(Some(0.30), Some(2.50), Some(0.03), None),
            1_000_000,
            65_536
        ),
        preset!(
            "gemini-2-5-flash-lite",
            "Gemini 2.5 Flash-Lite",
            "Google",
            usd_full(Some(0.10), Some(0.40), Some(0.01), None),
            1_000_000,
            65_536
        ),
        // ==================== DeepSeek ====================
        preset!(
            "deepseek-v3",
            "DeepSeek V3",
            "DeepSeek",
            usd_full(Some(0.27), Some(1.10), Some(0.07), None),
            164_000,
            8_000
        ),
        preset!(
            "deepseek-r1",
            "DeepSeek R1",
            "DeepSeek",
            usd_full(Some(0.55), Some(2.19), Some(0.14), Some(2.19)),
            64_000,
            8_000
        ),
        // ==================== 智谱 GLM ====================
        preset!(
            "glm-5",
            "GLM-5",
            "Zhipu",
            usd(Some(1.00), Some(3.19)),
            128_000,
            4_000
        ),
        preset!(
            "glm-5-2",
            "GLM-5.2",
            "Zhipu",
            usd(Some(1.11), Some(3.89)),
            128_000,
            4_000
        ),
        preset!(
            "glm-4-7",
            "GLM-4.7",
            "Zhipu",
            usd(Some(0.60), Some(2.19)),
            203_000,
            4_000
        ),
        preset!(
            "glm-4-7-flash",
            "GLM-4.7-Flash",
            "Zhipu",
            usd(None, None),
            203_000,
            4_000
        ),
        preset!(
            "glm-4-5",
            "GLM-4.5",
            "Zhipu",
            usd(Some(0.11), Some(0.28)),
            128_000,
            4_000
        ),
        // ==================== MiniMax ====================
        preset!(
            "minimax-m3",
            "MiniMax-M3",
            "MiniMax",
            usd_full(Some(0.30), Some(1.20), Some(0.06), None),
            1_000_000,
            65_536
        ),
        preset!(
            "minimax-m2-1",
            "MiniMax-M2.1",
            "MiniMax",
            usd_full(Some(0.26), Some(1.00), Some(0.05), None),
            200_000,
            65_536
        ),
        preset!(
            "minimax-m2",
            "MiniMax-M2",
            "MiniMax",
            usd_full(Some(0.26), Some(1.00), Some(0.05), None),
            200_000,
            65_536
        ),
        preset!(
            "minimax-text-01",
            "MiniMax-Text-01",
            "MiniMax",
            usd_full(Some(0.20), Some(0.80), Some(0.02), None),
            4_000_000,
            65_536
        ),
        // ==================== Ollama / Local (免费参考) ====================
        preset!(
            "llama-3-1-8b",
            "Llama 3.1 8B",
            "Meta (Local)",
            usd(None, None),
            128_000,
            4_096
        ),
        preset!(
            "llama-3-1-70b",
            "Llama 3.1 70B",
            "Meta (Local)",
            usd(None, None),
            128_000,
            4_096
        ),
        preset!(
            "qwen2-5-72b",
            "Qwen2.5 72B",
            "Alibaba (Local)",
            usd(None, None),
            128_000,
            8_192
        ),
    ]
});

/// 根据模型 key 查找预置价格
pub fn find_preset(model_key: &str) -> Option<&PricingPreset> {
    PRESETS.iter().find(|p| p.model_key == model_key)
}

/// 根据厂商筛选预置价格
pub fn find_presets_by_provider(provider: &str) -> Vec<&PricingPreset> {
    PRESETS.iter().filter(|p| p.provider == provider).collect()
}

/// 获取所有厂商列表
pub fn list_providers() -> Vec<&'static str> {
    let mut providers: Vec<&str> = PRESETS.iter().map(|p| p.provider).collect();
    providers.sort();
    providers.dedup();
    providers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_preset() {
        let gpt4o = find_preset("gpt-4o").unwrap();
        assert_eq!(gpt4o.display_name, "GPT-4o");
        assert_eq!(gpt4o.pricing.input, Some(2.50));
        assert_eq!(gpt4o.pricing.output, Some(10.00));
    }

    #[test]
    fn test_find_presets_by_provider() {
        let openai = find_presets_by_provider("OpenAI");
        assert!(openai.len() >= 5);
    }

    #[test]
    fn test_list_providers() {
        let providers = list_providers();
        assert!(providers.contains(&"OpenAI"));
        assert!(providers.contains(&"Anthropic"));
        assert!(providers.contains(&"Google"));
        assert!(providers.contains(&"DeepSeek"));
        assert!(providers.contains(&"Zhipu"));
        assert!(providers.contains(&"MiniMax"));
    }

    #[test]
    fn test_free_models() {
        let flash = find_preset("glm-4-7-flash").unwrap();
        assert!(flash.pricing.is_free());
    }

    #[test]
    fn test_serializes_camel_case() {
        let json = serde_json::to_value(find_preset("gpt-4o").unwrap()).unwrap();
        assert!(json.get("modelKey").is_some());
        assert!(json.get("displayName").is_some());
        assert!(json.get("contextWindow").is_some());
        assert!(json.get("maxOutput").is_some());
        assert!(json["pricing"].get("unitTokens").is_some());
        assert!(json["pricing"].get("cachedInput").is_some());
    }
}
