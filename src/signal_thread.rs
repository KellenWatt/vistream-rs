
use std::thread::{self, JoinHandle};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

type FlagResult = Result<(), anyhow::Error>;

pub struct SignaledThread {
    term_flag: Arc<AtomicBool>,
    job: Arc<Mutex<dyn Fn() -> FlagResult + Send + 'static>>,
    worker: Option<JoinHandle<()>>,
}

#[allow(dead_code)]
impl SignaledThread {
    pub fn new<F: Fn() -> FlagResult + Send + 'static>(job: F) -> Self {
        SignaledThread {
            term_flag: Arc::new(AtomicBool::new(false)),
            job: Arc::new(Mutex::new(job)),
            worker: None,
        }
    }

    pub fn spawn<F: Fn() -> FlagResult + Send + 'static>(job: F) -> Self {
        let mut t = SignaledThread::new(job);
        t.start();
        t
    }

    pub fn start(&mut self) {
        let term_flag = self.term_flag.clone();
        let job = self.job.clone();
        let handle = thread::spawn(move || {
            while !term_flag.load(Ordering::Acquire) {
                if (*job.lock().unwrap())().is_err() {
                    term_flag.store(true, Ordering::Release);
                    return;
                };
            }
        });

        self.worker = Some(handle);
    }

    // named kill instead of stop so that parity between starting and stopping isn't implied.
    // sending the stop signal is fatal to the thread.
    pub fn kill(&mut self) {
        self.term_flag.store(true, Ordering::Release);
    }

    pub fn is_finished(&self) -> bool {
        self.worker.is_none() || self.worker.as_ref().unwrap().is_finished()
    }

    pub fn is_ready(&self) -> bool {
        !self.term_flag.load(Ordering::Acquire) && self.worker.is_none()
    }

    // for parity (and a more convenient API), this should be self, not &self. Existance of reset() requires a break in that
    // parity. This is likely only useful in Camera, and only because it prevents the reallocation
    // of reqs. If this isn't an issue (which it probably isn't), we could realloc per start, and
    // just toss old threads.
    // TODO Write an error type for SignaledThread to make this return type make more sense.
    pub fn join(&mut self) -> Option<thread::Result<()>> {
        Some(self.worker.take()?.join())
    }

    pub fn reset(&mut self) -> Result<(), ()> {
        // can't reset a running thread
        if self.worker.is_some() && !self.term_flag.load(Ordering::Acquire) {
            return Err(());
        }
        self.term_flag.store(false, Ordering::Release);
        self.worker = None;
        Ok(())
    }
}
