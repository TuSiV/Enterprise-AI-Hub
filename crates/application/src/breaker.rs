//! Circuit Breaker（方案 §12.6）：按 Provider Target（provider × model）维度维护，
//! 同一 Provider 的某个模型故障不会使其他模型全部退出候选。

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerDecision {
    Allow,
    /// 熔断打开，直接跳过该 target 触发 failover
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Closed,
    Open { opened_at: Instant },
    HalfOpen { probe_in_flight: bool },
}

struct Breaker {
    state: State,
    consecutive_failures: u32,
}

const FAILURE_THRESHOLD: u32 = 5;
const OPEN_DURATION: Duration = Duration::from_secs(30);

impl Breaker {
    fn new() -> Self {
        Self {
            state: State::Closed,
            consecutive_failures: 0,
        }
    }
}

#[derive(Default)]
pub struct CircuitBreakerRegistry {
    breakers: Mutex<HashMap<(String, String), Breaker>>,
}

impl CircuitBreakerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn check(&self, provider_id: &str, model_key: &str) -> BreakerDecision {
        let mut breakers = self.breakers.lock().await;
        let key = (provider_id.to_string(), model_key.to_string());
        let breaker = breakers.entry(key).or_insert_with(Breaker::new);
        match breaker.state {
            State::Closed => BreakerDecision::Allow,
            State::Open { opened_at } => {
                if opened_at.elapsed() >= OPEN_DURATION {
                    breaker.state = State::HalfOpen { probe_in_flight: true };
                    BreakerDecision::Allow
                } else {
                    BreakerDecision::Open
                }
            }
            State::HalfOpen { probe_in_flight } => {
                if probe_in_flight {
                    BreakerDecision::Open
                } else {
                    breaker.state = State::HalfOpen { probe_in_flight: true };
                    BreakerDecision::Allow
                }
            }
        }
    }

    pub async fn record_success(&self, provider_id: &str, model_key: &str) {
        let mut breakers = self.breakers.lock().await;
        let key = (provider_id.to_string(), model_key.to_string());
        let breaker = breakers.entry(key).or_insert_with(Breaker::new);
        breaker.state = State::Closed;
        breaker.consecutive_failures = 0;
    }

    pub async fn record_failure(&self, provider_id: &str, model_key: &str) {
        let mut breakers = self.breakers.lock().await;
        let key = (provider_id.to_string(), model_key.to_string());
        let breaker = breakers.entry(key).or_insert_with(Breaker::new);
        breaker.consecutive_failures += 1;
        if breaker.consecutive_failures >= FAILURE_THRESHOLD {
            breaker.state = State::Open {
                opened_at: Instant::now(),
            };
        }
    }

    pub async fn snapshot(&self) -> Vec<(String, String, String, u32)> {
        let breakers = self.breakers.lock().await;
        breakers
            .iter()
            .map(|((provider, model), breaker)| {
                let state = match breaker.state {
                    State::Closed => "closed".to_string(),
                    State::Open { .. } => "open".to_string(),
                    State::HalfOpen { .. } => "half_open".to_string(),
                };
                (provider.clone(), model.clone(), state, breaker.consecutive_failures)
            })
            .collect()
    }

    pub async fn reset(&self) {
        self.breakers.lock().await.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn opens_after_threshold_and_recovers() {
        let registry = CircuitBreakerRegistry::new();
        for _ in 0..4 {
            registry.record_failure("p1", "m1").await;
            assert_eq!(registry.check("p1", "m1").await, BreakerDecision::Allow);
        }
        registry.record_failure("p1", "m1").await;
        assert_eq!(registry.check("p1", "m1").await, BreakerDecision::Open);
        // 其他 target 不受影响（§12.6）
        assert_eq!(registry.check("p1", "m2").await, BreakerDecision::Allow);
        // 成功后关闭
        registry.record_success("p1", "m1").await;
        assert_eq!(registry.check("p1", "m1").await, BreakerDecision::Allow);
    }
}
