use std::collections::VecDeque;

pub struct Queue<T> {
    /// Front of the queue is the front of the vec. 'front' means first to be popped.
    queue: VecDeque<T>,
    capacity: usize,
}

#[allow(unused)]
impl<T> Queue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn fill_with(&mut self, func: impl Fn() -> T) {
        while self.queue.len() < self.capacity {
            self.queue.push_back(func());
        }
    }

    /// Pushes to the back of the queue. Pops the first element if the queue is full.
    pub fn push_back(&mut self, item: T) {
        if self.queue.len() == self.capacity {
            self.queue.pop_front();
        }
        self.queue.push_back(item);
    }

    /// Pops an element off the front of the queue.
    pub fn pop_front(&mut self) -> Option<T> {
        self.queue.pop_front()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.queue.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.queue.get_mut(index)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.queue.iter()
    }
}

#[macro_export]
macro_rules! impl_bytes {
    ($t:ty) => {
        impl Into<bevy_renet::renet::Bytes> for $t {
            fn into(self) -> bevy_renet::renet::Bytes {
                let encoded = bincode::serialize(&self).unwrap();
                bevy_renet::renet::Bytes::copy_from_slice(&encoded)
            }
        }

        impl TryFrom<bevy_renet::renet::Bytes> for $t {
            type Error = bincode::Error;

            fn try_from(bytes: bevy_renet::renet::Bytes) -> Result<Self, bincode::Error> {
                bincode::deserialize(&bytes)
            }
        }
    };
}

pub struct Buffer<T> {
    buf: VecDeque<T>,
    capacity: usize,
}

#[allow(unused)]
impl<T> Buffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn fill_with(&mut self, func: impl Fn() -> T) {
        while self.buf.len() < self.capacity {
            self.buf.push_back(func());
        }
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.buf.pop_front()
    }

    pub fn push_front(&mut self, value: T) {
        if self.buf.len() == self.capacity {
            self.buf.pop_back();
        }
        self.buf.push_front(value)
    }

    pub fn push_back(&mut self, item: T) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(item);
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.buf.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.buf.get_mut(index)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter()
    }
}
