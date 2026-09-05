//! 结构化日志初始化（方案 §30）。日志默认不输出 Secret / Authorization / 完整 Prompt，
//! 该约束由各模块的日志调用约定保证，此处仅做 redaction 工具。

use tracing_subscriber::EnvFilter;

pub fn init(log_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .try_init();
}

/// 脱敏：用于日志中的 key/token 类字符串，仅保留前 6 位。
pub fn redact(value: &str) -> String {
    let len = value.chars().count();
    if len <= 6 {
        "***".to_string()
    } else {
        let prefix: String = value.chars().take(6).collect();
        format!("{prefix}***")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_long_values() {
        assert_eq!(redact("aih_live_abcdef_secret"), "aih_li***");
        assert_eq!(redact("short"), "***");
    }
}
