# Service Dependency Manifest Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let runtime plugins declare required and optional service dependencies in `RuntimePluginManifest`.

**Architecture:** Add `ServiceKey` to `core::service` and store required/optional service keys on `RuntimePluginManifest`. During `RuntimePluginEngine::init_all()`, check required services against the shared `ServiceRegistry` before calling plugin `init()`.

**Tech Stack:** Rust 2024, `TypeId`, `type_name`, existing `RuntimeService`, existing `ServiceRegistry`, existing async plugin runtime tests.

---

### Task 1: Service Keys

**Files:**
- Modify: `ayiou/src/core/service.rs`

**Step 1: Write the failing test**

Add a test that creates `ServiceKey::of::<CounterService>()` and verifies it has the service `TypeId` and Rust type name.

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p ayiou --no-default-features core::service::tests::service_key_captures_type_metadata
```

Expected: FAIL because `ServiceKey` does not exist.

**Step 3: Implement minimal code**

Add:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ServiceKey {
    type_id: TypeId,
    type_name: &'static str,
}
```

with `of::<S>()`, `type_id()`, and `type_name()` methods.

**Step 4: Verify**

Run the targeted test.

### Task 2: Manifest Declarations

**Files:**
- Modify: `ayiou/src/core/plugin_system.rs`

**Step 1: Write failing tests**

Add tests that:

- `RuntimePluginManifest::require_service::<TestCounterService>()` records the required service key.
- `RuntimePluginEngine::init_all()` fails before plugin `init()` when a required service is missing.
- `RuntimePluginEngine::init_all()` succeeds when the required service exists.
- Optional service declarations do not fail startup.

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test -p ayiou --no-default-features required_service
```

Expected: FAIL because the manifest methods and runtime check do not exist.

**Step 3: Implement minimal code**

Add `required_services` and `optional_services` to `RuntimePluginManifest`, builder methods `require_service::<S>()` and `optional_service::<S>()`, and a helper that checks required services during `RuntimePluginEngine::init_all()`.

**Step 4: Verify**

Run:

```bash
cargo test -p ayiou --no-default-features required_service
cargo test -p ayiou --no-default-features optional_service
```

### Task 3: Docs And Examples

**Files:**
- Modify: `README.md`
- Modify: `docs/content/docs/getting-started.mdx`
- Modify: `docs/content/docs/kitchen-sink-example.mdx`
- Modify: `ayiou/examples/kitchen_sink.rs`

**Step 1: Update docs**

Show manifest-level service declarations in runtime service docs.

**Step 2: Update example**

Update the kitchen sink plugin manifest to declare `GreetingService` as required.

**Step 3: Verify**

Run:

```bash
cargo test -p ayiou --no-default-features --example kitchen_sink
cargo test -p ayiou --no-default-features
cargo test -p ayiou --all-features
cargo build -p ayiou --all-features --examples
```

### Task 4: Final Verification

Run:

```bash
cargo fmt --all
cargo test -p ayiou --no-default-features
cargo test -p ayiou --all-features
cargo build -p ayiou --all-features --examples
git diff --check
```

Commit the implementation after verification.
