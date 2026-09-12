//! Walk a syn file and collect non-test function spans.

// dry-rs:ignore-file. intentional parallel language frontend; keep separate.
use super::cfg_filter::{CfgUniverse, is_harness_mod_name, skip_cfg, skip_item};
use super::count_metric;
use crap_core::{FunctionComplexity, Metric};
use std::path::Path;
use syn::visit::{self, Visit};
use syn::{ImplItemFn, ItemFn, ItemImpl, ItemTrait, TraitItemFn};

pub(super) struct FunctionVisitor<'a> {
    pub(super) file: &'a Path,
    pub(super) metric: Metric,
    pub(super) out: Vec<FunctionComplexity>,
    pub(super) impl_type: Option<String>,
    pub(super) cfg: &'a CfgUniverse,
}

impl FunctionVisitor<'_> {
    fn push_fn(&mut self, name: String, start_line: usize, end_line: usize, body: &syn::Block) {
        self.out.push(FunctionComplexity {
            file: self.file.to_path_buf(),
            name,
            start_line,
            end_line,
            complexity: count_metric(self.metric, body),
        });
    }
}

fn type_name(ty: &syn::Type) -> String {
    if let Some(name) = named_type(ty) {
        return name;
    }
    if let Some(name) = sequence_type(ty) {
        return name;
    }
    pointer_type(ty).unwrap_or_else(|| "<impl>".into())
}

fn named_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(tp) => Some(
            tp.path
                .segments
                .last()
                .map_or_else(|| "<impl>".into(), |seg| seg.ident.to_string()),
        ),
        syn::Type::TraitObject(obj) => Some(trait_object_name(obj)),
        syn::Type::Paren(paren) => Some(type_name(&paren.elem)),
        _ => None,
    }
}

fn sequence_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Tuple(tuple) => {
            let inner = tuple.elems.iter().map(type_name).collect::<Vec<_>>().join(", ");
            Some(format!("({inner})"))
        }
        syn::Type::Slice(slice) => Some(format!("[{}]", type_name(&slice.elem))),
        syn::Type::Array(array) => Some(format!("[{}; _]", type_name(&array.elem))),
        _ => None,
    }
}

fn pointer_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Ptr(ptr) => Some(ptr_type_name(ptr)),
        syn::Type::Reference(reference) => Some(ref_type_name(reference)),
        _ => None,
    }
}

fn ptr_type_name(ptr: &syn::TypePtr) -> String {
    let kind = match ptr.mutability {
        syn::PointerMutability::Mut(_) => "*mut",
        syn::PointerMutability::Const(_) => "*const",
    };
    format!("{kind} {}", type_name(&ptr.elem))
}

fn ref_type_name(reference: &syn::TypeReference) -> String {
    let mutability = if reference.mutability.is_some() {
        "mut "
    } else {
        ""
    };
    format!("&{mutability}{}", type_name(&reference.elem))
}

fn trait_object_name(obj: &syn::TypeTraitObject) -> String {
    let first = obj.bounds.iter().find_map(|bound| {
        let syn::TypeParamBound::Trait(trait_bound) = bound else {
            return None;
        };
        trait_bound.path.segments.last().map(|seg| seg.ident.to_string())
    });
    first.map_or_else(|| "<impl>".into(), |name| format!("dyn {name}"))
}

pub(super) fn qualified_name(prefix: Option<&str>, method: &str) -> String {
    prefix.map_or_else(|| method.to_owned(), |ty| format!("{ty}::{method}"))
}

impl<'ast> Visit<'ast> for FunctionVisitor<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if skip_item(&node.attrs, self.cfg) {
            return;
        }
        let start_line = node.sig.fn_token.span.start().line;
        let end_line = node.block.brace_token.span.close().end().line;
        self.push_fn(
            node.sig.ident.to_string(),
            start_line,
            end_line,
            &node.block,
        );
        visit::visit_item_fn(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        if skip_cfg(&node.attrs, self.cfg) {
            return;
        }
        let prev = self.impl_type.take();
        self.impl_type = Some(type_name(&node.self_ty));
        visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if skip_item(&node.attrs, self.cfg) {
            return;
        }
        let method = node.sig.ident.to_string();
        let name = qualified_name(self.impl_type.as_deref(), &method);
        let start_line = node.sig.fn_token.span.start().line;
        let end_line = node.block.brace_token.span.close().end().line;
        self.push_fn(name, start_line, end_line, &node.block);
        visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        if skip_cfg(&node.attrs, self.cfg) {
            return;
        }
        visit::visit_item_trait(self, node);
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        if skip_item(&node.attrs, self.cfg) {
            return;
        }
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if is_harness_mod_name(&node.ident) || skip_cfg(&node.attrs, self.cfg) {
            return;
        }
        visit::visit_item_mod(self, node);
    }
}

#[cfg(test)]
mod tests {
    use super::qualified_name;
    use crate::complexity::analyze_source;
    use crap_core::{FunctionComplexity, Metric};
    use std::path::Path;

    fn snippet(src: &str, metric: Metric) -> Vec<FunctionComplexity> {
        let parsed = analyze_source(Path::new("t.rs"), src, metric);
        assert!(parsed.is_ok(), "{parsed:?}");
        parsed.unwrap_or_default()
    }

    fn cyclo(src: &str) -> Vec<FunctionComplexity> {
        snippet(src, Metric::Cyclomatic)
    }

    #[test]
    fn analyze_file_rejects_a_missing_path() {
        let result = crate::complexity::analyze_file(
            Path::new("/no/such/crap-rs-analyze.rs"),
            Metric::Cyclomatic,
            &[],
        );
        assert!(result.is_err(), "{result:?}");
    }

    #[test]
    fn impl_methods_are_prefixed() {
        let src = "struct Foo; impl Foo { fn bar(&self) { if true {} } }";
        let fns = cyclo(src);
        assert_eq!(fns[0].name, "Foo::bar");
        assert_eq!(fns[0].complexity, 2);
    }

    #[test]
    fn trait_default_is_skipped() {
        let fns = cyclo("trait T { fn m(&self) { if true {} } }");
        assert!(fns.is_empty());
    }

    #[test]
    fn nested_fn_is_scored_separately() {
        let fns = cyclo("fn outer() { fn inner() { if true {} } }");
        let expected = [("outer", 1), ("inner", 2)];
        assert_eq!(fns.len(), expected.len());
        for (got, (name, complexity)) in fns.iter().zip(expected) {
            assert_eq!((got.name.as_str(), got.complexity), (name, complexity));
        }
    }

    #[test]
    fn qualified_name_uses_prefix_when_present() {
        assert_eq!(qualified_name(Some("Foo"), "bar"), "Foo::bar");
        assert_eq!(qualified_name(None, "bar"), "bar");
    }

    #[test]
    fn nested_mod_functions_are_scored() {
        let fns = cyclo("mod inner { fn f() { if true {} } }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
    }

    #[test]
    fn trait_declaration_without_default_is_skipped() {
        let fns = cyclo("trait T { fn m(&self); }");
        assert!(fns.is_empty());
    }

    #[test]
    fn test_impl_method_is_skipped() {
        let src = "struct Foo; impl Foo { #[test] fn t() { if true {} } fn keep(&self) {} }";
        let fns = cyclo(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "Foo::keep");
    }

    #[test]
    fn test_trait_method_is_skipped() {
        let src = "trait T { #[test] fn t(&self) { if true {} } fn keep(&self) {} }";
        assert!(cyclo(src).is_empty());
    }

    #[test]
    fn tuple_impl_method_is_prefixed() {
        let fns = cyclo("impl (u8, u8) { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "(u8, u8)::m");
    }

    #[test]
    fn reference_impl_method_is_prefixed() {
        let fns = cyclo("impl &str { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "&str::m");
    }

    #[test]
    fn pointer_slice_and_dyn_impls_are_prefixed() {
        let cases = [
            ("impl *const u8 { fn m() {} }", "*const u8::m"),
            ("impl *mut u8 { fn m() {} }", "*mut u8::m"),
            ("impl [u8] { fn m() {} }", "[u8]::m"),
            ("impl [u8; 2] { fn m() {} }", "[u8; _]::m"),
            ("impl dyn Send { fn m() {} }", "dyn Send::m"),
            ("impl (u8) { fn m() {} }", "u8::m"),
        ];
        for (src, name) in cases {
            let fns = cyclo(src);
            assert_eq!(fns.len(), 1, "{src}");
            assert_eq!(fns[0].name, name, "{src}");
        }
    }

    #[test]
    fn bare_fn_impl_falls_back_to_impl() {
        let fns = cyclo("impl fn() { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "<impl>::m");
    }

    #[test]
    fn mut_reference_impl_method_is_prefixed() {
        let fns = cyclo("impl &mut str { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "&mut str::m");
    }

    #[test]
    fn dyn_lifetime_then_trait_uses_the_trait() {
        let fns = cyclo("impl dyn 'static + Send { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "dyn Send::m");
    }

    #[test]
    fn empty_type_path_falls_back_to_impl() {
        let ty = syn::Type::Path(syn::TypePath {
            attrs: Vec::new(),
            qself: None,
            path: syn::Path {
                leading_colon: None,
                segments: syn::punctuated::Punctuated::new(),
            },
        });
        assert_eq!(super::type_name(&ty), "<impl>");
    }

    #[test]
    fn dyn_lifetime_only_falls_back_to_impl() {
        let mut bounds = syn::punctuated::Punctuated::new();
        bounds.push(syn::TypeParamBound::Lifetime(syn::Lifetime::new(
            "'static",
            proc_macro2::Span::call_site(),
        )));
        let ty = syn::Type::TraitObject(syn::TypeTraitObject {
            attrs: Vec::new(),
            dyn_token: Some(syn::token::Dyn::default()),
            bounds,
        });
        assert_eq!(super::type_name(&ty), "<impl>");
    }
}
