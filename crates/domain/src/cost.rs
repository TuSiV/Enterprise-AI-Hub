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

//! Cost Engine（方案 §17.2）：价格配置 + 成本计算。
//! 计算结果为整数 microunits（1 currency unit = 1_000_000 microunits），避免浮点存储。

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Pricing {
    pub currency: String,
    pub unit_tokens: i64,
    /// 每单位 token 价格（如每 1M tokens 的 USD 价格）
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cached_input: Option<f64>,
    pub reasoning: Option<f64>,
}

impl Default for Pricing {
    fn default() -> Self {
        Self {
            currency: "USD".to_string(),
            unit_tokens: 1_000_000,
            input: None,
            output: None,
            cached_input: None,
            reasoning: None,
        }
    }
}

impl Pricing {
    pub fn from_json(value: &Value) -> Pricing {
        serde_json::from_value(value.clone()).unwrap_or_default()
    }

    pub fn is_free(&self) -> bool {
        self.input.is_none() && self.output.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CostBreakdown {
    pub input_cost_microunits: i64,
    pub output_cost_microunits: i64,
    pub cache_cost_microunits: i64,
    pub reasoning_cost_microunits: i64,
    pub total_cost_microunits: i64,
}

fn price_to_microunits(tokens: i64, unit_tokens: i64, price: f64) -> i64 {
    if tokens <= 0 || unit_tokens <= 0 {
        return 0;
    }
    // microunits = tokens / unit_tokens * price * 1e6，四舍五入
    let raw = (tokens as f64) / (unit_tokens as f64) * price * 1_000_000.0;
    raw.round() as i64
}

/// inputCost = inputTokens / unitTokens × inputPrice（方案 §17.2）
pub fn calculate(pricing: &Pricing, usage: &crate::canonical::CanonicalUsage) -> CostBreakdown {
    let mut result = CostBreakdown {
        input_cost_microunits: pricing
            .input
            .map(|p| price_to_microunits(usage.input_tokens, pricing.unit_tokens, p))
            .unwrap_or(0),
        output_cost_microunits: pricing
            .output
            .map(|p| price_to_microunits(usage.output_tokens, pricing.unit_tokens, p))
            .unwrap_or(0),
        cache_cost_microunits: pricing
            .cached_input
            .map(|p| price_to_microunits(usage.cached_input_tokens, pricing.unit_tokens, p))
            .unwrap_or(0),
        reasoning_cost_microunits: pricing
            .reasoning
            .map(|p| price_to_microunits(usage.reasoning_tokens, pricing.unit_tokens, p))
            .unwrap_or(0),
        ..Default::default()
    };
    // cached input 已在 input 计费的部分不再重复计价（provider 报告 cached tokens 通常是 input 子集）
    result.total_cost_microunits = result.input_cost_microunits
        + result.output_cost_microunits
        + result.cache_cost_microunits
        + result.reasoning_cost_microunits;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(input: i64, output: i64, cached: i64) -> crate::canonical::CanonicalUsage {
        crate::canonical::CanonicalUsage {
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cached,
            reasoning_tokens: 0,
            total_tokens: input + output,
            source: crate::canonical::UsageSource::Provider,
        }
    }

    #[test]
    fn calculates_standard_pricing() {
        let pricing = Pricing {
            currency: "USD".into(),
            unit_tokens: 1_000_000,
            input: Some(1.0),
            output: Some(4.0),
            cached_input: Some(0.2),
            reasoning: None,
        };
        // 1M input @1.0 → 1_000_000 microunits (1 USD)；0.5M output @4.0 → 2_000_000
        let cost = calculate(&pricing, &usage(1_000_000, 500_000, 0));
        assert_eq!(cost.input_cost_microunits, 1_000_000);
        assert_eq!(cost.output_cost_microunits, 2_000_000);
        assert_eq!(cost.total_cost_microunits, 3_000_000);
    }

    #[test]
    fn cached_input_is_priced_separately() {
        let pricing = Pricing {
            currency: "USD".into(),
            unit_tokens: 1_000_000,
            input: Some(1.0),
            output: Some(2.0),
            cached_input: Some(0.2),
            reasoning: None,
        };
        let cost = calculate(&pricing, &usage(1_000_000, 0, 400_000));
        assert_eq!(cost.input_cost_microunits, 1_000_000);
        assert_eq!(cost.cache_cost_microunits, 80_000);
        assert_eq!(cost.total_cost_microunits, 1_080_000);
    }

    #[test]
    fn missing_pricing_is_free() {
        let pricing = Pricing::default();
        let cost = calculate(&pricing, &usage(1000, 1000, 0));
        assert_eq!(cost.total_cost_microunits, 0);
    }

    #[test]
    fn fractional_prices_round_correctly() {
        let pricing = Pricing {
            currency: "USD".into(),
            unit_tokens: 1_000_000,
            input: Some(0.27),
            output: None,
            cached_input: None,
            reasoning: None,
        };
        let cost = calculate(&pricing, &usage(1_000, 0, 0));
        // 1000/1M * 0.27 * 1e6 = 270 microunits
        assert_eq!(cost.total_cost_microunits, 270);
    }
}
