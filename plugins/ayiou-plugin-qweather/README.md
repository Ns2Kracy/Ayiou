# ayiou-plugin-qweather

QWeather plugin for Ayiou. It provides `/weather <city>` command and returns:
- text weather summary
- H5-rendered weather card image (PNG)

## Features

- QWeather city lookup + current weather + 3-day forecast
- Beautiful weather card rendered from HTML/CSS
- OneBot image reply using `base64://`
- Fallback to text when browser screenshot is unavailable

## Environment Variables

- `QWEATHER_API_KEY` (required): QWeather API key
- `QWEATHER_LANG` (optional, default: `zh`)
- `QWEATHER_UNIT` (optional, default: `m`)
- `WEATHER_SHOT_BIN` (optional): browser executable path for screenshot, e.g. `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`

## Runtime Requirement

Need one of these in runtime environment:
- `google-chrome`
- `chromium`
- `chromium-browser`
- or set `WEATHER_SHOT_BIN`

## Usage

Register plugin:

```rust
use ayiou_plugin_qweather::QWeatherPlugin;

let bot = Bot::<OneBotV11Adapter>::new()
    .with_onebot_defaults()
    .register_plugin(QWeatherPlugin);
```

Command:

```text
/weather 北京
```

## Notes

- If image rendering fails, plugin still returns text summary.
- City name matching depends on QWeather lookup results.
