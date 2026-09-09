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
mod tests {
    use super::*;

    const LAG: Duration = Duration::from_millis(50);
    const SPACING: Duration = Duration::from_millis(10);

    #[tokio::test(start_paused = true)]
    async fn delay_stage_forwards_in_order_after_lag() {
        let (input, stage_input) = unbounded_channel();
        let (stage_output, mut output) = unbounded_channel();
        tokio::spawn(delay_stage(|_| LAG, stage_input, stage_output));
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
    async fn earlier_deadlines_overtake_pending_messages() {
        let (input, stage_input) = unbounded_channel();
        let (stage_output, mut output) = unbounded_channel();
        tokio::spawn(delay_stage(
            |lag: &u64| Duration::from_millis(*lag),
            stage_input,
            stage_output,
        ));
        let start = Instant::now();
        input.send(150).expect("stage dropped its input");
        tokio::task::yield_now().await;
        tokio::time::advance(SPACING).await;
        input.send(50).expect("stage dropped its input");

        assert_eq!(output.recv().await, Some(50));
        assert_eq!(Instant::now() - start, Duration::from_millis(60));
        assert_eq!(output.recv().await, Some(150));
        assert_eq!(Instant::now() - start, Duration::from_millis(150));
    }

    #[tokio::test(start_paused = true)]
    async fn delay_stage_drains_after_input_closes() {
        let (input, stage_input) = unbounded_channel();
        let (stage_output, mut output) = unbounded_channel();
        tokio::spawn(delay_stage(
            |(lag, _): &(u64, &str)| Duration::from_millis(*lag),
            stage_input,
            stage_output,
        ));
        input.send((100, "slow")).expect("stage dropped its input");
        input.send((50, "first")).expect("stage dropped its input");
        input.send((50, "second")).expect("stage dropped its input");
        drop(input);

        assert_eq!(output.recv().await, Some((50, "first")));
        assert_eq!(output.recv().await, Some((50, "second")));
        assert_eq!(output.recv().await, Some((100, "slow")));
        assert_eq!(output.recv().await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn both_delay_directions_preserve_reliable_order_among_unreliable_messages() {
        let impairment = Impairment {
            lag: LAG,
            jitter: 1.0,
            ..Default::default()
        };
        for inbound in [true, false] {
            let (input, output) = unbounded_channel();
            let (input, mut output) = if inbound {
                (
                    impaired_sender(impairment, input, |(_, unreliable)| *unreliable),
                    output,
                )
            } else {
                (
                    input,
                    impaired_receiver(impairment, output, |(_, unreliable)| *unreliable),
                )
            };
            for id in 0..6 {
                input.send((id, true)).expect("stage dropped its input");
                input.send((id, false)).expect("stage dropped its input");
                tokio::task::yield_now().await;
                tokio::time::advance(SPACING).await;
            }
            drop(input);
            let mut reliable = Vec::new();
            let mut unreliable = Vec::new();
            while let Some((id, unordered)) = output.recv().await {
                if unordered {
                    unreliable.push(id);
                } else {
                    reliable.push(id);
                }
            }
            assert_eq!(reliable, (0..6).collect::<Vec<_>>());
            unreliable.sort();
            assert_eq!(unreliable, reliable);
        }
    }

    #[test]
    fn jitter_stays_within_bounds_and_leaves_reliable_delay_fixed() {
        let impairment = Impairment {
            lag: Duration::from_millis(100),
            jitter: 0.5,
            ..Default::default()
        };
        for _ in 0..100 {
            assert!((Duration::from_millis(50)..=Duration::from_millis(150)).contains(&impairment.delay(true)));
            assert_eq!(impairment.delay(false), impairment.lag);
        }
        let fixed = Impairment {
            jitter: 0.0,
            ..impairment
        };
        assert_eq!(fixed.delay(true), fixed.lag);
    }

    #[test]
    fn zero_lag_bypasses_both_delay_stages_even_with_jitter() {
        let impairment = Impairment {
            jitter: 1.0,
            ..Default::default()
        };
        assert_eq!(impairment.delay(true), Duration::ZERO);
        let (input, output) = unbounded_channel();
        let input = impaired_sender(impairment, input, |_| true);
        let mut output = impaired_receiver(impairment, output, |_| true);
        input.send(7).expect("stage dropped its input");
        assert_eq!(output.try_recv(), Ok(7));
    }

    #[test]
    fn drop_probability_bounds_are_exact() {
        let never = Impairment {
            drop_probability: 0.0,
            ..Default::default()
        };
        let always = Impairment {
            drop_probability: 1.0,
            ..Default::default()
        };
        for _ in 0..100 {
            assert!(!never.drops());
            assert!(always.drops());
        }
    }
}
