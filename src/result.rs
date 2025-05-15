use reqwest::StatusCode;

pub struct RequestResult {
    pub code: StatusCode,
    pub duration_ms: u128,
}

impl RequestResult {
    pub fn new(code: StatusCode, duration_ms: u128) -> Self {
        Self { code, duration_ms }
    }
}

pub struct ExecutionResult {
    pub total_requests: usize,
    pub count_1xx: u32,
    pub count_2xx: u32,
    pub count_3xx: u32,
    pub count_4xx: u32,
    pub count_5xx: u32,
    pub count_timeout: u32,
    pub fastest: u128,
    pub slowest: u128,
}

impl ExecutionResult {
    pub fn new() -> Self {
        Self {
            total_requests: 0,
            count_1xx: 0,
            count_2xx: 0,
            count_3xx: 0,
            count_4xx: 0,
            count_5xx: 0,
            count_timeout: 0,
            fastest: u128::MAX,
            slowest: u128::MIN,
        }
    }

    pub fn init(mut self, results: Vec<RequestResult>) -> Self {
        self.total_requests = results.len();

        for result in results {
            self.fastest = std::cmp::min(self.fastest, result.duration_ms);
            self.slowest = std::cmp::max(self.slowest, result.duration_ms);

            if result.code.is_informational() {
                self.count_1xx += 1;
            } else if result.code.is_success() {
                self.count_2xx += 1;
            } else if result.code.is_redirection() {
                self.count_3xx += 1;
            } else if result.code.is_client_error() {
                self.count_4xx += 1;
            } else {
                self.count_5xx += 1;
            }
        }
        self
    }
}
