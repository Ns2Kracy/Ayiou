use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

pub type BoxedTaskFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;
pub type ScheduledJob = Arc<dyn Fn() -> BoxedTaskFuture + Send + Sync + 'static>;

pub fn schedule_job<F, Fut>(f: F) -> ScheduledJob
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    Arc::new(move || Box::pin(f()))
}

#[async_trait]
pub trait Scheduler: Send + Sync + 'static {
    fn schedule_once(&self, delay: Duration, job: ScheduledJob) -> Result<TaskId>;
    fn schedule_interval(&self, interval: Duration, job: ScheduledJob) -> Result<TaskId>;
    fn schedule_cron(&self, expr: &str, job: ScheduledJob) -> Result<TaskId>;

    async fn cancel(&self, id: TaskId) -> Result<bool>;
    async fn shutdown(&self) -> Result<()>;
}

pub struct TokioScheduler {
    next_id: AtomicU64,
    tasks: DashMap<TaskId, JoinHandle<()>>,
}

impl Default for TokioScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl TokioScheduler {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            tasks: DashMap::new(),
        }
    }

    fn reap_finished(&self) {
        self.tasks.retain(|_, handle| !handle.is_finished());
    }

    fn next_task_id(&self) -> TaskId {
        TaskId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    fn track(&self, id: TaskId, handle: JoinHandle<()>) {
        self.tasks.insert(id, handle);
    }
}

#[async_trait]
impl Scheduler for TokioScheduler {
    fn schedule_once(&self, delay: Duration, job: ScheduledJob) -> Result<TaskId> {
        let id = self.next_task_id();
        self.reap_finished();
        let handle = tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            (job)().await;
        });

        self.track(id, handle);
        Ok(id)
    }

    fn schedule_interval(&self, interval: Duration, job: ScheduledJob) -> Result<TaskId> {
        if interval.is_zero() {
            return Err(anyhow!("interval must be greater than zero"));
        }

        let id = self.next_task_id();
        self.reap_finished();
        let handle = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                (job)().await;
            }
        });

        self.track(id, handle);
        Ok(id)
    }

    fn schedule_cron(&self, expr: &str, job: ScheduledJob) -> Result<TaskId> {
        use std::str::FromStr;
        let schedule = cron::Schedule::from_str(expr)
            .map_err(|e| anyhow!("invalid cron expression `{}`: {}", expr, e))?;

        let id = self.next_task_id();
        self.reap_finished();
        let handle = tokio::spawn(async move {
            loop {
                let now = Utc::now();
                let Some(next) = schedule.after(&now).next() else {
                    break;
                };

                let wait = (next - now)
                    .to_std()
                    .unwrap_or_else(|_| Duration::from_millis(0));
                tokio::time::sleep(wait).await;
                (job)().await;
            }
        });

        self.track(id, handle);
        Ok(id)
    }

    async fn cancel(&self, id: TaskId) -> Result<bool> {
        if let Some((_, handle)) = self.tasks.remove(&id) {
            handle.abort();
            return Ok(true);
        }
        Ok(false)
    }

    async fn shutdown(&self) -> Result<()> {
        let ids: Vec<TaskId> = self.tasks.iter().map(|entry| *entry.key()).collect();
        for id in ids {
            let _ = self.cancel(id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn scheduler_executes_once_task() {
        let scheduler = TokioScheduler::new();
        let hit = Arc::new(AtomicUsize::new(0));
        let hit2 = hit.clone();

        scheduler
            .schedule_once(
                Duration::from_millis(20),
                schedule_job(move || {
                    let hit = hit2.clone();
                    async move {
                        hit.fetch_add(1, Ordering::SeqCst);
                    }
                }),
            )
            .unwrap();

        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(hit.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn scheduler_cancel_interval_task() {
        let scheduler = TokioScheduler::new();
        let hit = Arc::new(AtomicUsize::new(0));
        let hit2 = hit.clone();

        let id = scheduler
            .schedule_interval(
                Duration::from_millis(20),
                schedule_job(move || {
                    let hit = hit2.clone();
                    async move {
                        hit.fetch_add(1, Ordering::SeqCst);
                    }
                }),
            )
            .unwrap();

        tokio::time::sleep(Duration::from_millis(70)).await;
        let _ = scheduler.cancel(id).await.unwrap();
        let captured = hit.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(40)).await;

        assert!(captured >= 1);
        assert_eq!(hit.load(Ordering::SeqCst), captured);
    }
}
