#[cfg(not(any(
    feature = "adapter-console",
    feature = "adapter-onebot-v11",
    feature = "driver-console",
    feature = "driver-mock",
    feature = "driver-wsclient"
)))]
#[test]
fn default_build_does_not_export_builtin_drivers_or_adapters() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/default_no_adapter_driver.rs");
}

#[cfg(not(any(
    feature = "adapter-console",
    feature = "adapter-onebot-v11",
    feature = "driver-console",
    feature = "driver-mock",
    feature = "driver-wsclient"
)))]
#[test]
fn core_surface_is_available_without_default_features() {
    assert_eq!(ayiou::RuntimeState::default(), ayiou::RuntimeState::Stopped);
    assert_eq!(
        std::any::type_name::<ayiou::core::adapter::AdapterRuntime>(),
        "ayiou::core::adapter::AdapterRuntime"
    );
}
