#[cfg(test)]
mod logging_tests {
    use ahma_core::utils::logging::init_logging;
    use std::sync::{Arc, Barrier};
    use std::thread;

    #[test]
    fn test_init_logging_multiple_times() {
        // This test ensures that calling init_logging multiple times does not cause a panic.
        // The Once::new() in the init_logging function should prevent re-initialization.
        assert!(init_logging("info", false).is_ok());
        assert!(init_logging("info", false).is_ok());
    }

    #[test]
    fn test_init_logging_concurrently() {
        // This test ensures that calling init_logging concurrently from multiple threads
        // does not cause a panic or race conditions.
        let num_threads = 10;
        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = vec![];

        for _ in 0..num_threads {
            let c = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                c.wait();
                assert!(init_logging("info", false).is_ok());
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }
}
