//! Runs slow work (opening a level, ROM builds, model imports, model pictures) on worker threads. The result
//! comes back to the window's thread as a closure, which the window runs at the start of a frame.
//! A port of the former Python editor's `jobs.py`.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

type Work = Box<dyn FnOnce() + Send>;

pub struct Jobs<T> {
    work: Sender<Work>,
    done_tx: Sender<Box<dyn FnOnce(&mut T) + Send>>,
    done: Receiver<Box<dyn FnOnce(&mut T) + Send>>,
    ctx: egui::Context,
    /// Jobs given out and not yet handed back.
    pub running: usize,
}

impl<T: 'static> Jobs<T> {
    /// A pool of `workers` threads. `ctx` is asked to draw again when a job is done.
    pub fn new(workers: usize, ctx: egui::Context) -> Jobs<T> {
        let (work, queue) = channel::<Work>();
        let queue = Arc::new(Mutex::new(queue));
        for i in 0..workers.max(1) {
            let queue = queue.clone();
            let _ = std::thread::Builder::new().name(format!("worker {i}")).spawn(move || loop {
                let job = queue.lock().unwrap_or_else(|e| e.into_inner()).recv();
                match job {
                    Ok(job) => job(),
                    Err(_) => return,
                }
            });
        }
        let (done_tx, done) = channel();
        Jobs { work, done_tx, done, ctx, running: 0 }
    }

    /// Calls `work` on a worker thread, then `then` with its result on the window's thread.
    pub fn run<R: Send + 'static>(&mut self, work: impl FnOnce() -> R + Send + 'static, then: impl FnOnce(&mut T, R) + Send + 'static) {
        let (tx, ctx) = (self.done_tx.clone(), self.ctx.clone());
        self.running += 1;
        let _ = self.work.send(Box::new(move || {
            let r = work();
            let _ = tx.send(Box::new(move |t: &mut T| then(t, r)));
            ctx.request_repaint();
        }));
    }

    /// The results that came in since the last call.
    pub fn take_done(&mut self) -> Vec<Box<dyn FnOnce(&mut T) + Send>> {
        let out: Vec<_> = self.done.try_iter().collect();
        self.running -= out.len().min(self.running);
        out
    }
}
