use super::*;

pub struct Channel<T> {
    queue: Mutex<VecDeque<T>>,
    message_ready: Condvar,
}
impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            queue: Mutex::default(),
            message_ready: Condvar::new(),
        }
    }

    pub fn send(&mut self, message: T) -> Result<(), Box<dyn std::error::Error + '_>> {
        // add the message to the queue
        self.queue.lock()?.push_front(message);

        // Notify a blocked thread that a message is ready
        self.message_ready.notify_one();

        Ok(())
    }

    pub fn receive(&mut self) -> Result<T, Box<dyn std::error::Error + '_>> {
        let mut guard = self.queue.lock()?;
        
        loop {
            match guard.pop_front() {
                Some(message) => return Ok(message),
                None => guard = self.message_ready.wait(guard)?,
            }
        }
    }
}
