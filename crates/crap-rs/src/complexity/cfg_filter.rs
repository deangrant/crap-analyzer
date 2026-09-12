//! Skip harness attrs (`#[test]` / `#[bench]` / `#[…::test]`) and inactive `#[cfg]`.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use std::collections::HashSet;

/// Feature names treated as enabled when evaluating `#[cfg]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct CfgUniverse {
    features: HashSet<String>,
}

impl CfgUniverse {
    pub(super) fn new(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            features: features.into_iter().map(Into::into).collect(),
        }
    }

    fn all_cfgs_active(&self, attrs: &[syn::Attribute]) -> bool {
        attrs
            .iter()
            .filter(|attr| attr.path().is_ident("cfg"))
            .all(|attr| self.eval_attr(attr))
    }

    fn eval_attr(&self, attr: &syn::Attribute) -> bool {
        attr.parse_args::<syn::Meta>().is_ok_and(|meta| self.eval_meta(&meta))
    }

    fn eval_meta(&self, meta: &syn::Meta) -> bool {
        match meta {
            syn::Meta::Path(path) => {
                path.get_ident().is_some_and(|id| Self::eval_flag(&id.to_string()))
            }
            syn::Meta::List(list) => self.eval_list(list),
            syn::Meta::NameValue(nv) => self.eval_name_value(nv),
        }
    }

    fn eval_list(&self, list: &syn::MetaList) -> bool {
        if list.path.is_ident("all") {
            return cfg_list(list).iter().all(|item| self.eval_meta(item));
        }
        if list.path.is_ident("any") {
            return cfg_list(list).iter().any(|item| self.eval_meta(item));
        }
        if list.path.is_ident("not") {
            return cfg_list(list).first().is_none_or(|item| !self.eval_meta(item));
        }
        false
    }

    fn eval_name_value(&self, nv: &syn::MetaNameValue) -> bool {
        let Some(value) = lit_str(&nv.value) else {
            return false;
        };
        if nv.path.is_ident("feature") {
            return self.features.contains(&value);
        }
        host_cfg_value(&nv.path, &value)
    }

    fn eval_flag(name: &str) -> bool {
        // Bare `test` / `proc_macro` stay false: this analyzer is not a rustc
        // test or proc-macro build. Unknown flags are also false.
        match name {
            "unix" => cfg!(unix),
            "windows" => cfg!(windows),
            "debug_assertions" => cfg!(debug_assertions),
            _ => false,
        }
    }
}

fn is_harness_attr(attr: &syn::Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|seg| seg.ident == "test" || seg.ident == "bench")
}

fn has_harness_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(is_harness_attr)
}

pub(super) fn skip_item(attrs: &[syn::Attribute], cfg: &CfgUniverse) -> bool {
    has_harness_attr(attrs) || !cfg.all_cfgs_active(attrs)
}

pub(super) fn skip_cfg(attrs: &[syn::Attribute], cfg: &CfgUniverse) -> bool {
    !cfg.all_cfgs_active(attrs)
}

fn cfg_list(list: &syn::MetaList) -> Vec<syn::Meta> {
    list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .map(|items| items.into_iter().collect())
        .unwrap_or_default()
}

fn lit_str(expr: &syn::Expr) -> Option<String> {
    if let syn::Expr::Lit(syn::ExprLit {
        lit: syn::Lit::Str(s),
        ..
    }) = expr
    {
        return Some(s.value());
    }
    None
}

const HOST_OS: &str = std::env::consts::OS;
const HOST_ARCH: &str = std::env::consts::ARCH;

const HOST_FAMILY: &str = if cfg!(target_family = "unix") {
    "unix"
} else if cfg!(target_family = "windows") {
    "windows"
} else {
    ""
};

const HOST_POINTER_WIDTH: &str = if cfg!(target_pointer_width = "64") {
    "64"
} else if cfg!(target_pointer_width = "32") {
    "32"
} else if cfg!(target_pointer_width = "16") {
    "16"
} else {
    ""
};

const HOST_ENDIAN: &str = if cfg!(target_endian = "little") {
    "little"
} else if cfg!(target_endian = "big") {
    "big"
} else {
    ""
};

const HOST_ENV: &str = if cfg!(target_env = "gnu") {
    "gnu"
} else if cfg!(target_env = "musl") {
    "musl"
} else if cfg!(target_env = "msvc") {
    "msvc"
} else if cfg!(target_env = "sgx") {
    "sgx"
} else if cfg!(target_env = "uclibc") {
    "uclibc"
} else if cfg!(target_env = "newlib") {
    "newlib"
} else {
    ""
};

const HOST_VENDOR: &str = if cfg!(target_vendor = "apple") {
    "apple"
} else if cfg!(target_vendor = "pc") {
    "pc"
} else if cfg!(target_vendor = "unknown") {
    "unknown"
} else if cfg!(target_vendor = "fortanix") {
    "fortanix"
} else {
    ""
};

fn host_cfg_value(path: &syn::Path, value: &str) -> bool {
    if path.is_ident("target_os") {
        return host_eq(HOST_OS, value);
    }
    if path.is_ident("target_arch") {
        return host_eq(HOST_ARCH, value);
    }
    host_cfg_width(path, value)
}

fn host_cfg_width(path: &syn::Path, value: &str) -> bool {
    if path.is_ident("target_family") {
        return host_eq(HOST_FAMILY, value);
    }
    if path.is_ident("target_pointer_width") {
        return host_eq(HOST_POINTER_WIDTH, value);
    }
    host_cfg_extra(path, value)
}

fn host_cfg_extra(path: &syn::Path, value: &str) -> bool {
    if path.is_ident("target_endian") {
        return host_eq(HOST_ENDIAN, value);
    }
    if path.is_ident("target_env") {
        return HOST_ENV == value;
    }
    if path.is_ident("target_vendor") {
        return HOST_VENDOR == value;
    }
    false
}

fn host_eq(host: &str, value: &str) -> bool {
    !host.is_empty() && host == value
}

#[cfg(test)]
mod tests {
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
    fn ungated_tests_module_helpers_are_scored() {
        let fns = cyclo("mod tests { fn helper() { if true {} } } fn keep() {}");
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
}
