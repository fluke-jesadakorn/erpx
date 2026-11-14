//! AST checks for implemented owner crates. Runs as an explicit catalogue target.
use std::{collections::BTreeSet, fs, path::Path};
use syn::{UseTree, Visibility, parse::Parser, visit::Visit};

/// Only the fully parsed metadata used by owner-generated contracts is
/// admitted; conditional code and arbitrary helper attributes stay closed.
fn examined_contract_attribute(attribute: &syn::Attribute) -> bool {
    let syn::Meta::List(list) = &attribute.meta else {
        return false;
    };
    let parser = syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
    let Ok(items) = parser.parse2(list.tokens.clone()) else {
        return false;
    };
    if items.len() != 2 || items.trailing_punct() {
        return false;
    }
    let mut items = items.iter();
    let Some(syn::Meta::NameValue(feature)) = items.next() else {
        return false;
    };
    if !feature.path.is_ident("feature")
        || !matches!(&feature.value, syn::Expr::Lit(value) if matches!(&value.lit, syn::Lit::Str(value) if value.value() == "contracts"))
    {
        return false;
    }
    let Some(syn::Meta::List(payload)) = items.next() else {
        return false;
    };
    if payload.path.is_ident("derive") {
        let parser = syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated;
        let Ok(paths) = parser.parse2(payload.tokens.clone()) else {
            return false;
        };
        if paths.is_empty() || paths.trailing_punct() {
            return false;
        }
        let mut seen = BTreeSet::new();
        for path in paths {
            if path.leading_colon.is_some()
                || path
                    .segments
                    .iter()
                    .any(|s| !matches!(s.arguments, syn::PathArguments::None))
            {
                return false;
            }
            let parts: Vec<_> = path.segments.iter().map(|s| s.ident.to_string()).collect();
            let name = match parts.as_slice() {
                [a, b] if a == "ts_rs" && b == "TS" => "ts_rs::TS",
                [a, b] if a == "utoipa" && b == "ToSchema" => "utoipa::ToSchema",
                _ => return false,
            };
            if !seen.insert(name) {
                return false;
            }
        }
        return true;
    }
    if payload.path.is_ident("ts") {
        let parse_ts = |input: syn::parse::ParseStream<'_>| -> syn::Result<bool> {
            if input.peek(syn::Token![type]) {
                input.parse::<syn::Token![type]>()?;
                input.parse::<syn::Token![=]>()?;
                let value = input.parse::<syn::LitStr>()?.value();
                // Exact literal types used by the reviewed owner contracts.
                // Arbitrary unions/expressions stay rejected.
                return Ok(matches!(
                    value.as_str(),
                    "number" | "number | null" | "0" | "\"customer\""
                ) && input.is_empty());
            }
            if input.parse::<syn::Ident>()? != "optional" {
                return Ok(false);
            }
            if input.is_empty() {
                return Ok(true);
            }
            input.parse::<syn::Token![,]>()?;
            input.parse::<syn::Token![type]>()?;
            input.parse::<syn::Token![=]>()?;
            Ok(input.parse::<syn::LitStr>()?.value() == "number | null" && input.is_empty())
        };
        return parse_ts.parse2(payload.tokens.clone()).unwrap_or(false);
    }
    if payload.path.is_ident("schema") {
        let parse_schema = |input: syn::parse::ParseStream<'_>| -> syn::Result<bool> {
            let mut keys = BTreeSet::new();
            while !input.is_empty() {
                let key = input.parse::<syn::Ident>()?.to_string();
                if !keys.insert(key.clone()) {
                    return Ok(false);
                }
                input.parse::<syn::Token![=]>()?;
                match key.as_str() {
                    "value_type" => {
                        let ty: syn::Type = input.parse()?;
                        let syn::Type::Path(ty) = ty else {
                            return Ok(false);
                        };
                        if ty.qself.is_some()
                            || ty.path.leading_colon.is_some()
                            || ty.path.segments.len() != 1
                        {
                            return Ok(false);
                        }
                        let segment = &ty.path.segments[0];
                        let valid = if segment.ident == "i64" {
                            matches!(segment.arguments, syn::PathArguments::None)
                        } else if segment.ident == "Option" {
                            match &segment.arguments {
                                syn::PathArguments::AngleBracketed(args)
                                    if args.colon2_token.is_none()
                                        && !args.args.trailing_punct()
                                        && args.args.len() == 1 =>
                                {
                                    matches!(args.args.first(), Some(syn::GenericArgument::Type(syn::Type::Path(inner))) if inner.qself.is_none() && inner.path.is_ident("i64"))
                                }
                                _ => false,
                            }
                        } else {
                            false
                        };
                        if !valid {
                            return Ok(false);
                        }
                    }
                    "minimum" | "maximum" => {
                        // Signed bounds are explicit integer literals; no
                        // expression or call is admitted.
                        let _negative = input.parse::<syn::Token![-]>().is_ok();
                        input.parse::<syn::LitInt>()?;
                    }
                    _ => return Ok(false),
                }
                if !input.is_empty() {
                    input.parse::<syn::Token![,]>()?;
                    if input.is_empty() {
                        return Ok(false);
                    }
                }
            }
            Ok(keys.len() == 3
                && keys.contains("value_type")
                && keys.contains("minimum")
                && keys.contains("maximum"))
        };
        return parse_schema.parse2(payload.tokens.clone()).unwrap_or(false);
    }
    false
}

struct MatchArguments {
    expression: syn::Expr,
    pattern: syn::Pat,
    guard: Option<syn::Expr>,
}

impl syn::parse::Parse for MatchArguments {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let expression = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let pattern = syn::Pat::parse_multi_with_leading_vert(input)?;
        let guard = if input.peek(syn::Token![if]) {
            input.parse::<syn::Token![if]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        if input.peek(syn::Token![,]) {
            input.parse::<syn::Token![,]>()?;
        }
        Ok(Self {
            expression,
            pattern,
            guard,
        })
    }
}

fn paths(tree: &UseTree, prefix: Vec<String>, output: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(path.ident.to_string());
            paths(&path.tree, prefix, output);
        }
        UseTree::Name(name) => {
            let mut prefix = prefix;
            prefix.push(name.ident.to_string());
            output.push(prefix);
        }
        UseTree::Rename(rename) => {
            let mut prefix = prefix;
            prefix.push(rename.ident.to_string());
            output.push(prefix);
        }
        UseTree::Glob(_) => output.push(prefix),
        UseTree::Group(group) => {
            for item in &group.items {
                paths(item, prefix.clone(), output);
            }
        }
    }
}

/// Items compiled only under the test harness are not part of the portable
/// business core, so their fixtures do not constrain production layering.
fn test_item(attributes: &[syn::Attribute]) -> bool {
    test_only(attributes)
        || attributes.iter().any(|attribute| {
            let path = attribute.path();
            path.is_ident("test")
                || path
                    .segments
                    .last()
                    .is_some_and(|segment| segment.ident == "test")
        })
}

fn test_only(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        let syn::Meta::List(list) = &attribute.meta else {
            return false;
        };
        attribute.path().is_ident("cfg") && cfg_is_test_only(&list.tokens.to_string())
    })
}

fn cfg_is_test_only(tokens: &str) -> bool {
    let compact: String = tokens
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if compact == "test" {
        return true;
    }
    // `all(test, ...)` only exists with the test cfg enabled. `any(test, ...)`
    // and `not(test)` still compile production code, so they stay inspected.
    let Some(inner) = compact
        .strip_prefix("all(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        return false;
    };
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, character) in inner.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                if &inner[start..index] == "test" {
                    return true;
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    &inner[start..] == "test"
}

/// `json!` builds a value from pure data. Its token stream is not a Rust
/// expression, so inspect the tokens for effectful roots instead of dropping
/// the macro unexamined. Quoted JSON strings are data and cannot introduce a
/// path effect.
fn json_argument_effect(tokens: &str) -> Option<String> {
    let mut cleaned = String::with_capacity(tokens.len());
    let mut characters = tokens.chars();
    while let Some(character) = characters.next() {
        if character == '"' {
            cleaned.push('"');
            let mut escaped = false;
            for inner in characters.by_ref() {
                if escaped {
                    escaped = false;
                } else if inner == '\\' {
                    escaped = true;
                } else if inner == '"' {
                    break;
                }
            }
            continue;
        }
        if !character.is_whitespace() {
            cleaned.push(character);
        }
    }
    for root in [
        "std::",
        "sqlx::",
        "tokio::",
        "axum::",
        "reqwest::",
        "openssl::",
        "lopdf::",
        "tower::",
        "tracing::",
        "rand::",
        "getrandom::",
        "uuid::",
        "time::",
        "chrono::",
    ] {
        if cleaned.contains(root) {
            return Some(root.trim_end_matches("::").to_owned());
        }
    }
    for macro_name in [
        "env!",
        "option_env!",
        "include!",
        "include_str!",
        "include_bytes!",
    ] {
        if cleaned.contains(macro_name) {
            return Some(macro_name.trim_end_matches('!').to_owned());
        }
    }
    if let Some(index) = cleaned.find("erpx_server_") {
        let rest = &cleaned[index..];
        if !rest.starts_with("erpx_server_shared") {
            return Some("erpx_server".to_owned());
        }
    }
    None
}

struct BoundaryVisitor {
    module: Vec<String>,
    layer: Option<String>,
    issues: BTreeSet<String>,
}

impl BoundaryVisitor {
    fn inspect_path(&mut self, path: &[String]) {
        let Some(first) = path.first() else {
            return;
        };
        if first.starts_with("erpx_server_") && path.get(1).map(String::as_str) != Some("public") {
            self.issues
                .insert(format!("owner-private-import:{}", path.join("::")));
        }
        let Some(layer) = self.layer.as_deref() else {
            return;
        };
        if matches!(
            first.as_str(),
            "axum"
                | "tokio"
                | "sqlx"
                | "reqwest"
                | "openssl"
                | "lopdf"
                | "tower"
                | "tracing"
                | "rand"
                | "getrandom"
                | "uuid"
        ) || (first == "std"
            && path.get(1).is_some_and(|part| {
                matches!(part.as_str(), "env" | "fs" | "net" | "process" | "thread")
            }))
            || path.starts_with(&["std".to_owned(), "time".to_owned(), "SystemTime".to_owned()])
        {
            self.issues
                .insert(format!("portable-core-effect:{}", path.join("::")));
        }
        if matches!(first.as_str(), "time" | "chrono")
            && path
                .last()
                .is_some_and(|part| matches!(part.as_str(), "now" | "now_utc" | "now_local"))
        {
            self.issues
                .insert(format!("portable-core-clock:{}", path.join("::")));
        }
        let mut local = self.module.clone();
        let mut index = 0;
        match first.as_str() {
            "crate" => {
                local.clear();
                index = 1;
            }
            "self" => index = 1,
            "super" => {
                while path.get(index).is_some_and(|part| part == "super") {
                    local.pop();
                    index += 1;
                }
            }
            _ => local.clear(),
        }
        if index > 0 {
            local.extend_from_slice(&path[index..]);
            if local.first().is_some_and(|target| {
                target == "adapters"
                    || target == "public"
                    || layer == "domain" && target == "application"
            }) {
                self.issues
                    .insert(format!("inward-layer-violation:{}", path.join("::")));
            }
        }
        if first.starts_with("erpx_server_") && first != "erpx_server_shared" {
            self.issues.insert(format!(
                "portable-cross-owner-contract-required:{}",
                path.join("::")
            ));
        }
    }
}

impl<'ast> Visit<'ast> for BoundaryVisitor {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, item);
    }
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_fn(self, item);
    }
    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_impl_item_fn(self, item);
    }
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_impl(self, item);
    }
    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_trait(self, item);
    }
    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_struct(self, item);
    }
    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_enum(self, item);
    }
    fn visit_item_const(&mut self, item: &'ast syn::ItemConst) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_const(self, item);
    }
    fn visit_item_static(&mut self, item: &'ast syn::ItemStatic) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_static(self, item);
    }
    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        if test_item(&item.attrs) {
            return;
        }
        syn::visit::visit_item_type(self, item);
    }
    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        if test_item(&item.attrs) {
            return;
        }
        let mut imports = Vec::new();
        paths(&item.tree, Vec::new(), &mut imports);
        for path in imports {
            self.inspect_path(&path);
        }
        syn::visit::visit_item_use(self, item);
    }
    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.inspect_path(
            &path
                .segments
                .iter()
                .map(|part| part.ident.to_string())
                .collect::<Vec<_>>(),
        );
        syn::visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, item: &'ast syn::Macro) {
        if self.layer.is_some() {
            let name = item
                .path
                .segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if !matches!(
                name.as_str(),
                "format"
                    | "write"
                    | "writeln"
                    | "vec"
                    | "matches"
                    | "assert"
                    | "assert_eq"
                    | "assert_ne"
                    | "debug_assert"
                    | "debug_assert_eq"
                    | "debug_assert_ne"
                    | "panic"
                    | "unreachable"
                    | "json"
                    | "serde_json::json"
                    | "format_description"
            ) {
                self.issues
                    .insert(format!("portable-core-macro-not-approved:{name}"));
            } else if name == "matches" {
                match syn::parse2::<MatchArguments>(item.tokens.clone()) {
                    Ok(arguments) => {
                        self.visit_expr(&arguments.expression);
                        self.visit_pat(&arguments.pattern);
                        if let Some(guard) = arguments.guard {
                            self.visit_expr(&guard);
                        }
                    }
                    Err(_) => {
                        self.issues
                            .insert("portable-core-macro-arguments-unexamined:matches".to_owned());
                    }
                }
            } else if let Ok(arguments) =
                syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated
                    .parse2(item.tokens.clone())
            {
                for argument in arguments {
                    self.visit_expr(&argument);
                }
            } else if matches!(
                name.as_str(),
                "json" | "serde_json::json" | "format_description"
            ) {
                if let Some(effect) = json_argument_effect(&item.tokens.to_string()) {
                    self.issues.insert(format!(
                        "portable-core-macro-argument-effect:{name}:{effect}"
                    ));
                }
            } else if name == "vec" {
                match syn::parse_str::<syn::Expr>(&format!("[{}]", item.tokens)) {
                    Ok(expression) => self.visit_expr(&expression),
                    Err(_) => {
                        self.issues
                            .insert("portable-core-macro-arguments-unexamined:vec".to_owned());
                    }
                }
            } else {
                self.issues
                    .insert(format!("portable-core-macro-arguments-unexamined:{name}"));
            }
        }
        syn::visit::visit_macro(self, item);
    }
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if self.layer.is_some() && attribute.path().is_ident("path") {
            self.issues.insert("portable-core-path-override".to_owned());
        }
        if self.layer.is_some()
            && attribute.path().is_ident("cfg_attr")
            && !examined_contract_attribute(attribute)
        {
            self.issues
                .insert("portable-core-conditional-attribute-unexamined".to_owned());
        }
        syn::visit::visit_attribute(self, attribute);
    }
}

pub fn inspect_source(source: &str, relative: &str) -> Result<Vec<String>, String> {
    let tree = syn::parse_file(source)
        .map_err(|error| format!("RUST_ARCHITECTURE_PARSE:{relative}:{error}"))?;
    let module = relative
        .trim_end_matches(".rs")
        .trim_end_matches("/mod")
        .split('/')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let layer = module
        .first()
        .filter(|layer| matches!(layer.as_str(), "domain" | "application"))
        .cloned();
    let mut visitor = BoundaryVisitor {
        module,
        layer,
        issues: BTreeSet::new(),
    };
    visitor.visit_file(&tree);
    if relative == "lib.rs" {
        for item in &tree.items {
            match item {
                syn::Item::Mod(module)
                    if matches!(module.vis, Visibility::Public(_)) && module.ident != "public" =>
                {
                    visitor
                        .issues
                        .insert(format!("owner-root-export:{}", module.ident));
                }
                syn::Item::Use(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-reexport".to_owned());
                }
                syn::Item::Fn(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor
                        .issues
                        .insert("owner-root-function-export".to_owned());
                }
                syn::Item::Struct(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-type-export".to_owned());
                }
                syn::Item::Enum(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-type-export".to_owned());
                }
                syn::Item::Trait(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-type-export".to_owned());
                }
                syn::Item::Type(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-type-export".to_owned());
                }
                syn::Item::Const(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-value-export".to_owned());
                }
                syn::Item::Static(item) if matches!(item.vis, Visibility::Public(_)) => {
                    visitor.issues.insert("owner-root-value-export".to_owned());
                }
                _ => {}
            }
        }
        if !tree.items.iter().any(|item| matches!(item, syn::Item::Mod(module) if module.ident == "public" && matches!(module.vis, Visibility::Public(_)))) { visitor.issues.insert("owner-public-facade-required".to_owned()); }
    }
    Ok(visitor.issues.into_iter().collect())
}

pub fn inspect_crate(directory: &Path) -> Result<Vec<String>, String> {
    let root = directory.join("src");
    let mut pending = vec![root.clone()];
    let mut issues = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let relative = path
                    .strip_prefix(&root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy();
                let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
                issues.extend(
                    inspect_source(&source, &relative)?
                        .into_iter()
                        .map(|issue| format!("{}:{issue}", path.display())),
                );
            }
        }
    }
    Ok(issues)
}
