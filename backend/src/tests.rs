use std::thread;
use std::time::Duration;
use futures::{
    future,
    Future,
    Stream,
};
use futures::sync::oneshot;
use futures::stream::futures_unordered::FuturesUnordered;

use ::{
    DebugLogger,
    TracedThread,
    SyncSpan,
    TraceFuture,
};

#[test]
fn test_sync() {
    let _thread = TracedThread::new("test_sync", Box::new(DebugLogger));
    let _first_span = SyncSpan::new("first_span");
    let _second_span = SyncSpan::new("second_span");
}

#[test]
fn test_async() {
    let _thread = TracedThread::new("test_async", Box::new(DebugLogger));

    let (txs, rxs) = (0..10).map(|_| oneshot::channel::<usize>())
        .unzip::<_, _, Vec<_>, Vec<_>>();

    let mut rx_join = FuturesUnordered::new();
    for (i, rx) in rxs.into_iter().enumerate() {
        rx_join.push(rx.traced(format!("rx:{}", i)));
    }

    let okay = future::ok(10).traced("okay");
    let err = future::err(11)
        .traced("not okay")
        .then(|_: Result<usize, usize>| Ok::<usize, oneshot::Canceled>(12))
        .traced("calm down");

    let sender = thread::spawn(move || {
        let _thread = TracedThread::new("test_async:sender", Box::new(DebugLogger));
        thread::sleep(Duration::from_millis(10));
        for (i, tx) in txs.into_iter().enumerate() {
            tx.send(i).unwrap();
        }
    });

    let (oneshots, okay, calm_down) = rx_join.collect()
        .traced("collect")
        .join3(okay, err)
        .traced("join3")
        .wait()
        .unwrap();
    sender.join().unwrap();
    assert_eq!(oneshots.iter().sum::<usize>() + okay + calm_down, 67);
}
