use std::collections::{HashSet, VecDeque};
use std::hash::Hash;

/// A data structure that combines a queue (FIFO) with set semantics (no duplicates).
/// Elements can only be removed from the front and added to the back.
/// Duplicate insertions are prevented.
#[derive(Debug, Clone)]
pub struct QueueSet<T>
where
    T: Hash + Eq + Clone,
{
    queue: VecDeque<T>,
    set: HashSet<T>,
}

impl<T> QueueSet<T>
where
    T: Hash + Eq + Clone,
{
    /// Creates a new empty QueueSet.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            set: HashSet::new(),
        }
    }

    /// Creates a new QueueSet with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            set: HashSet::with_capacity(capacity),
        }
    }

    /// Adds an element to the back of the queue.
    /// Returns `true` if the element was newly inserted, `false` if it was already present.
    pub fn push_back(&mut self, value: T) -> bool {
        if self.set.insert(value.clone()) {
            self.queue.push_back(value);
            true
        } else {
            false
        }
    }

    /// Removes and returns the element at the front of the queue.
    /// Returns `None` if the queue is empty.
    pub fn pop_front(&mut self) -> Option<T> {
        if let Some(value) = self.queue.pop_front() {
            self.set.remove(&value);
            Some(value)
        } else {
            None
        }
    }

    /// Returns a reference to the element at the front of the queue without removing it.
    /// Returns `None` if the queue is empty.
    pub fn front(&self) -> Option<&T> {
        self.queue.front()
    }

    /// Returns a reference to the element at the back of the queue without removing it.
    /// Returns `None` if the queue is empty.
    pub fn back(&self) -> Option<&T> {
        self.queue.back()
    }

    /// Returns `true` if the queue contains the specified value.
    pub fn contains(&self, value: &T) -> bool {
        self.set.contains(value)
    }

    /// Returns the number of elements in the queue.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Returns `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Removes all elements from the queue.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.set.clear();
    }

    /// Returns the element at the specified index, or `None` if out of bounds.
    pub fn get(&self, index: usize) -> Option<&T> {
        self.queue.get(index)
    }

    /// Shrinks the capacity of the queue as much as possible.
    pub fn shrink_to_fit(&mut self) {
        self.queue.shrink_to_fit();
        self.set.shrink_to_fit();
    }

    /// Reserves capacity for at least `additional` more elements.
    pub fn reserve(&mut self, additional: usize) {
        self.queue.reserve(additional);
        self.set.reserve(additional);
    }
}

impl<T> Default for QueueSet<T>
where
    T: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let qs: QueueSet<i32> = QueueSet::new();
        assert!(qs.is_empty());
        assert_eq!(qs.len(), 0);
    }

    #[test]
    fn test_with_capacity() {
        let qs: QueueSet<i32> = QueueSet::with_capacity(10);
        assert!(qs.is_empty());
        assert_eq!(qs.len(), 0);
    }

    #[test]
    fn test_push_back_new_element() {
        let mut qs = QueueSet::new();
        assert!(qs.push_back(1));
        assert_eq!(qs.len(), 1);
        assert!(qs.contains(&1));
        assert_eq!(qs.front(), Some(&1));
        assert_eq!(qs.back(), Some(&1));
    }

    #[test]
    fn test_push_back_duplicate() {
        let mut qs = QueueSet::new();
        assert!(qs.push_back(1));
        assert!(!qs.push_back(1)); // Duplicate should return false
        assert_eq!(qs.len(), 1);
        assert!(qs.contains(&1));
    }

    #[test]
    fn test_push_back_multiple() {
        let mut qs = QueueSet::new();
        assert!(qs.push_back(1));
        assert!(qs.push_back(2));
        assert!(qs.push_back(3));

        assert_eq!(qs.len(), 3);
        assert_eq!(qs.front(), Some(&1));
        assert_eq!(qs.back(), Some(&3));

        assert!(qs.contains(&1));
        assert!(qs.contains(&2));
        assert!(qs.contains(&3));
    }

    #[test]
    fn test_pop_front_empty() {
        let mut qs: QueueSet<i32> = QueueSet::new();
        assert_eq!(qs.pop_front(), None);
    }

    #[test]
    fn test_pop_front_single_element() {
        let mut qs = QueueSet::new();
        qs.push_back(42);

        assert_eq!(qs.pop_front(), Some(42));
        assert!(qs.is_empty());
        assert!(!qs.contains(&42));
    }

    #[test]
    fn test_pop_front_multiple_elements() {
        let mut qs = QueueSet::new();
        qs.push_back(1);
        qs.push_back(2);
        qs.push_back(3);

        assert_eq!(qs.pop_front(), Some(1));
        assert_eq!(qs.len(), 2);
        assert!(!qs.contains(&1));
        assert!(qs.contains(&2));
        assert!(qs.contains(&3));

        assert_eq!(qs.pop_front(), Some(2));
        assert_eq!(qs.len(), 1);
        assert!(!qs.contains(&2));
        assert!(qs.contains(&3));

        assert_eq!(qs.pop_front(), Some(3));
        assert!(qs.is_empty());
        assert!(!qs.contains(&3));
    }

    #[test]
    fn test_fifo_order() {
        let mut qs = QueueSet::new();
        for i in 1..=5 {
            qs.push_back(i);
        }

        for expected in 1..=5 {
            assert_eq!(qs.pop_front(), Some(expected));
        }

        assert!(qs.is_empty());
    }

    #[test]
    fn test_front_and_back() {
        let mut qs = QueueSet::new();

        // Empty queue
        assert_eq!(qs.front(), None);
        assert_eq!(qs.back(), None);

        // Single element
        qs.push_back(1);
        assert_eq!(qs.front(), Some(&1));
        assert_eq!(qs.back(), Some(&1));

        // Multiple elements
        qs.push_back(2);
        qs.push_back(3);
        assert_eq!(qs.front(), Some(&1));
        assert_eq!(qs.back(), Some(&3));
    }

    #[test]
    fn test_contains() {
        let mut qs = QueueSet::new();

        assert!(!qs.contains(&1));

        qs.push_back(1);
        qs.push_back(2);

        assert!(qs.contains(&1));
        assert!(qs.contains(&2));
        assert!(!qs.contains(&3));

        qs.pop_front();
        assert!(!qs.contains(&1));
        assert!(qs.contains(&2));
    }

    #[test]
    fn test_get() {
        let mut qs = QueueSet::new();
        qs.push_back(10);
        qs.push_back(20);
        qs.push_back(30);

        assert_eq!(qs.get(0), Some(&10));
        assert_eq!(qs.get(1), Some(&20));
        assert_eq!(qs.get(2), Some(&30));
        assert_eq!(qs.get(3), None);
    }

    #[test]
    fn test_clear() {
        let mut qs = QueueSet::new();
        qs.push_back(1);
        qs.push_back(2);

        assert!(!qs.is_empty());

        qs.clear();

        assert!(qs.is_empty());
        assert_eq!(qs.len(), 0);
        assert!(!qs.contains(&1));
        assert!(!qs.contains(&2));
    }

    #[test]
    fn test_default() {
        let qs: QueueSet<i32> = QueueSet::default();
        assert!(qs.is_empty());
    }

    #[test]
    fn test_clone() {
        let mut qs1 = QueueSet::new();
        qs1.push_back(1);
        qs1.push_back(2);

        let qs2 = qs1.clone();

        assert_eq!(qs1.len(), qs2.len());
    }

    #[test]
    fn test_mixed_operations() {
        let mut qs = QueueSet::new();

        // Add some elements
        assert!(qs.push_back(1));
        assert!(qs.push_back(2));
        assert!(qs.push_back(3));

        // Try to add duplicate
        assert!(!qs.push_back(2));
        assert_eq!(qs.len(), 3);

        // Remove from front
        assert_eq!(qs.pop_front(), Some(1));
        assert_eq!(qs.len(), 2);

        // Add the removed element back (should work now)
        assert!(qs.push_back(1));
        assert_eq!(qs.len(), 3);
        assert_eq!(qs.back(), Some(&1));
    }

    #[test]
    fn test_string_type() {
        let mut qs = QueueSet::new();

        assert!(qs.push_back("hello".to_string()));
        assert!(qs.push_back("world".to_string()));
        assert!(!qs.push_back("hello".to_string())); // Duplicate

        assert_eq!(qs.len(), 2);
        assert!(qs.contains(&"hello".to_string()));
        assert!(qs.contains(&"world".to_string()));

        assert_eq!(qs.pop_front(), Some("hello".to_string()));
        assert_eq!(qs.pop_front(), Some("world".to_string()));
        assert!(qs.is_empty());
    }

    #[test]
    fn test_large_dataset() {
        let mut qs = QueueSet::new();

        // Add 1000 elements
        for i in 0..1000 {
            assert!(qs.push_back(i));
        }

        // Try to add duplicates
        for i in 0..1000 {
            assert!(!qs.push_back(i));
        }

        assert_eq!(qs.len(), 1000);

        // Remove all elements in FIFO order
        for expected in 0..1000 {
            assert_eq!(qs.pop_front(), Some(expected));
        }

        assert!(qs.is_empty());
    }

    #[test]
    fn test_reserve_and_shrink() {
        let mut qs = QueueSet::new();

        qs.reserve(100);
        qs.push_back(1);
        qs.push_back(2);

        qs.shrink_to_fit();

        assert_eq!(qs.len(), 2);
        assert!(qs.contains(&1));
        assert!(qs.contains(&2));
    }

    #[test]
    fn test_edge_case_empty_operations() {
        let mut qs: QueueSet<i32> = QueueSet::new();

        assert_eq!(qs.pop_front(), None);
        assert_eq!(qs.front(), None);
        assert_eq!(qs.back(), None);
        assert_eq!(qs.get(0), None);
        assert!(!qs.contains(&1));

        qs.clear(); // Should not panic on empty
        qs.shrink_to_fit(); // Should not panic on empty

        assert!(qs.is_empty());
    }
}