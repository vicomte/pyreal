//! Making the device's pipelines on several threads at once.
//!
//! A driver whose shader cache is cold compiles each pipeline as it is made, and the engine makes
//! some sixty when it starts: one after another that is many seconds on the main thread, while the
//! driver can compile them side by side. [`Jobs`] runs each pipeline's making on a small pool of
//! threads and hands back a [`Pending`] the constructor waits for once it needs the pipeline.
//!
//! `PYREAL_SERIAL_PIPELINES=1` makes every job run where it is asked for, in order, as before:
//! for a driver that mishandles pipelines made side by side, and to measure the difference.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// A job for the pool: anything that runs once and sends its own result back.
type Job = Box<dyn FnOnce() + Send + 'static>;

/// Whether `PYREAL_SERIAL_PIPELINES` asks for every job to run in order, where it is asked for.
#[must_use]
pub fn serial_asked() -> bool {
    std::env::var_os("PYREAL_SERIAL_PIPELINES").is_some_and(|v| !v.is_empty() && v != "0")
}

/// A pool of threads running jobs in the order they were given, or none at all (every job run
/// where it is asked for).
#[derive(Debug)]
pub struct Jobs {
    queue: Option<mpsc::Sender<Job>>,
    workers: Vec<std::thread::JoinHandle<()>>,
}

impl Jobs {
    /// A pool of the machine's cores but two (the frame's thread and the ray-tracing builders'
    /// warm-up keep theirs), at least two and at most eight; `PYREAL_PIPELINE_THREADS` sets the
    /// width; none when [`serial_asked`].
    #[must_use]
    pub fn new() -> Self {
        if serial_asked() {
            return Self::serial();
        }
        let width = std::env::var("PYREAL_PIPELINE_THREADS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map_or(4, std::num::NonZeroUsize::get)
                    .saturating_sub(2)
                    .clamp(2, 8)
            });
        Self::with_threads(width)
    }

    /// No pool: every job runs where it is asked for.
    #[must_use]
    pub fn serial() -> Self {
        Self {
            queue: None,
            workers: Vec::new(),
        }
    }

    /// A pool of `threads` threads (none, and every job run where it is asked for, when 0 or 1).
    #[must_use]
    pub fn with_threads(threads: usize) -> Self {
        if threads <= 1 {
            return Self::serial();
        }
        let (tx, rx) = mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        let workers = (0..threads)
            .filter_map(|k| {
                let rx = Arc::clone(&rx);
                std::thread::Builder::new()
                    .name(format!("pipelines {k}"))
                    .spawn(move || loop {
                        // The lock is held only to take the next job, never while it runs.
                        let job = match rx.lock() {
                            Ok(rx) => rx.recv(),
                            Err(_) => return,
                        };
                        match job {
                            Ok(job) => job(),
                            Err(_) => return,
                        }
                    })
                    .ok()
            })
            .collect::<Vec<_>>();
        if workers.is_empty() {
            return Self::serial();
        }
        Self {
            queue: Some(tx),
            workers,
        }
    }

    /// Whether jobs run on threads of their own.
    #[must_use]
    pub fn parallel(&self) -> bool {
        self.queue.is_some()
    }

    /// Runs `job` on the pool, or here when there is none.
    pub fn run<T: Send + 'static>(&self, job: impl FnOnce() -> T + Send + 'static) -> Pending<T> {
        let Some(queue) = self.queue.as_ref() else {
            return Pending::Ready(job());
        };
        let (tx, rx) = mpsc::channel();
        let wrapped: Job = Box::new(move || {
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
            let _ = tx.send(out);
        });
        match queue.send(wrapped) {
            Ok(()) => Pending::Waiting(rx),
            // The pool is gone: the job came back unrun, so it runs here and sends its result to
            // the channel `rx` holds.
            Err(mpsc::SendError(job)) => {
                job();
                Pending::Waiting(rx)
            }
        }
    }
}

impl Default for Jobs {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Jobs {
    fn drop(&mut self) {
        // Closing the queue lets every worker finish what it holds and leave.
        self.queue = None;
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

/// A job's result, there already or still being made.
#[derive(Debug)]
pub enum Pending<T> {
    Ready(T),
    Waiting(mpsc::Receiver<std::thread::Result<T>>),
}

impl<T> Pending<T> {
    /// The result, waiting for it when it is still being made. A job that panicked panics here,
    /// on the thread that wanted its result, as it would have had it run here.
    ///
    /// # Panics
    /// When the job panicked, or its thread was lost before it ran.
    pub fn wait(self) -> T {
        match self {
            Self::Ready(t) => t,
            Self::Waiting(rx) => match rx.recv() {
                Ok(Ok(t)) => t,
                Ok(Err(panic)) => std::panic::resume_unwind(panic),
                Err(_) => panic!("a pipeline job's thread was lost before it ran"),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_run_on_the_pool_and_their_results_come_back_in_any_order() {
        let jobs = Jobs::with_threads(4);
        assert!(jobs.parallel());
        let pending: Vec<_> = (0..32u64)
            .map(|k| {
                jobs.run(move || {
                    std::thread::sleep(std::time::Duration::from_millis(32 - k));
                    k * 2
                })
            })
            .collect();
        let got: Vec<u64> = pending.into_iter().map(Pending::wait).collect();
        assert_eq!(got, (0..32).map(|k| k * 2).collect::<Vec<_>>());
    }

    #[test]
    fn serial_jobs_run_where_they_are_asked_for() {
        let jobs = Jobs::serial();
        assert!(!jobs.parallel());
        let here = std::thread::current().id();
        let ran_on = jobs.run(move || std::thread::current().id()).wait();
        assert_eq!(ran_on, here);
    }

    #[test]
    fn a_job_that_panics_panics_where_its_result_is_wanted() {
        let jobs = Jobs::with_threads(2);
        let p = jobs.run(|| -> u32 { panic!("the job's own panic") });
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.wait()));
        assert!(caught.is_err());
        // The pool still runs jobs after one panicked.
        assert_eq!(jobs.run(|| 7).wait(), 7);
    }
}
