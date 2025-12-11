use syn::{Attribute, Expr, Lit, Result};

/// Plugin-level attributes (#[plugin(...)])
#[derive(Default)]
pub struct PluginAttrs {
    pub name: Option<String>,
    pub prefix: Option<String>,
    pub rename_rule: Option<RenameRule>,
    pub description: Option<String>,
    pub version: Option<String>,
}

/// Variant-level attributes
#[derive(Default)]
pub struct VariantAttrs {
    pub description: Option<String>,
    pub alias: Option<String>,
    pub aliases: Vec<String>,
    pub rename: Option<String>,
    pub hide: bool,
}

#[derive(Clone, Copy)]
pub enum RenameRule {
    Lowercase,
    Uppercase,
    SnakeCase,
    CamelCase,
    PascalCase,
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

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "lowercase" => Some(RenameRule::Lowercase),
            "UPPERCASE" | "uppercase" => Some(RenameRule::Uppercase),
            "snake_case" => Some(RenameRule::SnakeCase),
            "camelCase" => Some(RenameRule::CamelCase),
            "PascalCase" => Some(RenameRule::PascalCase),
            "kebab-case" => Some(RenameRule::KebabCase),
            _ => None,
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

impl PluginAttrs {
    pub fn from_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("plugin") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.name = Some(s.value());
                    }
                } else if meta.path.is_ident("prefix") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.prefix = Some(s.value());
                    }
                } else if meta.path.is_ident("rename_rule") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.rename_rule = RenameRule::from_str(&s.value());
                    }
                } else if meta.path.is_ident("description") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.description = Some(s.value());
                    }
                } else if meta.path.is_ident("version") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.version = Some(s.value());
                    }
                }
                Ok(())
            })?;
        }

        Ok(result)
    }
}

impl VariantAttrs {
    pub fn from_attributes(attrs: &[Attribute]) -> Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("command") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("description") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.description = Some(s.value());
                    }
                } else if meta.path.is_ident("alias") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.alias = Some(s.value());
                    }
                } else if meta.path.is_ident("aliases") {
                    let value: Expr = meta.value()?.parse()?;
                    if let Expr::Array(arr) = value {
                        for elem in arr.elems {
                            if let Expr::Lit(lit) = elem
                                && let Lit::Str(s) = lit.lit
                            {
                                result.aliases.push(s.value());
                            }
                        }
                    }
                } else if meta.path.is_ident("rename") {
                    let value: Lit = meta.value()?.parse()?;
                    if let Lit::Str(s) = value {
                        result.rename = Some(s.value());
                    }
                } else if meta.path.is_ident("hide") {
                    result.hide = true;
                }
                Ok(())
            })?;
        }

        Ok(result)
    }
}
