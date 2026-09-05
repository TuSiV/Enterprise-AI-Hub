//! 内存 Rate Limiter（方案 §15.2）：单实例 Token/滑动窗口；
//! 多实例分布式限流属于后续 Scale Adapter（方案 §44 Stage F）。

use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimiterDecision {
    Allowed,
    RateLimited { retry_after_secs: u64 },
    DailyQuotaExceeded,
}

struct AppState {
    minute_events: VecDeque<Instant>,
    daily_day: chrono::NaiveDate,
    daily_count: u64,
}

pub struct RateLimiter {
    states: Mutex<HashMap<String, AppState>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
        }
    }

    /// rpm：每分钟请求数；daily_requests：每日请求数。None 表示不限制。
    pub async fn check(
        &self,
        subject: &str,
        rpm: Option<i64>,
        daily_requests: Option<i64>,
    ) -> LimiterDecision {
        let mut states = self.states.lock().await;
        let today = chrono::Utc::now().date_naive();
        let state = states.entry(subject.to_string()).or_insert(AppState {
            minute_events: VecDeque::new(),
            daily_day: today,
            daily_count: 0,
        });
        if state.daily_day != today {
            state.daily_day = today;
            state.daily_count = 0;
        }

        if let Some(daily) = daily_requests {
            if daily >= 0 && state.daily_count >= daily as u64 {
                return LimiterDecision::DailyQuotaExceeded;
            }
        }

        if let Some(rpm) = rpm {
            let now = Instant::now();
            let window = std::time::Duration::from_secs(60);
            while let Some(front) = state.minute_events.front() {
                if now.duration_since(*front) > window {
                    state.minute_events.pop_front();
                } else {
                    break;
                }
            }
            if state.minute_events.len() >= rpm.max(0) as usize {
                return LimiterDecision::RateLimited { retry_after_secs: 60 };
            }
            state.minute_events.push_back(now);
        }

        state.daily_count += 1;
        LimiterDecision::Allowed
    }

    /// 请求失败时回退计数，避免失败请求占用配额。
    pub async fn refund(&self, subject: &str) {
        let mut states = self.states.lock().await;
        if let Some(state) = states.get_mut(subject) {
            state.minute_events.pop_back();
            state.daily_count = state.daily_count.saturating_sub(1);
        }
    }

    pub async fn reset(&self, subject: &str) {
        self.states.lock().await.remove(subject);
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rpm_limit_blocks_excess() {
        let limiter = RateLimiter::new();
        for _ in 0..3 {
            assert_eq!(
                limiter.check("app-1", Some(3), None).await,
                LimiterDecision::Allowed
            );
        }
        assert!(matches!(
            limiter.check("app-1", Some(3), None).await,
            LimiterDecision::RateLimited { .. }
        ));
        // 其他 subject 不受影响
        assert_eq!(limiter.check("app-2", Some(3), None).await, LimiterDecision::Allowed);
    }

    #[tokio::test]
    async fn daily_quota_blocks() {
        let limiter = RateLimiter::new();
        assert_eq!(limiter.check("app-1", None, Some(1)).await, LimiterDecision::Allowed);
        assert_eq!(
            limiter.check("app-1", None, Some(1)).await,
            LimiterDecision::DailyQuotaExceeded
        );
    }

    #[tokio::test]
    async fn no_limits_always_allowed() {
        let limiter = RateLimiter::new();
        for _ in 0..100 {
            assert_eq!(limiter.check("app-1", None, None).await, LimiterDecision::Allowed);
        }
    }
}
