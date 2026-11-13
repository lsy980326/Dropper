use std::sync::{mpsc, Mutex};

/// Delivers events on the caller thread while a bounded set of workers owns inputs.
pub fn run<T: Send, E: Send>(
    items: Vec<T>,
    workers: usize,
    process: impl Fn(T, &mut dyn FnMut(E)) + Sync,
    emit: &mut dyn FnMut(E),
) {
    let workers = workers.max(1).min(items.len());
    let queue = Mutex::new(items.into_iter());
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::sync_channel(128);
        for _ in 0..workers {
            let tx = tx.clone();
            let queue = &queue;
            let process = &process;
            scope.spawn(move || loop {
                let item = queue.lock().expect("Batch queue poisoned").next();
                let Some(item) = item else {
                    break;
                };
                process(item, &mut |event| {
                    let _ = tx.send(event);
                });
            });
        }
        drop(tx);
        for event in rx {
            emit(event);
        }
    });
}
