use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use tokio::task::JoinHandle;

pub type Labels<'a> = &'a [(&'a str, &'a str)];

pub trait MetricsSink: Send + Sync + 'static {
    fn incr_counter(&self, name: &str, value: u64, labels: Labels<'_>);
    fn observe_duration_ms(&self, name: &str, duration_ms: u64, labels: Labels<'_>);
}

#[derive(Default)]
pub struct NoopMetrics;

impl MetricsSink for NoopMetrics {
    fn incr_counter(&self, _name: &str, _value: u64, _labels: Labels<'_>) {}

    fn observe_duration_ms(&self, _name: &str, _duration_ms: u64, _labels: Labels<'_>) {}
}

#[derive(Default)]
pub struct InMemoryMetrics {
    counters: DashMap<String, u64>,
    duration_sum_ms: DashMap<String, u64>,
    duration_count: DashMap<String, u64>,
}

impl InMemoryMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot_counters(&self) -> Vec<(String, u64)> {
        let mut items: Vec<_> = self
            .counters
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    }

    pub fn snapshot_durations(&self) -> Vec<(String, u64, u64)> {
        let mut items = Vec::new();
        for item in &self.duration_sum_ms {
            let key = item.key().clone();
            let sum = *item.value();
            let count = self.duration_count.get(&key).map_or(0, |v| *v.value());
            items.push((key, sum, count));
        }
        items.sort_by(|a, b| a.0.cmp(&b.0));
        items
    }
}

impl MetricsSink for InMemoryMetrics {
    fn incr_counter(&self, name: &str, value: u64, labels: Labels<'_>) {
        let key = format_metric_key(name, labels);
        self.counters
            .entry(key)
            .and_modify(|v| *v = v.saturating_add(value))
            .or_insert(value);
    }

    fn observe_duration_ms(&self, name: &str, duration_ms: u64, labels: Labels<'_>) {
        let key = format_metric_key(name, labels);

        self.duration_sum_ms
            .entry(key.clone())
            .and_modify(|v| *v = v.saturating_add(duration_ms))
            .or_insert(duration_ms);

        self.duration_count
            .entry(key)
            .and_modify(|v| *v = v.saturating_add(1))
            .or_insert(1);
    }
}

pub fn spawn_metrics_log_reporter(
    metrics: Arc<InMemoryMetrics>,
    interval: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;

            let counters = metrics.snapshot_counters();
            let durations = metrics.snapshot_durations();

            if !counters.is_empty() {
                log::info!("metrics.counters: {:?}", counters);
            }
            if !durations.is_empty() {
                log::info!("metrics.durations(sum_ms,count): {:?}", durations);
            }
        }
    })
}

pub fn format_metric_key(name: &str, labels: Labels<'_>) -> String {
    if labels.is_empty() {
        return name.to_string();
    }

    let mut labels_vec: Vec<_> = labels.iter().map(|(k, v)| (*k, *v)).collect();
    labels_vec.sort_by(|a, b| a.0.cmp(b.0));

    let mut out = String::with_capacity(name.len() + labels_vec.len() * 12);
    out.push_str(name);
    out.push('{');

    for (idx, (k, v)) in labels_vec.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }

    out.push('}');
    out
}

pub fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_key_has_stable_label_order() {
        let a = format_metric_key(
            "events_in_total",
            &[("plugin", "a"), ("platform", "onebot")],
        );
        let b = format_metric_key(
            "events_in_total",
            &[("platform", "onebot"), ("plugin", "a")],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn in_memory_metrics_accumulates_values() {
        let metrics = InMemoryMetrics::new();
        metrics.incr_counter("events_in_total", 1, &[("platform", "onebot")]);
        metrics.incr_counter("events_in_total", 2, &[("platform", "onebot")]);
        metrics.observe_duration_ms("plugin_handle_duration_ms", 10, &[]);
        metrics.observe_duration_ms("plugin_handle_duration_ms", 40, &[]);

        let counters = metrics.snapshot_counters();
        assert_eq!(counters.len(), 1);
        assert_eq!(counters[0].1, 3);

        let durations = metrics.snapshot_durations();
        assert_eq!(durations.len(), 1);
        assert_eq!(durations[0].1, 50);
        assert_eq!(durations[0].2, 2);
    }
}
