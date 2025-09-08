//! Simple test to verify test discovery is working

#[tokio::test]
async fn test_simple_discovery() {
    assert_eq!(1 + 1, 2);
}

#[test]
fn test_sync_discovery() {
    assert_eq!(2 + 2, 4);
}
