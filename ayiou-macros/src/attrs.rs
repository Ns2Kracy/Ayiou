use darling::{FromDeriveInput, FromMeta, FromVariant};

#[derive(Debug, Clone, Copy, Default, FromMeta)]
pub enum RenameRule {
    #[default]
    #[darling(rename = "lowercase")]
    Lowercase,
    #[darling(rename = "UPPERCASE")]
    Uppercase,
    #[darling(rename = "snake_case")]
    SnakeCase,
    #[darling(rename = "camelCase")]
    CamelCase,
    #[darling(rename = "PascalCase")]
    PascalCase,
    #[darling(rename = "kebab-case")]
    KebabCase,
}

impl RenameRule {
    pub fn apply(&self, name: &str) -> String {
        match self {
            RenameRule::Lowercase => name.to_lowercase(),
            RenameRule::Uppercase => name.to_uppercase(),
            RenameRule::SnakeCase => to_snake_case(name),
            RenameRule::CamelCase => to_camel_case(name),
            RenameRule::PascalCase => name.to_string(),
            RenameRule::KebabCase => to_kebab_case(name),
        }
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap());
    }
    result
}

fn to_camel_case(s: &str) -> String {
    let snake = to_snake_case(s);
    let mut result = String::new();
    let mut capitalize_next = false;
    for c in snake.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_uppercase().next().unwrap());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

fn to_kebab_case(s: &str) -> String {
    to_snake_case(s).replace('_', "-")
}

/// Plugin-level attributes from #[plugin(...)]
#[derive(Debug, FromDeriveInput)]
#[darling(attributes(plugin), supports(enum_any))]
pub struct PluginAttrs {
    pub ident: syn::Ident,
    pub data: darling::ast::Data<VariantAttrs, ()>,

    #[darling(default)]
    pub name: Option<String>,
    #[darling(default)]
    pub prefix: Option<String>,
    #[darling(default)]
    pub rename_rule: Option<RenameRule>,
    #[darling(default)]
    pub description: Option<String>,
    #[darling(default)]
    pub version: Option<String>,
    /// Plugin dependencies. Use "name" for required, "name?" for optional.
    #[darling(default, multiple)]
    pub dependencies: Vec<String>,
}

/// Variant-level attributes from #[plugin(...)]
#[derive(Debug, FromVariant)]
#[darling(attributes(plugin))]
pub struct VariantAttrs {
    pub ident: syn::Ident,
    pub fields: darling::ast::Fields<syn::Field>,

    #[darling(default)]
    pub description: Option<String>,
    #[darling(default)]
    pub alias: Option<String>,
    #[darling(default, multiple)]
    pub aliases: Vec<String>,
    #[darling(default)]
    pub rename: Option<String>,
    #[darling(default)]
    pub hide: bool,
}
