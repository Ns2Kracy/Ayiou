use anyhow::Result;
use ayiou::prelude::*;
use base64::Engine;
use log::warn;

use crate::card::build_weather_card_html;
use crate::client::{QWeatherClient, WeatherBundle};
use crate::render::render_html_to_png;

#[derive(Plugin)]
#[plugin(
    name = "qweather",
    command = "weather",
    prefix = "/",
    description = "QWeather forecast plugin"
)]
pub struct QWeatherPlugin;

impl QWeatherPlugin {
    pub async fn execute(&self, ctx: &Ctx) -> Result<()> {
        let location = ctx.command_args().unwrap_or_default();
        let location = location.trim();
        if location.is_empty() {
            ctx.reply_text("Usage: /weather <city>").await?;
            return Ok(());
        }

        let client = match QWeatherClient::new_from_env() {
            Ok(c) => c,
            Err(err) => {
                ctx.reply_text(format!("QWeather config error: {}", err))
                    .await?;
                return Ok(());
            }
        };

        let bundle = match client.query_weather(location).await {
            Ok(bundle) => bundle,
            Err(err) => {
                ctx.reply_text(format!("Weather query failed: {}", err))
                    .await?;
                return Ok(());
            }
        };

        ctx.reply_text(build_summary(&bundle)).await?;

        let html = build_weather_card_html(&bundle);
        match render_html_to_png(&html).await {
            Ok(bytes) => {
                let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
                let message = Message::Array(vec![MessageSegment::Image {
                    file: format!("base64://{}", b64),
                    image_type: None,
                    url: None,
                }]);
                ctx.reply(message).await?;
            }
            Err(err) => {
                warn!("render weather card failed: {}", err);
                ctx.reply_text(
                    "天气图生成失败，仅返回了文字结果。请检查 Chrome/Chromium 是否可用。",
                )
                .await?;
            }
        }

        Ok(())
    }
}

fn build_summary(bundle: &WeatherBundle) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{} 现在 {} {}°C",
        bundle.location_name, bundle.now_text, bundle.now_temp_c
    ));

    for i in 0..bundle.dates.len() {
        let d = &bundle.dates[i];
        let lo = bundle.min_temps.get(i).copied().unwrap_or_default();
        let hi = bundle.max_temps.get(i).copied().unwrap_or_default();
        lines.push(format!("{}: {}~{}°C", d, lo, hi));
    }

    lines.join("\n")
}
