use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEvent {
    pub bot_id: String,
    pub name: String,
    pub value: u64,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub bot_id: String,
    pub name: String,
    pub value: u64,
    pub labels: HashMap<String, String>,
}

#[derive(Clone, Default)]
pub struct MetricsStore {
    counters: Arc<DashMap<String, MetricPoint>>,
}

impl MetricsStore {
    pub fn upsert(&self, event: MetricEvent) {
        let key = metric_key(&event.bot_id, &event.name, &event.labels);
        self.counters
            .entry(key)
            .and_modify(|existing| {
                existing.value = existing.value.saturating_add(event.value);
            })
            .or_insert(MetricPoint {
                bot_id: event.bot_id,
                name: event.name,
                value: event.value,
                labels: event.labels,
            });
    }

    pub fn query_by_bot(&self, bot_id: &str) -> Vec<MetricPoint> {
        let mut points: Vec<_> = self
            .counters
            .iter()
            .filter_map(|entry| {
                let value = entry.value();
                if value.bot_id == bot_id {
                    Some(value.clone())
                } else {
                    None
                }
            })
            .collect();
        points.sort_by(|a, b| a.name.cmp(&b.name));
        points
    }
}

fn metric_key(bot_id: &str, name: &str, labels: &HashMap<String, String>) -> String {
    if labels.is_empty() {
        return format!("{bot_id}:{name}");
    }

    let mut items: Vec<_> = labels.iter().collect();
    items.sort_by(|a, b| a.0.cmp(b.0));

    let labels_part = items
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");

    format!("{bot_id}:{name}{{{labels_part}}}")
}
