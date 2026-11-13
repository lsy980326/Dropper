use dropper_lib::parallel;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};

#[test]
fn workers_overlap_without_exceeding_limit_and_emit_every_result() {
    let active = AtomicUsize::new(0);
    let maximum = AtomicUsize::new(0);
    let barrier = Arc::new(Barrier::new(2));
    let mut results = vec![];
    parallel::run(
        (0..8).collect(),
        2,
        |item, send| {
            let count = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(count, Ordering::SeqCst);
            if item < 2 {
                barrier.wait();
            }
            send(item);
            active.fetch_sub(1, Ordering::SeqCst);
        },
        &mut |event| results.push(event),
    );
    results.sort();
    assert_eq!(results, (0..8).collect::<Vec<_>>());
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[test]
fn empty_batch_finishes_without_a_sender_deadlock() {
    parallel::run(Vec::<usize>::new(), 0, |_, send| send(1), &mut |_| {
        panic!("Unexpected event")
    });
}
