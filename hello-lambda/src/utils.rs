use rand::Rng;
use std::time::Duration;

/// Returns true 2% of the time (panic simulation)
pub fn should_panic() -> bool {
    let mut rng = rand::thread_rng();
    rng.gen_range(1..=100) <= 10
}

/// Returns true 8% of the time (error simulation)
pub fn should_error() -> bool {
    let mut rng = rand::thread_rng();
    rng.gen_range(1..=100) <= 25
}

/// Returns a random delay between 1 and 3 seconds
pub fn get_random_delay() -> Duration {
    let mut rng = rand::thread_rng();
    let delay_ms = rng.gen_range(1000..=3000); // 1-3 seconds in milliseconds
    Duration::from_millis(delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_delay_range() {
        for _ in 0..100 {
            let delay = get_random_delay();
            assert!(delay >= Duration::from_millis(1000));
            assert!(delay <= Duration::from_millis(3000));
        }
    }

    #[test]
    fn test_probability_functions_return_bool() {
        // Just test that they return booleans and don't panic
        for _ in 0..50 {
            let _ = should_panic();
            let _ = should_error();
        }
    }
}