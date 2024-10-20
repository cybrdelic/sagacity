use std::sync::Arc;
use tokio::sync::Mutex;

struct TokenManager {
    // Token limits
    max_requests_per_minute: usize,
    max_tokens_per_minute: usize,
    max_tokens_per_day: usize,

    // Token usage
    current_requests_minute: usize,
    current_tokens_minute: usize,
    current_tokens_day: usize,

    // Mutex for thread-safe access
    mutex: Mutex<()>,
}

impl TokenManager {
    fn new(model: &str) -> Self {
        // Define token limits based on the model
        let (max_requests_per_minute, max_tokens_per_minute, max_tokens_per_day) = match model {
            "Claude 3.5 Sonnet" => (1000, 80_000, 2_500_000),
            "Claude 3 Opus" => (1000, 40_000, 2_500_000),
            "Claude 3 Sonnet" => (1000, 80_000, 2_500_000),
            "Claude 3 Haiku" => (1000, 100_000, 25_000_000),
            _ => (1000, 80_000, 2_500_000),
        };

        TokenManager {
            max_requests_per_minute,
            max_tokens_per_minute,
            max_tokens_per_day,
            current_requests_minute: 0,
            current_tokens_minute: 0,
            current_tokens_day: 0,
            mutex: Mutex::new(()),
        }
    }

    async fn can_proceed(&mut self, tokens: usize) -> bool {
        let _lock = self.mutex.lock().await;

        // Check daily limit
        if self.current_tokens_day + tokens > self.max_tokens_per_day {
            return false;
        }

        // Check per-minute token limit
        if self.current_tokens_minute + tokens > self.max_tokens_per_minute {
            return false;
        }

        // Check per-minute request limit
        if self.current_requests_minute + 1 > self.max_requests_per_minute {
            return false;
        }

        // Update token counts
        self.current_tokens_day += tokens;
        self.current_tokens_minute += tokens;
        self.current_requests_minute += 1;

        true
    }

    async fn reset_minute(&mut self) {
        let _lock = self.mutex.lock().await;
        self.current_requests_minute = 0;
        self.current_tokens_minute = 0;
    }

    async fn reset_day(&mut self) {
        let _lock = self.mutex.lock().await;
        self.current_tokens_day = 0;
    }
}
