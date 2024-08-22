use super::*;

pub struct SimpleChannel<T> {
    queue: Mutex<VecDeque<T>>,
    message_ready: Condvar,
}
impl<T> SimpleChannel<T> {
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
    is_message_in_use: AtomicBool,
    is_message_ready: AtomicBool,
}

unsafe impl<T> Sync for OneshotChannel<T> where T: Send {}

impl<T> OneshotChannel<T> {
    pub const fn new() -> Self {
        Self {
            message: UnsafeCell::new(MaybeUninit::uninit()),
            is_message_in_use: AtomicBool::new(false),
            is_message_ready: AtomicBool::new(false),
        }
    }

    pub fn is_message_ready(&self) -> bool {
        self.is_message_ready.load(Relaxed)
    }

    /// [OneshotChannel] can only [OneshotChannel::send] one message.
    /// # Panics
    /// - when [OneshotChannel::send] is called more than once
    pub fn send(&self, message: T) {
        // set the in use flag
        // panic if it was already set
        if self.is_message_in_use.swap(true, Relaxed) {
            panic!("Can't send more than one message. Only call OneshotChannel::send once!!!!");
        }

        // Safety: the message can't be in use because of the panic
        unsafe {
            let maybe_uninit_message = &mut *self.message.get();
            maybe_uninit_message.write(message);
        }

        // notify the message is ready
        self.is_message_ready.store(true, Release);
    }

    /// Use [OneshotChannel::is_message_ready] to be sure to [OneshotChannel::receive] won't panic
    /// # Panics
    /// - When the message is not ready.
    pub fn receive(&self) -> T {
        // sets the message ready flag to false
        // panics if the value was already false
        if !self.is_message_ready.swap(false, Acquire) {
            panic!("The message was not ready. Be sure to check OneshotChannel::is_message_ready before calling OneshotChannel::receive");
        }

        // Safety: The message is initialized at this point because of the panic
        unsafe {
            let maybe_uninit_message = &*self.message.get();
            maybe_uninit_message.assume_init_read()
        }
    }
}

impl<T> OneshotChannel<T> {
    /// # Safety
    /// - Only call this method once!
    pub unsafe fn send_unchecked(&self, message: T) {
        let maybe_uninit_message = &mut *self.message.get();
        maybe_uninit_message.write(message);

        // Notify that a message is ready
        self.is_message_ready.store(true, Release);
    }

    /// # Safety:
    /// - Only call this method after [OneshotChannel::is_message_ready] returns `true`
    /// - Only call this method once
    pub unsafe fn receive_unchecked(&self) -> T {
        let maybe_uninit_message = &*self.message.get();
        maybe_uninit_message.assume_init_read()
    }
}

impl<T> Drop for OneshotChannel<T> {
    fn drop(&mut self) {
        if *self.is_message_ready.get_mut() {
            unsafe { self.message.get_mut().assume_init_drop() }
        }
    }
}

#[test]
fn oneshot_channel_drop() {
    const MESSAGE: &'static str = "Message text";
    let channel = OneshotChannel::new();
    let current_thread = thread::current();

    thread::scope(|s| {
        s.spawn(|| {
            channel.send(MESSAGE);
            current_thread.unpark();
        });

        while !channel.is_message_ready() {
            thread::park();
        }

        assert_eq!(channel.receive(), MESSAGE);
    });
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let channel = Arc::new(Channel {
        message: UnsafeCell::new(MaybeUninit::uninit()),
        is_message_ready: AtomicBool::new(false),
    });

    let sender = Sender {
        channel: Arc::clone(&channel),
    };

    let reciever = Receiver { channel };

    (sender, reciever)
}

struct Channel<T> {
    message: UnsafeCell<MaybeUninit<T>>,
    is_message_ready: AtomicBool,
}
unsafe impl<T> Sync for Channel<T> where T: Send {}
impl<T> Drop for Channel<T> {
    fn drop(&mut self) {
        if *self.is_message_ready.get_mut() {
            unsafe { self.message.get_mut().assume_init_drop() };
        }
    }
}

pub struct Sender<T> {
    channel: Arc<Channel<T>>,
}
pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Sender<T> {
    pub fn send(self, message: T) {
        // Safety: This method take ownership of self (Self is not Copy) so the message can't be initialized more than once
        unsafe { (*self.channel.message.get()).write(message) };

        self.channel.is_message_ready.store(true, Release);
    }
}
impl<T> Receiver<T> {
    pub fn is_message_ready(&self) -> bool {
        self.channel.is_message_ready.load(Relaxed)
    }
    pub fn receive(self) -> T {
        if !self.channel.is_message_ready.swap(false, Acquire) {
            panic!("Message is not ready! Be sure to check Receiver::is_message_ready before calling Receiver::receive");
        }

        unsafe { (*self.channel.message.get()).assume_init_read() }
    }
}

#[test]
fn split_channel_drop() {
    const MESSAGE: &'static str = "Message text";
    let (sender, receiver) = channel();
    let current_thread = thread::current();

    thread::scope(|s| {
        s.spawn(|| {
            sender.send(MESSAGE);
            current_thread.unpark();
        });

        while !receiver.is_message_ready() {
            thread::park();
        }

        assert_eq!(receiver.receive(), MESSAGE);
    });
}
