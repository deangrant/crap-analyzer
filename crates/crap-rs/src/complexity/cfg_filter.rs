//! Skip `#[test]` and `#[cfg(test)]` items when collecting functions.

fn has_attr(attrs: &[syn::Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

pub(super) fn is_test_item(attrs: &[syn::Attribute]) -> bool {
    has_attr(attrs, "test") || is_cfg_test(attrs)
}

pub(super) fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr.parse_args::<syn::Meta>().is_ok_and(|meta| cfg_is_test_only(&meta))
    })
}

fn cfg_is_test_only(meta: &syn::Meta) -> bool {
    match meta {
        syn::Meta::Path(path) => path.is_ident("test"),
        syn::Meta::List(list) if list.path.is_ident("all") => {
            cfg_list(list).iter().any(cfg_is_test_only)
        }
        syn::Meta::List(list) if list.path.is_ident("any") => {
            let items = cfg_list(list);
            !items.is_empty() && items.iter().all(cfg_is_test_only)
        }
        _ => false,
    }
}

fn cfg_list(list: &syn::MetaList) -> Vec<syn::Meta> {
    list.parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
        .map(|items| items.into_iter().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use crate::complexity::analyze_source;
    use crap_core::{FunctionComplexity, Metric};
    use std::path::Path;

    fn cyclo(src: &str) -> Vec<FunctionComplexity> {
        let parsed = analyze_source(Path::new("t.rs"), src, Metric::Cyclomatic);
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
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
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
    fn cfg_feature_is_kept() {
        let fns = cyclo("#[cfg(feature = \"x\")] fn f() { if true {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }

    #[test]
    fn cfg_test_trait_is_skipped() {
        let src = "#[cfg(test)] trait T { fn m(&self) { if true {} } }";
        assert!(cyclo(src).is_empty());
    }
}
