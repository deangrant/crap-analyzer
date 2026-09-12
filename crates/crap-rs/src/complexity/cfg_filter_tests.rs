use super::super::analyze_source_cfg;
use super::CfgUniverse;
use crap_core::{FunctionComplexity, Metric};
use std::path::Path;

fn cyclo(src: &str) -> Vec<FunctionComplexity> {
    cyclo_features(src, &CfgUniverse::new(std::iter::empty::<String>()))
}

fn cyclo_features(src: &str, cfg: &CfgUniverse) -> Vec<FunctionComplexity> {
    let parsed = analyze_source_cfg(Path::new("t.rs"), src, Metric::Cyclomatic, cfg);
    assert!(parsed.is_ok(), "{parsed:?}");
    parsed.unwrap_or_default()
}

#[test]
fn test_functions_are_skipped() {
    let fns = cyclo("#[test] fn t() { if true {} } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn bench_functions_are_skipped() {
    let fns = cyclo("#[bench] fn b() { if true {} } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn path_qualified_test_attrs_are_skipped() {
    let fns = cyclo("#[tokio::test] fn t() { if true {} } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn ungated_tests_module_helpers_are_skipped() {
    let fns = cyclo("mod tests { fn helper() { if true {} } } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn ungated_test_module_helpers_are_skipped() {
    let fns = cyclo("mod test { fn helper() { if true {} } } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn other_module_helpers_are_still_scored() {
    let fns = cyclo("mod helpers { fn helper() { if true {} } } fn keep() {}");
    assert_eq!(fns.len(), 2);
    let names: Vec<_> = fns.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"keep"));
}

#[test]
fn cfg_test_helper_is_skipped() {
    let fns = cyclo("#[cfg(test)] fn helper() { if true {} } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_test_impl_is_skipped() {
    let src = "struct Foo; #[cfg(test)] impl Foo { fn bar(&self) { if true {} } }";
    assert!(cyclo(src).is_empty());
}

#[test]
fn cfg_all_test_is_skipped() {
    let src = "#[cfg(all(test, feature = \"x\"))] fn helper() { if true {} } fn keep() {}";
    let fns = cyclo(src);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_any_test_or_unix_is_kept() {
    let fns = cyclo("#[cfg(any(test, unix))] fn f() { if true {} }");
    #[cfg(unix)]
    {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }
    #[cfg(not(unix))]
    assert!(fns.is_empty());
}

#[test]
fn cfg_test_mod_is_skipped() {
    let src = "#[cfg(test)] mod tests { fn helper() { if true {} } } fn keep() {}";
    let fns = cyclo(src);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_any_test_only_is_skipped() {
    let fns = cyclo("#[cfg(any(test))] fn helper() { if true {} } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_feature_is_skipped_when_disabled() {
    let fns = cyclo("#[cfg(feature = \"x\")] fn f() { if true {} } fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_feature_off_is_skipped() {
    let fns = cyclo("#[cfg(feature = \"off\")] fn gated() {} fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_feature_is_kept_when_enabled() {
    let cfg = CfgUniverse::new(["x"]);
    let fns = cyclo_features("#[cfg(feature = \"x\")] fn f() { if true {} }", &cfg);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_not_feature_is_kept_when_disabled() {
    let src = "#[cfg(not(feature = \"x\"))] fn f() { if true {} }";
    let fns = cyclo(src);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_test_trait_is_skipped() {
    let src = "#[cfg(test)] trait T { fn m(&self) { if true {} } }";
    assert!(cyclo(src).is_empty());
}

#[test]
fn cfg_target_os_host_is_kept() {
    let src = format!(
        "#[cfg(target_os = \"{os}\")] fn f() {{}}",
        os = super::HOST_OS
    );
    let fns = cyclo(&src);
    assert!(!super::HOST_OS.is_empty());
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_target_os_unknown_is_skipped() {
    let fns = cyclo("#[cfg(target_os = \"unknown-os\")] fn f() {}");
    assert!(fns.is_empty());
}

#[test]
fn cfg_target_os_linux_is_host_gated() {
    let fns = cyclo("#[cfg(target_os = \"linux\")] fn linux() {} fn keep() {}");
    #[cfg(target_os = "linux")]
    assert_eq!(fns.len(), 2);
    #[cfg(not(target_os = "linux"))]
    {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }
}

#[test]
fn cfg_unknown_list_and_key_are_skipped() {
    assert!(cyclo("#[cfg(weird(x))] fn f() {}").is_empty());
    assert!(cyclo("#[cfg(foo = \"bar\")] fn f() {}").is_empty());
    assert!(cyclo("#[cfg(foo = 1)] fn f() {}").is_empty());
}

#[test]
fn cfg_windows_is_host_gated() {
    let fns = cyclo("#[cfg(windows)] fn w() {} fn keep() {}");
    #[cfg(windows)]
    assert_eq!(fns.len(), 2);
    #[cfg(not(windows))]
    {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }
}

#[test]
fn cfg_unknown_flag_is_skipped() {
    let fns = cyclo("#[cfg(foo)] fn f() {} fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}

#[test]
fn cfg_debug_assertions_is_host_gated() {
    let fns = cyclo("#[cfg(debug_assertions)] fn f() {} fn keep() {}");
    #[cfg(debug_assertions)]
    assert_eq!(fns.len(), 2);
    #[cfg(not(debug_assertions))]
    {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }
}

#[test]
fn cfg_not_debug_assertions_is_host_gated() {
    let fns = cyclo("#[cfg(not(debug_assertions))] fn f() {} fn keep() {}");
    #[cfg(not(debug_assertions))]
    assert_eq!(fns.len(), 2);
    #[cfg(debug_assertions)]
    {
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "keep");
    }
}

#[test]
fn cfg_target_arch_host_is_kept() {
    let src = format!(
        "#[cfg(target_arch = \"{arch}\")] fn f() {{}}",
        arch = super::HOST_ARCH
    );
    let fns = cyclo(&src);
    assert!(!super::HOST_ARCH.is_empty());
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_target_arch_unknown_is_skipped() {
    let fns = cyclo("#[cfg(target_arch = \"unknown-arch\")] fn f() {}");
    assert!(fns.is_empty());
}

#[test]
fn cfg_target_family_host_is_kept() {
    let src = format!(
        "#[cfg(target_family = \"{family}\")] fn f() {{}}",
        family = super::HOST_FAMILY
    );
    let fns = cyclo(&src);
    assert!(!super::HOST_FAMILY.is_empty());
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_target_pointer_width_host_is_kept() {
    let src = format!(
        "#[cfg(target_pointer_width = \"{width}\")] fn f() {{}}",
        width = super::HOST_POINTER_WIDTH
    );
    let fns = cyclo(&src);
    assert!(!super::HOST_POINTER_WIDTH.is_empty());
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_target_endian_host_is_kept() {
    let src = format!(
        "#[cfg(target_endian = \"{endian}\")] fn f() {{}}",
        endian = super::HOST_ENDIAN
    );
    let fns = cyclo(&src);
    assert!(!super::HOST_ENDIAN.is_empty());
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_target_env_host_is_kept() {
    let src = format!(
        "#[cfg(target_env = \"{env}\")] fn f() {{}}",
        env = super::HOST_ENV
    );
    let fns = cyclo(&src);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_target_vendor_host_is_kept() {
    let src = format!(
        "#[cfg(target_vendor = \"{vendor}\")] fn f() {{}}",
        vendor = super::HOST_VENDOR
    );
    let fns = cyclo(&src);
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "f");
}

#[test]
fn cfg_proc_macro_flag_is_skipped() {
    let fns = cyclo("#[cfg(proc_macro)] fn f() {} fn keep() {}");
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "keep");
}
