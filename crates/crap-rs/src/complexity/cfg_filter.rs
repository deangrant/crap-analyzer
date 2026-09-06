//! Skip `#[test]` and inactive `#[cfg]` items when collecting functions.

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
        true
    }

    fn eval_name_value(&self, nv: &syn::MetaNameValue) -> bool {
        let Some(value) = lit_str(&nv.value) else {
            return true;
        };
        if nv.path.is_ident("feature") {
            return self.features.contains(&value);
        }
        if nv.path.is_ident("target_os") {
            return host_target_os(&value);
        }
        true
    }

    fn eval_flag(name: &str) -> bool {
        match name {
            "test" => false,
            "unix" => cfg!(unix),
            "windows" => cfg!(windows),
            _ => true,
        }
    }
}

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

pub(super) fn skip_item(attrs: &[syn::Attribute], cfg: &CfgUniverse) -> bool {
    has_attr(attrs, "test") || !cfg.all_cfgs_active(attrs)
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

const HOST_OS: &str = if cfg!(target_os = "linux") {
    "linux"
} else if cfg!(target_os = "macos") {
    "macos"
} else if cfg!(target_os = "windows") {
    "windows"
} else if cfg!(target_os = "ios") {
    "ios"
} else if cfg!(target_os = "android") {
    "android"
} else if cfg!(target_os = "freebsd") {
    "freebsd"
} else {
    ""
};

fn host_target_os(os: &str) -> bool {
    !HOST_OS.is_empty() && HOST_OS == os
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
        if cfg!(unix) {
            assert_eq!(fns.len(), 1);
            assert_eq!(fns[0].name, "f");
        } else {
            assert!(fns.is_empty());
        }
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
    fn cfg_unknown_list_and_key_are_kept() {
        assert_eq!(cyclo("#[cfg(weird(x))] fn f() {}").len(), 1);
        assert_eq!(cyclo("#[cfg(foo = \"bar\")] fn f() {}").len(), 1);
        assert_eq!(cyclo("#[cfg(foo = 1)] fn f() {}").len(), 1);
    }
}
