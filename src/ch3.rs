//! Summary
//! - There might not be a global consistent order of all atomic operations, as things can appear to happen in a different order from different threads.
//! - However, each individual atomic variable has its own total modification order, regardless of memory ordering, which all threads agree on.
//! - The order of operations is formally defined through happens-before relationships.
//! - Within a single thread, there is a happens-before relationship between every single operation.
//! - Spawning a thread happens-before everything the spawned thread does.
//! - Everything a thread does happens-before joining that thread.
//! - Unlocking a mutex happens-before locking that mutex again.
//! - Acquire-loading the value from a release-store establishes a happens-before relationship. This value may be modified by any number of fetch-and-modify and compare-and-exchange operations.
//! - A consume-load would be a lightweight version of an acquire-load, if it existed.
//! - Sequentially consistent ordering results in a globally consistent order of operations, but is almost never necessary and can make code review more complicated.
//! - Fences allow you to combine the memory ordering of multiple operations or apply a memory ordering conditionally. 
    
use super::*;

#[test]
fn seqcst() {
    static IS_THREAD_A_ACCESSING_S: AtomicBool = AtomicBool::new(false);
    static IS_THREAD_B_ACCESSING_S: AtomicBool = AtomicBool::new(false);
    
    static mut S: String = String::new();
    
    let a = thread::spawn(|| {
        // raise the accessing S from thread a flag
        IS_THREAD_A_ACCESSING_S.store(true, SeqCst);

        // check if thread b is accessing S
        if !IS_THREAD_B_ACCESSING_S.load(SeqCst) {
            // SAFETY: the accessing S from thread b flag was not set
            unsafe { S.push_str("\npushed from a\n") };
        }
    });

    let b = thread::spawn(|| {
        // raise the accessing S from thread b flag
        IS_THREAD_B_ACCESSING_S.store(true, SeqCst);

        // check if thread a is accessing S
        if !IS_THREAD_A_ACCESSING_S.load(SeqCst) {
            // SAFETY: the accessing S from thread a flag was not set
            unsafe { S.push_str("\npushed from b\n") };
        }
    });

    a.join().unwrap();
    b.join().unwrap();

    println!("{}", unsafe { S.as_str() });
}

#[test]
fn conditional_fence() {
    fn some_calculation(i: usize) -> u64 {
        return (2 * i) as u64;
    }

    static mut DATA: [u64; 10] = [0; 10];
    const ATOMIC_FALSE: AtomicBool = AtomicBool::new(false);
    static READY: [AtomicBool; 10] = [ATOMIC_FALSE; 10];

    for thread_index in 0..10 {
        thread::spawn(move || {
            // make a calculation
            let data = some_calculation(thread_index);

            // save the data
            // SAFETY: each thread gets a unique index
            unsafe { DATA[thread_index] = data };

            // Signal that the data from this thread is ready
            READY[thread_index].store(true, Release);
        });
    }

    // thread::sleep(Duration::from_millis(500));

    let ready: [bool; 10] = std::array::from_fn(|i| READY[i].load(Relaxed));
    
    if ready.contains(&true) {
        fence(Acquire);
        for i in 0..10 {
            if ready[i] {
                // SAFETY: The acquire-fence + Relaxed-load ensures that this loop happens-after READY's release-store.
                // the if expression also ensures we don't read unutilized data
                println!("data{i} = {}", unsafe { DATA[i] });
            }
        }
    }
}

#[test]
fn multiple_variables_one_fence() {
    static A: AtomicU8 = AtomicU8::new(0);
    static B: AtomicU8 = AtomicU8::new(0);
    static C: AtomicU8 = AtomicU8::new(0);

    thread::scope(|s| {
        // note from https://marabos.nl/atomics/memory-ordering.html#fences
        // A fence does not have to directly precede or follow the atomic operations.
        // Anything else can happen in between, including control flow.
        // This can be used to make the fence conditional, similar to how compare-and-exchange operations have both a success and a failure ordering

        // thread 2
        s.spawn(|| {
            let _a = A.load(Relaxed);
            let _b = B.load(Relaxed);
            let _c = C.load(Relaxed);
            fence(Acquire); // this fence happens after
        });

        // thread 1
        s.spawn(|| {
            fence(Release); // this fence happens before
            A.store(1, Relaxed);
            B.store(2, Relaxed);
            C.store(3, Relaxed);
        });
    });
    println!(
        "{}, {}, {}",
        A.load(Relaxed),
        B.load(Relaxed),
        C.load(Relaxed)
    );
}

#[test]
fn fence_equalities() {
    static A: AtomicUsize = AtomicUsize::new(0);
    static B: AtomicUsize = AtomicUsize::new(0);

    thread::scope(|s| {
        s.spawn(|| {
            // a release-store operation
            A.store(1, Release);
        });
        s.spawn(|| {
            // is the same as a release-fence and a relaxed-store
            fence(Release);
            B.store(5, Relaxed);
        });

        s.spawn(|| {
            // a acquire-load operation
            let _a_value = A.load(Acquire);
        });
        s.spawn(|| {
            // is the same as a acquire-fence and a relaxed-load
            let _b_value = B.load(Relaxed);
            fence(Acquire);
        });
    });

    println!(
        "A loaded without fence: {}\nB loaded with fence: {}",
        A.load(Relaxed),
        B.load(Relaxed)
    );
    // Using a separate fence can result in an extra processor instruction, though, which can be slightly less efficient
}