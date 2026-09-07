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

//! 预置服务商配置（按厂商分组）。

use std::sync::LazyLock;

/// 预置服务商条目（与 admin API 其它 DTO 一致，camelCase 序列化）
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderPreset {
    pub key: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub base_url: &'static str,
    pub default_model: &'static str,
}

/// 主流服务商预置配置库
pub static PRESETS: LazyLock<Vec<ProviderPreset>> = LazyLock::new(|| {
    vec![
        // OpenAI
        ProviderPreset {
            key: "openai",
            name: "OpenAI",
            kind: "openai",
            base_url: "https://api.openai.com/v1",
            default_model: "gpt-4o",
        },
        ProviderPreset {
            key: "openai-o1",
            name: "OpenAI (o-series)",
            kind: "openai",
            base_url: "https://api.openai.com/v1",
            default_model: "o3",
        },
        // Anthropic
        ProviderPreset {
            key: "anthropic",
            name: "Anthropic",
            kind: "anthropic",
            base_url: "https://api.anthropic.com",
            default_model: "claude-sonnet-4-6",
        },
        // Google Gemini
        ProviderPreset {
            key: "google",
            name: "Google Gemini",
            kind: "gemini",
            base_url: "https://generativelanguage.googleapis.com",
            default_model: "gemini-3-1-pro",
        },
        // DeepSeek
        ProviderPreset {
            key: "deepseek",
            name: "DeepSeek",
            kind: "openai_compatible",
            base_url: "https://api.deepseek.com/v1",
            default_model: "deepseek-v3",
        },
        // 智谱 GLM
        ProviderPreset {
            key: "zhipu",
            name: "智谱 GLM",
            kind: "openai_compatible",
            base_url: "https://open.bigmodel.cn/api/paas/v4",
            default_model: "glm-5",
        },
        // MiniMax
        ProviderPreset {
            key: "minimax",
            name: "MiniMax",
            kind: "openai_compatible",
            base_url: "https://api.minimax.chat/v1",
            default_model: "minimax-m3",
        },
        // Moonshot (Kimi)
        ProviderPreset {
            key: "moonshot",
            name: "Moonshot (Kimi)",
            kind: "openai_compatible",
            base_url: "https://api.moonshot.cn/v1",
            default_model: "moonshot-v1-128k",
        },
        // 通义千问
        ProviderPreset {
            key: "qwen",
            name: "通义千问 (Qwen)",
            kind: "openai_compatible",
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
            default_model: "qwen-max",
        },
        // 文心一言
        ProviderPreset {
            key: "wenxin",
            name: "文心一言 (ERNIE)",
            kind: "openai_compatible",
            base_url: "https://aip.baidubce.com/rpc/2.0/ai_custom/v1/wenxinworkshop",
            default_model: "ernie-4.0-8k",
        },
        // 讯飞星火
        ProviderPreset {
            key: "spark",
            name: "讯飞星火 (Spark)",
            kind: "openai_compatible",
            base_url: "https://spark-api-open.xf-yun.com/v1",
            default_model: "generalv3.5",
        },
        // Ollama (本地)
        ProviderPreset {
            key: "ollama",
            name: "Ollama (本地)",
            kind: "ollama",
            base_url: "http://127.0.0.1:11434/v1",
            default_model: "llama3.1:8b",
        },
        // Together AI
        ProviderPreset {
            key: "together",
            name: "Together AI",
            kind: "openai_compatible",
            base_url: "https://api.together.xyz/v1",
            default_model: "meta-llama/Llama-3.1-70B-Instruct-Turbo",
        },
        // Groq
        ProviderPreset {
            key: "groq",
            name: "Groq",
            kind: "openai_compatible",
            base_url: "https://api.groq.com/openai/v1",
            default_model: "llama-3.1-70b-versatile",
        },
    ]
});

/// 根据 key 查找预置服务商
pub fn find_preset(key: &str) -> Option<&ProviderPreset> {
    PRESETS.iter().find(|p| p.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_preset() {
        let openai = find_preset("openai").unwrap();
        assert_eq!(openai.name, "OpenAI");
        assert_eq!(openai.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_presets_not_empty() {
        assert!(!PRESETS.is_empty());
    }

    #[test]
    fn test_serializes_camel_case() {
        let json = serde_json::to_value(find_preset("openai").unwrap()).unwrap();
        assert!(json.get("baseUrl").is_some());
        assert!(json.get("defaultModel").is_some());
    }
}
