//! Simple test to verify test discovery is working
use ahma_core::utils::logging::init_test_logging;

#[tokio::test]
async fn test_simple_discovery() {
    init_test_logging();
    assert_eq!(1 + 1, 2);
}

#[test]
fn test_sync_discovery() {
    init_test_logging();
    assert_eq!(2 + 2, 4);
}
