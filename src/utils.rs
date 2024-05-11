use std::collections::VecDeque;

pub struct Queue<T> {
    queue: VecDeque<T>,
    capacity: usize,
}

impl<T> Queue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.queue.len() == self.capacity {
            self.queue.pop_back();
        }
        self.queue.push_front(item);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.queue.get(index)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
    }
}
