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
        // lock the queue
        let mut guard = self.queue.lock()?;

        // receiving loop
        loop {
            // check if there is a message in the queue
            match guard.pop_front() {
                // return the message
                Some(message) => return Ok(message),

                // or wait for the message to be ready
                None => guard = self.message_ready.wait(guard)?,
            }
        }
    }
}
