use anyhow::Result;
use chrono::Utc;
use cron::Schedule;
use std::{future::Future, pin::Pin, str::FromStr};
use tokio::time::{Duration, sleep};

use crate::onebot::api::Api;

/// 类型擦除的 Future
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Cron 任务 trait
pub trait CronTask: Send + Sync + 'static {
    /// 任务名称
    fn name(&self) -> &str {
        "unnamed"
    }

    /// 执行任务
    fn run(&self, api: Api) -> BoxFuture<'static, Result<()>>;
}

/// Cron 任务包装器
pub struct CronJob {
    schedule: Schedule,
    task: Box<dyn CronTask>,
}

impl CronJob {
    pub fn new<T: CronTask>(cron_expr: &str, task: T) -> Result<Self> {
        let schedule = Schedule::from_str(cron_expr)?;
        Ok(Self {
            schedule,
            task: Box::new(task),
        })
    }

    pub fn name(&self) -> &str {
        self.task.name()
    }

    /// 启动 cron 任务循环
    pub async fn start(self, api: Api) {
        let name = self.task.name().to_string();
        tracing::info!("Starting cron task: {}", name);

        loop {
            let now = Utc::now();
            if let Some(next) = self.schedule.upcoming(Utc).next() {
                let duration = (next - now).to_std().unwrap_or(Duration::from_secs(1));
                sleep(duration).await;

                tracing::debug!("Running cron task: {}", name);
                if let Err(err) = self.task.run(api.clone()).await {
                    tracing::error!("Cron task {} failed: {}", name, err);
                }
            } else {
                break;
            }
        }
    }
}

/// 从闭包创建 Cron 任务的包装器
pub struct CronHandler<F> {
    name: String,
    handler: F,
}

impl<F, Fut> CronTask for CronHandler<F>
where
    F: Fn(Api) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&self, api: Api) -> BoxFuture<'static, Result<()>> {
        Box::pin((self.handler)(api))
    }
}

/// Cron 任务构建器
pub struct CronBuilder<F> {
    cron_expr: String,
    name: String,
    handler: F,
}

impl<F, Fut> CronBuilder<F>
where
    F: Fn(Api) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    pub fn new(cron_expr: impl Into<String>, handler: F) -> Self {
        Self {
            cron_expr: cron_expr.into(),
            name: "unnamed".to_string(),
            handler,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn build(self) -> Result<CronJob> {
        let task = CronHandler {
            name: self.name,
            handler: self.handler,
        };
        CronJob::new(&self.cron_expr, task)
    }
}

/// 创建 Cron 任务构建器
///
/// # Example
/// ```ignore
/// let job = cron("0 * * * * *", |api| async move {
///     // 每分钟执行一次
///     api.send_group_msg(123456, &Message::String("定时消息".into())).await?;
///     Ok(())
/// })
/// .name("heartbeat")
/// .build()?;
/// ```
pub fn cron<F, Fut>(cron_expr: impl Into<String>, handler: F) -> CronBuilder<F>
where
    F: Fn(Api) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<()>> + Send + 'static,
{
    CronBuilder::new(cron_expr, handler)
}

/// Cron 任务管理器
pub struct CronScheduler {
    jobs: Vec<CronJob>,
}

impl CronScheduler {
    pub fn new() -> Self {
        Self { jobs: vec![] }
    }

    /// 添加 cron 任务
    pub fn job(mut self, job: CronJob) -> Self {
        tracing::info!("Registered cron task: {}", job.name());
        self.jobs.push(job);
        self
    }

    /// 启动所有 cron 任务
    pub fn start(self, api: Api) {
        for job in self.jobs {
            let api = api.clone();
            tokio::spawn(async move {
                job.start(api).await;
            });
        }
    }
}

impl Default for CronScheduler {
    fn default() -> Self {
        Self::new()
    }
}
