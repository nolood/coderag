//! Fibonacci number calculation implementations

use std::collections::HashMap;

/// Calculate fibonacci number using recursion
pub fn fibonacci(n: u32) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

/// Calculate fibonacci number using iteration
pub fn fib_iterative(n: u32) -> u64 {
    let mut a = 0;
    let mut b = 1;

    for _ in 0..n {
        let temp = a + b;
        a = b;
        b = temp;
    }

    a
}

/// Calculate fibonacci with memoization
pub struct FibonacciMemo {
    cache: HashMap<u32, u64>,
}

impl FibonacciMemo {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub fn calculate_fibonacci(&mut self, n: u32) -> u64 {
        if let Some(&result) = self.cache.get(&n) {
            return result;
        }

        let result = match n {
            0 => 0,
            1 => 1,
            _ => self.calculate_fibonacci(n - 1) + self.calculate_fibonacci(n - 2),
        };

        self.cache.insert(n, result);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fibonacci() {
        assert_eq!(fibonacci(0), 0);
        assert_eq!(fibonacci(1), 1);
        assert_eq!(fibonacci(10), 55);
    }

    #[test]
    fn test_fib_iterative() {
        assert_eq!(fib_iterative(0), 0);
        assert_eq!(fib_iterative(1), 1);
        assert_eq!(fib_iterative(10), 55);
    }
}