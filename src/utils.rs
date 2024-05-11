use std::collections::VecDeque;

pub struct Queue<T> {
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
