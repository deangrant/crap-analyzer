//! Walk a syn file and collect non-test function spans.

use super::cfg_filter::{is_cfg_test, is_test_item};
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
    pub(super) trait_name: Option<String>,
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

fn impl_type_name(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    tp.path.segments.last().map(|seg| seg.ident.to_string())
}

pub(super) fn qualified_name(prefix: Option<&str>, method: &str) -> String {
    prefix.map_or_else(|| method.to_owned(), |ty| format!("{ty}::{method}"))
}

impl<'ast> Visit<'ast> for FunctionVisitor<'_> {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        if is_test_item(&node.attrs) {
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
        if is_cfg_test(&node.attrs) {
            return;
        }
        let prev = self.impl_type.take();
        self.impl_type = impl_type_name(&node.self_ty);
        visit::visit_item_impl(self, node);
        self.impl_type = prev;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        if is_test_item(&node.attrs) {
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
        if is_cfg_test(&node.attrs) {
            return;
        }
        let prev = self.trait_name.take();
        self.trait_name = Some(node.ident.to_string());
        visit::visit_item_trait(self, node);
        self.trait_name = prev;
    }

    fn visit_trait_item_fn(&mut self, node: &'ast TraitItemFn) {
        if is_test_item(&node.attrs) {
            return;
        }
        if let Some(body) = &node.default {
            let method = node.sig.ident.to_string();
            let name = qualified_name(self.trait_name.as_deref(), &method);
            let start_line = node.sig.fn_token.span.start().line;
            let end_line = body.brace_token.span.close().end().line;
            self.push_fn(name, start_line, end_line, body);
        }
        visit::visit_trait_item_fn(self, node);
    }

    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
        if !is_cfg_test(&node.attrs) {
            visit::visit_item_mod(self, node);
        }
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
    fn impl_methods_are_prefixed() {
        let src = "struct Foo; impl Foo { fn bar(&self) { if true {} } }";
        let fns = cyclo(src);
        assert_eq!(fns[0].name, "Foo::bar");
        assert_eq!(fns[0].complexity, 2);
    }

    #[test]
    fn trait_default_is_scored() {
        let fns = cyclo("trait T { fn m(&self) { if true {} } }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "T::m");
        assert_eq!(fns[0].complexity, 2);
    }

    #[test]
    fn nested_fn_is_scored_separately() {
        let fns = cyclo("fn outer() { fn inner() { if true {} } }");
        assert_eq!(fns.len(), 2);
        assert_eq!(fns[0].name, "outer");
        assert_eq!(fns[0].complexity, 1);
        assert_eq!(fns[1].name, "inner");
        assert_eq!(fns[1].complexity, 2);
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
        let fns = cyclo(src);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "T::keep");
    }

    #[test]
    fn tuple_impl_method_is_unprefixed() {
        let fns = cyclo("impl (u8, u8) { fn m() {} }");
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "m");
    }
}
