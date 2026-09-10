use std::{collections::VecDeque, time::Duration};

use rand::{RngExt, rng};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    time::{Instant, sleep_until},
};

#[derive(Debug, Clone, Copy, Default)]
pub struct Impairment {
    pub lag: Duration,
    pub jitter: f32,
    pub drop_probability: f32,
}

impl Impairment {
    pub(super) fn drops(&self) -> bool {
        self.drop_probability > 0.0 && rng().random_bool(f64::from(self.drop_probability))
    }

    fn delay(&self, unreliable: bool) -> Duration {
        if !unreliable || self.jitter == 0.0 || self.lag.is_zero() {
            return self.lag;
        }
        let jitter = f64::from(self.jitter);
        self.lag.mul_f64(rng().random_range((1.0 - jitter)..=(1.0 + jitter)))
    }
}

pub(super) fn impaired_sender<T: Send + 'static>(
    impairment: Impairment,
    output: UnboundedSender<T>,
    unreliable: impl Fn(&T) -> bool + Send + 'static,
) -> UnboundedSender<T> {
    if impairment.lag.is_zero() {
        return output;
    }
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(delay_stage(
        move |item| impairment.delay(unreliable(item)),
        receiver,
        output,
    ));
    sender
}

pub(super) fn impaired_receiver<T: Send + 'static>(
    impairment: Impairment,
    input: UnboundedReceiver<T>,
    unreliable: impl Fn(&T) -> bool + Send + 'static,
) -> UnboundedReceiver<T> {
    if impairment.lag.is_zero() {
        return input;
    }
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(delay_stage(
        move |item| impairment.delay(unreliable(item)),
        input,
        sender,
    ));
    receiver
}

async fn delay_stage<T>(
    mut delay: impl FnMut(&T) -> Duration,
    mut input: UnboundedReceiver<T>,
    output: UnboundedSender<T>,
) {
    let mut queue: VecDeque<(Instant, T)> = VecDeque::new();
    loop {
        let Some((due, _)) = queue.front() else {
            match input.recv().await {
                Some(item) => {
                    let lag = delay(&item);
                    enqueue(&mut queue, item, lag);
                }
                None => break,
            }
            continue;
        };
        let due = *due;
        tokio::select! {
            () = sleep_until(due) => {
                let Some((_, item)) = queue.pop_front() else {
                    return;
                };
                if output.send(item).is_err() {
                    return;
                }
            }
            received = input.recv() => match received {
                Some(item) => {
                    let lag = delay(&item);
                    enqueue(&mut queue, item, lag);
                }
                None => break,
            },
        }
    }
    while let Some((due, item)) = queue.pop_front() {
        sleep_until(due).await;
        if output.send(item).is_err() {
            return;
        }
    }
}

fn enqueue<T>(queue: &mut VecDeque<(Instant, T)>, item: T, lag: Duration) {
    let due = Instant::now() + lag;
    let index = queue.partition_point(|(queued, _)| *queued <= due);
    queue.insert(index, (due, item));
}

#[cfg(test)]
#[path = "tests/impairment.rs"]
mod tests;
