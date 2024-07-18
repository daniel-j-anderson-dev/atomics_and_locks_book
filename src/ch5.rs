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

pub struct OneshotChannel<T> {
    message: UnsafeCell<MaybeUninit<T>>,
    is_message_ready: AtomicBool,
}
unsafe impl<T> Sync for Channel<T> where T: Send {}
impl<T> OneshotChannel<T> {
    pub const fn new() -> Self {
        Self {
            message: UnsafeCell::new(MaybeUninit::uninit()),
            is_message_ready: AtomicBool::new(false),
        }
    }

    /// # Safety
    /// - Only call this method once!
    pub unsafe fn send(&mut self, message: T) {
        let maybe_uninit_message = &mut *self.message.get();
        maybe_uninit_message.write(message);

        // Notify that a message is ready
        self.is_message_ready.store(true, Release);
    }

    pub fn is_message_ready(&self) -> bool {
        self.is_message_ready.load(Acquire)
    }

    /// # Safety:
    /// - Only call this method after [OneshotChannel::is_message_ready] returns `true`
    /// - Only call this method once
    pub unsafe fn receive(&mut self) -> T {
        let maybe_uninit_message = &*self.message.get();
        maybe_uninit_message.assume_init_read()
    }
}
