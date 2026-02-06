use std::sync::{Mutex, MutexGuard};

/// A generic state machine wrapper ensuring thread-safe state transitions.
///
/// This struct wraps a state `S` in a `Mutex` and provides a `transition` method
/// to perform atomic state updates and return an action/result.
///
/// # Example
///
/// ```rust
/// use ahma_common::state_machine::StateMachine;
///
/// enum State {
///     Idle,
///     Running,
/// }
///
/// let machine = StateMachine::new(State::Idle);
///
/// let action = machine.transition(|state| {
///     match state {
///         State::Idle => {
///             *state = State::Running;
///             "Started"
///         }
///         State::Running => "Already running",
///     }
/// });
/// ```
#[derive(Debug)]
pub struct StateMachine<S> {
    state: Mutex<S>,
}

impl<S> StateMachine<S> {
    /// Creates a new `StateMachine` in the given initial state.
    pub fn new(initial_state: S) -> Self {
        Self {
            state: Mutex::new(initial_state),
        }
    }

    /// Access the underlying state directly via a MutexGuard.
    ///
    /// Use this for simple reads or checks that don't require complex transitions.
    /// For transitions, prefer `transition`.
    pub fn lock(&self) -> MutexGuard<'_, S> {
        self.state.lock().unwrap()
    }

    /// Perform an atomic transition on the state.
    ///
    /// The closure `f` is called with a mutable reference to the current state.
    /// The lock is held for the duration of the closure.
    ///
    /// Returns the result of the closure.
    pub fn transition<R>(&self, f: impl FnOnce(&mut S) -> R) -> R {
        let mut state = self.state.lock().unwrap();
        f(&mut *state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_transitions() {
        let machine = Arc::new(StateMachine::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let machine = machine.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    machine.transition(|state| {
                        *state += 1;
                    });
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(*machine.lock(), 1000);
    }

    #[test]
    fn test_complex_transition_logic() {
        enum State {
            A,
            B,
        }

        let machine = StateMachine::new(State::A);

        let result = machine.transition(|state| match state {
            State::A => {
                *state = State::B;
                "moved to B"
            }
            _ => "error",
        });

        assert_eq!(result, "moved to B");
        match *machine.lock() {
            State::B => (),
            _ => panic!("Wrong state"),
        }
    }
}
