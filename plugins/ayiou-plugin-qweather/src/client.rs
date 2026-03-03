use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct WeatherBundle {
    pub location_name: String,
    pub now_text: String,
    pub now_temp_c: i32,
    pub min_temps: Vec<i32>,
    pub max_temps: Vec<i32>,
    pub dates: Vec<String>,
}

pub struct QWeatherClient {
    http: Client,
    api_key: String,
    lang: String,
    unit: String,
}

impl QWeatherClient {
    pub fn new_from_env() -> Result<Self> {
        let api_key = std::env::var("QWEATHER_API_KEY")
            .context("Missing QWEATHER_API_KEY env var. Please set your QWeather key.")?;
        let lang = std::env::var("QWEATHER_LANG").unwrap_or_else(|_| "zh".to_string());
        let unit = std::env::var("QWEATHER_UNIT").unwrap_or_else(|_| "m".to_string());

        Ok(Self {
            http: Client::new(),
            api_key,
            lang,
            unit,
        })
    }

    pub async fn query_weather(&self, location: &str) -> Result<WeatherBundle> {
        let city = self.lookup_city(location).await?;
        let now = self.fetch_now(&city.id).await?;
        let daily = self.fetch_daily_3d(&city.id).await?;

        if daily.is_empty() {
            bail!("QWeather returned empty forecast for {}", city.name);
        }

        let mut min_temps = Vec::with_capacity(daily.len());
        let mut max_temps = Vec::with_capacity(daily.len());
        let mut dates = Vec::with_capacity(daily.len());

        for item in &daily {
            min_temps.push(parse_temp(&item.temp_min, "tempMin")?);
            max_temps.push(parse_temp(&item.temp_max, "tempMax")?);
            dates.push(short_date(&item.fx_date));
        }

        Ok(WeatherBundle {
            location_name: city.name,
            now_text: now.text,
            now_temp_c: parse_temp(&now.temp, "temp")?,
            min_temps,
            max_temps,
            dates,
        })
    }

    async fn lookup_city(&self, location: &str) -> Result<LookupLocation> {
        let resp: LookupResponse = self
            .http
            .get("https://geoapi.qweather.com/v2/city/lookup")
            .query(&[
                ("location", location),
                ("key", self.api_key.as_str()),
                ("lang", self.lang.as_str()),
            ])
            .send()
            .await
            .context("Failed to call QWeather city lookup")?
            .error_for_status()
            .context("QWeather city lookup returned non-2xx status")?
            .json()
            .await
            .context("Failed to decode QWeather city lookup response")?;

        if resp.code != "200" {
            bail!("QWeather city lookup failed with code {}", resp.code);
        }

        resp.location
            .into_iter()
            .next()
            .context("No matched city found. Try a more specific location name.")
    }

    async fn fetch_now(&self, location_id: &str) -> Result<NowData> {
        let resp: NowResponse = self
            .http
            .get("https://devapi.qweather.com/v7/weather/now")
            .query(&[
                ("location", location_id),
                ("key", self.api_key.as_str()),
                ("lang", self.lang.as_str()),
                ("unit", self.unit.as_str()),
            ])
            .send()
            .await
            .context("Failed to call QWeather weather/now")?
            .error_for_status()
            .context("QWeather weather/now returned non-2xx status")?
            .json()
            .await
            .context("Failed to decode QWeather weather/now response")?;

        if resp.code != "200" {
            bail!("QWeather now failed with code {}", resp.code);
        }

        resp.now.context("QWeather now payload is empty")
    }

    async fn fetch_daily_3d(&self, location_id: &str) -> Result<Vec<DailyData>> {
        let resp: ForecastResponse = self
            .http
            .get("https://devapi.qweather.com/v7/weather/3d")
            .query(&[
                ("location", location_id),
                ("key", self.api_key.as_str()),
                ("lang", self.lang.as_str()),
                ("unit", self.unit.as_str()),
            ])
            .send()
            .await
            .context("Failed to call QWeather weather/3d")?
            .error_for_status()
            .context("QWeather weather/3d returned non-2xx status")?
            .json()
            .await
            .context("Failed to decode QWeather weather/3d response")?;

        if resp.code != "200" {
            bail!("QWeather 3d failed with code {}", resp.code);
        }

        Ok(resp.daily)
    }
}

fn parse_temp(raw: &str, field: &str) -> Result<i32> {
    raw.parse::<i32>()
        .with_context(|| format!("Invalid {} value: {}", field, raw))
}

fn short_date(raw: &str) -> String {
    if raw.len() >= 10 {
        return raw[5..10].to_string();
    }
    raw.to_string()
}

#[derive(Debug, Deserialize)]
struct LookupResponse {
    code: String,
    #[serde(default)]
    location: Vec<LookupLocation>,
}

#[derive(Debug, Deserialize)]
struct LookupLocation {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct NowResponse {
    code: String,
    now: Option<NowData>,
}

#[derive(Debug, Deserialize)]
struct NowData {
    temp: String,
    text: String,
}

#[derive(Debug, Deserialize)]
struct ForecastResponse {
    code: String,
    #[serde(default)]
    daily: Vec<DailyData>,
}

#[derive(Debug, Deserialize)]
struct DailyData {
    #[serde(rename = "fxDate")]
    fx_date: String,
    #[serde(rename = "tempMin")]
    temp_min: String,
    #[serde(rename = "tempMax")]
    temp_max: String,
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct LookupResponse {
        code: String,
        location: Vec<LookupLocation>,
    }

    #[derive(Debug, Deserialize)]
    struct LookupLocation {
        id: String,
        name: String,
    }

    #[test]
    fn parse_lookup_json() {
        let raw = r#"{
          "code": "200",
          "location": [
            {"id":"101010100","name":"北京"}
          ]
        }"#;

        let parsed: LookupResponse = serde_json::from_str(raw).expect("json should parse");
        assert_eq!(parsed.code, "200");
        assert_eq!(parsed.location[0].id, "101010100");
        assert_eq!(parsed.location[0].name, "北京");
    }
}
