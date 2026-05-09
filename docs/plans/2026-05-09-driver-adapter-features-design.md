# Driver And Adapter Feature Split Design

## Context

`ayiou` currently exposes all built-in drivers and adapters by default:

- `ayiou::driver::{console, mock, wsclient}`
- `ayiou::adapter::{console, onebot}`
- convenience aliases such as `ConsoleBot` and `OneBotV11Bot`

That forces developers who only need the core runtime and plugin APIs to compile transport and protocol implementations they did not select.

## Decision

Use an explicit Cargo feature split first, with names and module boundaries that can later move into separate crates.

`ayiou` will use `default = []`. The core runtime, plugin system, model types, macros, and common `Driver` / `Adapter` traits remain available without features.

Feature groups:

- `driver-console`
- `driver-wsclient`
- `driver-mock`
- `adapter-console`
- `adapter-onebot-v11`

Adapter features enable their required built-in driver feature:

- `adapter-console = ["driver-console"]`
- `adapter-onebot-v11 = ["driver-wsclient"]`

## Public API

The top-level `driver` and `adapter` modules remain available only when at least one corresponding feature is enabled. Individual modules are gated by their feature:

- `ayiou::driver::console` behind `driver-console`
- `ayiou::driver::wsclient` behind `driver-wsclient`
- `ayiou::driver::mock` behind `driver-mock`
- `ayiou::adapter::console` behind `adapter-console`
- `ayiou::adapter::onebot` behind `adapter-onebot-v11`

Convenience bot aliases and constructors follow adapter features:

- `ConsoleBot` and `ConsoleBot::console()` behind `adapter-console`
- `OneBotV11Bot`, `OneBotV11Bot::ws()`, and `OneBotV11Bot::ws_with_token()` behind `adapter-onebot-v11`

## Dependency Shape

Dependencies needed only by optional implementations should become optional:

- `futures-util`, `tokio-tungstenite`, and `url` for `driver-wsclient`

Shared runtime dependencies remain normal dependencies.

## Tests

Feature-specific integration tests should declare `required-features` in `Cargo.toml`.

Add compile-fail coverage that proves default `ayiou` does not expose built-in `adapter` and `driver` modules. Keep one no-default-features runtime smoke test for the core surface.

## Future Crate Split

This layout intentionally mirrors a future workspace split:

- `ayiou-driver-console`
- `ayiou-driver-wsclient`
- `ayiou-adapter-console`
- `ayiou-adapter-onebot-v11`

The current change stops at Cargo features to avoid premature versioning and publishing overhead.
