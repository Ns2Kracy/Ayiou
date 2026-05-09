# Driver And Adapter Feature Split Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make built-in drivers and adapters opt-in Cargo features so the default `ayiou` dependency exposes only core runtime, plugin, macro, and trait APIs.

**Architecture:** Keep the current single `ayiou` crate. Gate built-in `adapter` and `driver` modules with explicit features, and make WebSocket-only dependencies optional behind `driver-wsclient`. Adapter features enable their required driver features.

**Tech Stack:** Rust 2024, Cargo features, integration test `required-features`, `trybuild` for compile-fail API absence checks, `cargo test`.

---

### Task 1: Add Compile-Fail Coverage For Default API Surface

**Files:**
- Modify: `ayiou/Cargo.toml`
- Create: `ayiou/tests/default_surface.rs`
- Create: `ayiou/tests/ui/default_no_adapter_driver.rs`
- Create: `ayiou/tests/ui/default_no_adapter_driver.stderr`

**Step 1: Write the failing tests**

Add `trybuild` as a dev dependency and a test that expects this snippet to fail under default features:

```rust
fn main() {
    let _ = std::any::type_name::<ayiou::adapter::console::adapter::ConsoleAdapter>();
    let _ = std::any::type_name::<ayiou::driver::wsclient::WsDriver>();
}
```

Also add a small no-default-features smoke test that imports core APIs.

**Step 2: Run tests to verify failure**

Run: `cargo test -p ayiou default_surface --no-default-features`

Expected: the compile-fail test does not fail yet because `adapter` and `driver` are still exposed.

### Task 2: Add Feature Definitions And Module Gates

**Files:**
- Modify: `ayiou/Cargo.toml`
- Modify: `ayiou/src/lib.rs`
- Modify: `ayiou/src/adapter.rs`
- Modify: `ayiou/src/driver.rs`
- Modify: `ayiou/src/core/adapter.rs`

**Step 1: Implement feature flags**

Add:

```toml
[features]
default = []
driver-console = []
driver-wsclient = ["dep:futures-util", "dep:tokio-tungstenite", "dep:url"]
driver-mock = []
adapter-console = ["driver-console"]
adapter-onebot-v11 = ["driver-wsclient"]
```

Make `futures-util`, `tokio-tungstenite`, and `url` optional.

Gate top-level modules:

```rust
#[cfg(any(feature = "adapter-console", feature = "adapter-onebot-v11"))]
pub mod adapter;
pub mod core;
#[cfg(any(feature = "driver-console", feature = "driver-wsclient", feature = "driver-mock"))]
pub mod driver;
```

Gate child modules and convenience bot aliases with matching features.

In `core::adapter` tests, use `crate::driver::mock::MockDriver` only behind `driver-mock`.

**Step 2: Run tests**

Run: `cargo test -p ayiou default_surface --no-default-features`

Expected: PASS.

### Task 3: Mark Feature-Specific Tests And Verify Each Feature

**Files:**
- Modify: `ayiou/Cargo.toml`

**Step 1: Add integration test metadata**

Use `[[test]] required-features` entries for:

- `wsdriver_token`: `["driver-wsclient"]`
- `onebot_adapter_token`: `["adapter-onebot-v11"]`
- `onebot_sender`: `["adapter-onebot-v11"]`

Other tests remain featureless if they only use core APIs.

**Step 2: Run focused feature tests**

Run:

```bash
cargo test -p ayiou --no-default-features
cargo test -p ayiou --no-default-features --features driver-wsclient wsdriver_token
cargo test -p ayiou --no-default-features --features adapter-onebot-v11 onebot
cargo test -p ayiou --no-default-features --features adapter-console --example kitchen_sink
```

Expected: all pass.

### Task 4: Update Docs

**Files:**
- Modify: `README.md`
- Modify: `docs/content/docs/getting-started.mdx`
- Modify: `docs/content/docs/index.mdx`
- Modify as needed: docs examples that import OneBot adapter APIs

**Step 1: Document explicit features**

Add examples:

```toml
ayiou = { version = "0.1", features = ["adapter-onebot-v11"] }
```

and:

```toml
ayiou = { version = "0.1", features = ["adapter-console"] }
```

**Step 2: Run final verification**

Run:

```bash
cargo test -p ayiou --no-default-features
cargo test -p ayiou --no-default-features --features adapter-console,adapter-onebot-v11,driver-mock
cargo test -p ayiou --all-features
cargo build -p ayiou --all-features --examples
```

Expected: all pass.
