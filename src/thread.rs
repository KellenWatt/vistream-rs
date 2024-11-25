#![allow(dead_code)]
use std::thread::{self, JoinHandle};
use std::sync::{Arc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

type FlagResult = Result<(), anyhow::Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("worker has already been launched")]
    AlreadyStarted,
    #[error("worker has already been joined")]
    AlreadyJoined,
    #[error("worker has not been started")]
    Unstarted,
    // #[error("worker thread panicked")]
    // ThreadPanicked(Box<dyn Any + Send + 'static>),

    // #[error(transparent)]
    // Send(#[from] std::sync::mpsc::SendError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub struct WorkerThread<T> {
    term_flag: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    job: Option<Box<dyn Fn() + Send>>,
    tx: mpsc::Sender<T>,
    // rx: mpsc::Receiver<U>,
}

impl<T: Send + 'static> WorkerThread<T> {
    pub fn new<F: Fn(&mpsc::Receiver<T>) -> FlagResult + Send + 'static>(job: F) -> WorkerThread<T> { 
        let (tx, rx): (_, mpsc::Receiver<T>) = mpsc::channel();
        let term_flag = Arc::new(AtomicBool::new(false));
        WorkerThread {
            term_flag: term_flag.clone(),
            worker: None,
            tx,
            // rx,
            job: Some(Box::new(move || {
                // let rx = rx;
                while !term_flag.load(Ordering::Acquire) {
                    if job(&rx).is_err() {
                        term_flag.store(true, Ordering::Release);
                    }
                }
            })),
        }
    }

    pub fn spawn<F: Fn(&mpsc::Receiver<T>) -> FlagResult + Send + 'static>(job: F) -> Result<WorkerThread<T>, Error> {
        let mut t = WorkerThread::new(job);
        t.start()?;
        Ok(t)
    }

    pub fn start(&mut self) -> Result<(), Error> {
        let job = self.job.take().ok_or(Error::AlreadyStarted)?;
        let builder = thread::Builder::new();
        let handle = builder.spawn(job)?;
        self.worker = Some(handle);
        Ok(())
    }

    pub fn kill(&mut self) {
        self.term_flag.store(true, Ordering::Release);
    }

    pub fn join(&mut self) -> Result<(), Error> {
        if self.worker.is_none() && self.job.is_none() {
            return Err(Error::AlreadyJoined);
        }
        // We are intentionally discarding here
        let _ = self.worker.take().ok_or(Error::Unstarted)?.join();
        // res.map_err(|e| Error::ThreadPanicked(e))
        Ok(())
    }

    pub fn is_finished(&self) -> bool {
        self.job.is_none() && (self.worker.is_none() || self.worker.as_ref().unwrap().is_finished())
    }


    // pub fn send(&self, work: T) -> Result<(), Error> {
    //     self.tx.send(work).map_err(|_| Error::Send)
    //     // return Ok(());
    // }

    pub fn sender(&self) -> mpsc::Sender<T> {
        self.tx.clone()
    }
}
