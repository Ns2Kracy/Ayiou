use crate::client::WeatherBundle;

pub fn build_weather_card_html(bundle: &WeatherBundle) -> String {
    let hi_points = polyline_points(&bundle.max_temps, 760.0, 120.0);
    let lo_points = polyline_points(&bundle.min_temps, 760.0, 120.0);

    let labels = bundle
        .dates
        .iter()
        .map(|d| format!("<span>{}</span>", escape_html(d)))
        .collect::<Vec<_>>()
        .join("");

    let hi_badges = bundle
        .max_temps
        .iter()
        .map(|t| format!("<span>{}°</span>", t))
        .collect::<Vec<_>>()
        .join("");

    let lo_badges = bundle
        .min_temps
        .iter()
        .map(|t| format!("<span>{}°</span>", t))
        .collect::<Vec<_>>()
        .join("");

    format!(
        r#"<!doctype html>
<html lang=\"zh-CN\">
<head>
<meta charset=\"utf-8\" />
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
<style>
* {{ box-sizing: border-box; }}
body {{ margin: 0; width: 1200px; height: 675px; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: radial-gradient(circle at 20% 10%, #b6e3ff 0%, #5f85d8 45%, #2f3f6a 100%); color: #ffffff; }}
.card {{ width: 1200px; height: 675px; padding: 42px; display: flex; flex-direction: column; justify-content: space-between; }}
.header {{ display: flex; justify-content: space-between; align-items: flex-start; }}
.city {{ font-size: 46px; font-weight: 700; letter-spacing: 1px; }}
.cond {{ margin-top: 10px; font-size: 22px; opacity: .92; }}
.now-temp {{ font-size: 92px; font-weight: 800; line-height: 1; text-shadow: 0 14px 28px rgba(10,20,40,.35); }}
.panel {{ margin-top: 14px; padding: 20px 24px; border-radius: 24px; background: linear-gradient(145deg, rgba(255,255,255,.22), rgba(255,255,255,.08)); backdrop-filter: blur(8px); border: 1px solid rgba(255,255,255,.2); }}
.line {{ width: 760px; height: 130px; margin-bottom: 12px; }}
.temps, .dates {{ width: 760px; display: grid; grid-template-columns: repeat({cols}, 1fr); gap: 10px; font-size: 20px; }}
.temps.lo {{ opacity: .84; margin-top: 6px; }}
.footer {{ font-size: 16px; opacity: .78; }}
</style>
</head>
<body>
  <div class=\"card\">
    <div class=\"header\">
      <div>
        <div class=\"city\">{city}</div>
        <div class=\"cond\">{cond}</div>
      </div>
      <div class=\"now-temp\">{temp}°</div>
    </div>

    <div class=\"panel\">
      <svg class=\"line\" viewBox=\"0 0 760 130\" preserveAspectRatio=\"none\">
        <polyline fill=\"none\" stroke=\"rgba(255,183,77,.95)\" stroke-width=\"4\" points=\"{hi}\" />
        <polyline fill=\"none\" stroke=\"rgba(159,220,255,.95)\" stroke-width=\"4\" points=\"{lo}\" />
      </svg>
      <div class=\"temps hi\">{hi_badges}</div>
      <div class=\"temps lo\">{lo_badges}</div>
      <div class=\"dates\">{labels}</div>
    </div>

    <div class=\"footer\">Data by QWeather · Rendered by Ayiou QWeather plugin</div>
  </div>
</body>
</html>"#,
        cols = bundle.dates.len().max(1),
        city = escape_html(&bundle.location_name),
        cond = escape_html(&bundle.now_text),
        temp = bundle.now_temp_c,
        hi = hi_points,
        lo = lo_points,
        hi_badges = hi_badges,
        lo_badges = lo_badges,
        labels = labels,
    )
}

pub fn polyline_points(temps: &[i32], width: f32, height: f32) -> String {
    if temps.is_empty() {
        return String::new();
    }

    let (min_t, max_t) = temps
        .iter()
        .fold((i32::MAX, i32::MIN), |(mn, mx), t| (mn.min(*t), mx.max(*t)));
    let range = (max_t - min_t).max(1) as f32;

    let step = if temps.len() == 1 {
        0.0
    } else {
        width / (temps.len() - 1) as f32
    };

    temps
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let x = if temps.len() == 1 {
                width / 2.0
            } else {
                idx as f32 * step
            };
            let ratio = (*t - min_t) as f32 / range;
            let y = height - (ratio * height);
            format!("{:.1},{:.1}", x, y)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bundle() -> WeatherBundle {
        WeatherBundle {
            location_name: "上海".to_string(),
            now_text: "多云".to_string(),
            now_temp_c: 23,
            min_temps: vec![18, 19, 20],
            max_temps: vec![25, 26, 27],
            dates: vec!["03-02".into(), "03-03".into(), "03-04".into()],
        }
    }

    #[test]
    fn html_contains_key_weather_fields() {
        let html = build_weather_card_html(&sample_bundle());
        assert!(html.contains("上海"));
        assert!(html.contains("多云"));
        assert!(html.contains("23"));
        assert!(html.contains("linear-gradient"));
    }

    #[test]
    fn polyline_points_should_span_canvas() {
        let points = polyline_points(&[10, 20, 30], 300.0, 120.0);
        assert_eq!(points.split_whitespace().count(), 3);
        assert!(points.contains("0,"));
    }
}
