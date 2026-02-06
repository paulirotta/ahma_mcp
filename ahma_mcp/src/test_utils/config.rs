use std::time::Duration;

pub struct TestConfig {
    pub default_timeout: Duration,
    pub notification_timeout: Duration,
    pub heavy_operation_timeout: Duration,
    pub quick_timeout: Duration,
    pub max_concurrent_tasks: usize,
    pub binary_cache_enabled: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            notification_timeout: Duration::from_secs(10),
            heavy_operation_timeout: Duration::from_secs(120),
            quick_timeout: Duration::from_secs(5),
            max_concurrent_tasks: 20,
            binary_cache_enabled: true,
        }
    }
}

pub fn default_config() -> TestConfig {
    TestConfig::default()
}
