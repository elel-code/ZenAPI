use anyhow::{Result, anyhow, bail};
use slint::{ComponentHandle, Timer, TimerMode};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use crate::ui::AppWindow;

pub fn run_ui_frame_latency_smoke(samples: usize, interval_ms: u64) -> Result<()> {
    if samples < 2 {
        bail!("ui-frame-latency smoke requires at least 2 samples");
    }
    if interval_ms == 0 {
        bail!("ui-frame-latency smoke interval must be greater than 0");
    }

    let app = AppWindow::new().map_err(|err| anyhow!(err.to_string()))?;
    app.set_response_status("Smoke".into());
    app.set_response_time("warming".into());
    app.set_response_size(format!("{samples} samples").into());
    app.set_response_tone("busy".into());
    app.show().map_err(|err| anyhow!(err.to_string()))?;

    let measurements = Rc::new(RefCell::new(FrameLatencyMeasurements::new(
        samples,
        interval_ms,
    )));
    let callback_measurements = Rc::clone(&measurements);
    let callback_app = app.as_weak();
    let timer = Timer::default();

    timer.start(
        TimerMode::Repeated,
        Duration::from_millis(interval_ms),
        move || {
            let Some(app) = callback_app.upgrade() else {
                let _ = slint::quit_event_loop();
                return;
            };

            let mut measurements = callback_measurements.borrow_mut();
            measurements.tick();
            app.set_response_time(
                format!(
                    "{}/{} ticks",
                    measurements.recorded(),
                    measurements.samples()
                )
                .into(),
            );

            if measurements.is_complete() {
                app.set_response_tone("success".into());
                let _ = slint::quit_event_loop();
            }
        },
    );

    let run_result = slint::run_event_loop().map_err(|err| anyhow!(err.to_string()));
    timer.stop();
    let _ = app.hide();
    run_result?;

    let summary = measurements.borrow().summary()?;
    println!("{}", summary.render());
    Ok(())
}

struct FrameLatencyMeasurements {
    samples: usize,
    interval_ms: u64,
    previous_tick: Option<Instant>,
    tick_ms: Vec<f64>,
}

impl FrameLatencyMeasurements {
    fn new(samples: usize, interval_ms: u64) -> Self {
        Self {
            samples,
            interval_ms,
            previous_tick: None,
            tick_ms: Vec::with_capacity(samples),
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        if let Some(previous_tick) = self.previous_tick {
            self.tick_ms
                .push(now.duration_since(previous_tick).as_secs_f64() * 1000.0);
        }
        self.previous_tick = Some(now);
    }

    fn recorded(&self) -> usize {
        self.tick_ms.len().min(self.samples)
    }

    fn samples(&self) -> usize {
        self.samples
    }

    fn is_complete(&self) -> bool {
        self.tick_ms.len() >= self.samples
    }

    fn summary(&self) -> Result<FrameLatencySummary> {
        if self.tick_ms.len() < self.samples {
            bail!(
                "ui-frame-latency smoke ended after {} of {} samples",
                self.tick_ms.len(),
                self.samples
            );
        }

        let mut tick_ms = self.tick_ms.clone();
        tick_ms.truncate(self.samples);
        let overrun_ms = tick_ms
            .iter()
            .map(|tick| (tick - self.interval_ms as f64).max(0.0))
            .collect::<Vec<_>>();

        Ok(FrameLatencySummary {
            samples: self.samples,
            interval_ms: self.interval_ms,
            tick: Stats::from_values(&tick_ms),
            overrun: Stats::from_values(&overrun_ms),
        })
    }
}

struct FrameLatencySummary {
    samples: usize,
    interval_ms: u64,
    tick: Stats,
    overrun: Stats,
}

impl FrameLatencySummary {
    fn render(&self) -> String {
        format!(
            "ui-frame-latency smoke\nsamples: {} interval_ms: {}\ntick_ms: min {:.2} avg {:.2} p95 {:.2} max {:.2}\noverrun_ms: min {:.2} avg {:.2} p95 {:.2} max {:.2}",
            self.samples,
            self.interval_ms,
            self.tick.min,
            self.tick.avg,
            self.tick.p95,
            self.tick.max,
            self.overrun.min,
            self.overrun.avg,
            self.overrun.p95,
            self.overrun.max,
        )
    }
}

struct Stats {
    min: f64,
    avg: f64,
    p95: f64,
    max: f64,
}

impl Stats {
    fn from_values(values: &[f64]) -> Self {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let sum = sorted.iter().sum::<f64>();
        let avg = sum / sorted.len() as f64;
        Self {
            min: sorted[0],
            avg,
            p95: percentile(&sorted, 0.95),
            max: sorted[sorted.len() - 1],
        }
    }
}

fn percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    let index = ((sorted_values.len() as f64 * percentile).ceil() as usize)
        .saturating_sub(1)
        .min(sorted_values.len() - 1);
    sorted_values[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_frame_latency_samples() {
        let measurements = FrameLatencyMeasurements {
            samples: 4,
            interval_ms: 16,
            previous_tick: None,
            tick_ms: vec![16.0, 18.0, 20.0, 40.0],
        };

        let summary = measurements.summary().expect("summary");

        assert_eq!(summary.samples, 4);
        assert_eq!(summary.interval_ms, 16);
        assert_eq!(summary.tick.min, 16.0);
        assert_eq!(summary.tick.max, 40.0);
        assert_eq!(summary.overrun.min, 0.0);
        assert_eq!(summary.overrun.max, 24.0);
    }
}
