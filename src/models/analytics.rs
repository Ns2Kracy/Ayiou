use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// Link click analytics
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct LinkClick {
    pub id: Uuid,
    pub link_id: Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub country: Option<String>,
    pub city: Option<String>,
    pub device_type: Option<String>,
    pub browser: Option<String>,
    pub os: Option<String>,
    pub clicked_at: DateTime<Utc>,
}

// Analytics-related responses
#[derive(Debug, Serialize)]
pub struct AnalyticsResponse {
    pub total_clicks: i64,
    pub clicks_today: i64,
    pub clicks_this_week: i64,
    pub clicks_this_month: i64,
    pub top_countries: Vec<CountryStats>,
    pub top_devices: Vec<DeviceStats>,
    pub click_history: Vec<ClickHistoryPoint>,
}

#[derive(Debug, Serialize)]
pub struct CountryStats {
    pub country: String,
    pub clicks: i64,
}

#[derive(Debug, Serialize)]
pub struct DeviceStats {
    pub device_type: String,
    pub clicks: i64,
}

#[derive(Debug, Serialize)]
pub struct ClickHistoryPoint {
    pub date: DateTime<Utc>,
    pub clicks: i64,
}
