// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright (c) Leo Bolster.

//! A single-producer single-consumer queue that never blocks and never allocates.
//!
//! The audio callback runs on a thread with a hard deadline: if it is late, the
//! sound card gets silence and the whole thing falls over. That rules out
//! locks, allocation, and anything else that can wait — which rules out most
//! ordinary channels. This is the small piece of machinery that lets the audio
//! thread hand decoded frames to the rest of the program and immediately forget
//! about them.
//!
//! Capacity is fixed. If the consumer stalls long enough to fill it, frames are
//! counted as dropped rather than the producer waiting — losing timecode is
//! survivable, being late in the audio callback is not.

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Queue<T, const N: usize> {
    slots: UnsafeCell<[T; N]>,
    /// Where the consumer reads next.
    head: AtomicUsize,
    /// Where the producer writes next.
    tail: AtomicUsize,
    dropped: AtomicUsize,
}

// Safe because exactly one thread pushes and exactly one pops, and the indices
// are published with release/acquire ordering so the data is visible before the
// index that makes it readable.
unsafe impl<T: Send, const N: usize> Sync for Queue<T, N> {}
unsafe impl<T: Send, const N: usize> Send for Queue<T, N> {}

impl<T: Copy + Default, const N: usize> Default for Queue<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Default, const N: usize> Queue<T, N> {
    pub fn new() -> Self {
        assert!(N > 1, "a queue of one slot can never hold anything");
        Self {
            slots: UnsafeCell::new([T::default(); N]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
        }
    }

    /// Producer side. Returns false if the queue is full, having counted it.
    pub fn push(&self, value: T) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % N;
        if next == self.head.load(Ordering::Acquire) {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        // Safety: only the producer writes, and only to the slot at `tail`,
        // which the consumer will not read until `tail` is published below.
        unsafe {
            (*self.slots.get())[tail] = value;
        }
        self.tail.store(next, Ordering::Release);
        true
    }

    /// Consumer side.
    pub fn pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Relaxed);
        if head == self.tail.load(Ordering::Acquire) {
            return None;
        }
        // Safety: the producer has published this slot and will not touch it
        // again until we advance `head`.
        let value = unsafe { (*self.slots.get())[head] };
        self.head.store((head + 1) % N, Ordering::Release);
        Some(value)
    }

    /// How many items were thrown away because nobody was collecting them.
    pub fn dropped(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn carries_items_in_order() {
        let queue: Queue<u32, 8> = Queue::new();
        assert_eq!(queue.pop(), None);
        for value in 1..=5 {
            assert!(queue.push(value));
        }
        for value in 1..=5 {
            assert_eq!(queue.pop(), Some(value));
        }
        assert_eq!(queue.pop(), None);
    }

    #[test]
    fn drops_rather_than_waiting_when_full() {
        // One slot is always left empty to tell full from empty apart.
        let queue: Queue<u32, 4> = Queue::new();
        assert!(queue.push(1));
        assert!(queue.push(2));
        assert!(queue.push(3));
        assert!(!queue.push(4), "should have refused");
        assert_eq!(queue.dropped(), 1);

        // And it recovers as soon as room appears.
        assert_eq!(queue.pop(), Some(1));
        assert!(queue.push(4));
    }

    #[test]
    fn survives_a_producer_and_consumer_running_flat_out() {
        const COUNT: u32 = 200_000;
        let queue: Arc<Queue<u32, 64>> = Arc::new(Queue::new());

        let producer = {
            let queue = Arc::clone(&queue);
            std::thread::spawn(move || {
                let mut sent = 0;
                let mut value = 0;
                while value < COUNT {
                    if queue.push(value) {
                        sent += 1;
                        value += 1;
                    }
                }
                sent
            })
        };

        let consumer = {
            let queue = Arc::clone(&queue);
            std::thread::spawn(move || {
                let mut received = Vec::new();
                while (received.len() as u32) < COUNT {
                    if let Some(value) = queue.pop() {
                        received.push(value);
                    }
                }
                received
            })
        };

        let sent = producer.join().unwrap();
        let received = consumer.join().unwrap();
        assert_eq!(sent, COUNT);
        // Nothing lost, nothing duplicated, nothing out of order.
        assert_eq!(received.len(), COUNT as usize);
        assert!(received.iter().enumerate().all(|(i, v)| i as u32 == *v));
    }
}
