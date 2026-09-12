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

/// Returns true for Rust harness modules named `tests` or `test`.
pub(super) fn is_harness_mod_name(ident: &syn::Ident) -> bool {
    ident == "tests" || ident == "test"
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
#[path = "cfg_filter_tests.rs"]
mod tests;
