use std::{collections::VecDeque, time::Duration};

use rand::{RngExt, rng};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    time::{Instant, sleep_until},
};

// Test aids for the network path: `--lag-ms` delays every message by a fixed
// amount in arrival order, `--drop` discards that fraction of unreliable
// messages in each direction.
#[derive(Debug, Clone, Copy, Default)]
pub struct Impairment {
    pub lag: Duration,
    pub drop_probability: f32,
}

impl Impairment {
    pub(super) fn drops(&self) -> bool {
        self.drop_probability > 0.0 && rng().random_bool(f64::from(self.drop_probability))
    }
}

pub(super) fn impaired_sender<T: Send + 'static>(lag: Duration, output: UnboundedSender<T>) -> UnboundedSender<T> {
    if lag.is_zero() {
        return output;
    }
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(delay_stage(lag, receiver, output));
    sender
}

pub(super) fn impaired_receiver<T: Send + 'static>(lag: Duration, input: UnboundedReceiver<T>) -> UnboundedReceiver<T> {
    if lag.is_zero() {
        return input;
    }
    let (sender, receiver) = unbounded_channel();
    tokio::spawn(delay_stage(lag, input, sender));
    receiver
}

// Forwards every item `lag` after it arrived, in arrival order, so a delayed
// channel still preserves the reliable lane's order.
async fn delay_stage<T>(lag: Duration, mut input: UnboundedReceiver<T>, output: UnboundedSender<T>) {
    let mut queue: VecDeque<(Instant, T)> = VecDeque::new();
    loop {
        let Some((due, _)) = queue.front() else {
            match input.recv().await {
                Some(item) => queue.push_back((Instant::now() + lag, item)),
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
                Some(item) => queue.push_back((Instant::now() + lag, item)),
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

#[cfg(test)]
mod tests {
    use super::*;

    const LAG: Duration = Duration::from_millis(50);
    const SPACING: Duration = Duration::from_millis(10);

    #[tokio::test(start_paused = true)]
    async fn delay_stage_forwards_in_order_after_lag() {
        let (input, stage_input) = unbounded_channel();
        let (stage_output, mut output) = unbounded_channel();
        tokio::spawn(delay_stage(LAG, stage_input, stage_output));
        let start = Instant::now();
        for item in 1..=3u32 {
            input.send(item).expect("stage dropped its input");
            tokio::task::yield_now().await;
            tokio::time::advance(SPACING).await;
        }

        for expected in 1..=3u32 {
            let item = output.recv().await.expect("stage closed early");
            assert_eq!(item, expected);
            let earliest = LAG + SPACING * (expected - 1);
            assert!(Instant::now() - start >= earliest, "item {item} arrived early");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn delay_stage_drains_after_input_closes() {
        let (input, stage_input) = unbounded_channel();
        let (stage_output, mut output) = unbounded_channel();
        tokio::spawn(delay_stage(LAG, stage_input, stage_output));
        input.send("first").expect("stage dropped its input");
        input.send("second").expect("stage dropped its input");
        drop(input);

        assert_eq!(output.recv().await, Some("first"));
        assert_eq!(output.recv().await, Some("second"));
        assert_eq!(output.recv().await, None);
    }

    #[test]
    fn drop_probability_bounds_are_exact() {
        let never = Impairment {
            lag: Duration::ZERO,
            drop_probability: 0.0,
        };
        let always = Impairment {
            lag: Duration::ZERO,
            drop_probability: 1.0,
        };
        for _ in 0..100 {
            assert!(!never.drops());
            assert!(always.drops());
        }
    }
}
