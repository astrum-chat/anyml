use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{
    Expr, Ident, LitBool, LitFloat, LitInt, LitStr, Pat, Token, braced, token,
};

/// A single field entry in the JSON object.
enum Field {
    /// `"key": value`
    KeyValue(String, Value),
    /// `if let Some(x) = expr { ...fields... }`
    IfLet {
        pat: Pat,
        expr: Expr,
        fields: Vec<Field>,
    },
    /// `if expr { ...fields... }`
    If {
        expr: Expr,
        fields: Vec<Field>,
    },
}

/// A JSON value.
enum Value {
    /// A string literal: `"hello"`
    LitStr(String),
    /// A bool literal: `true` / `false`
    LitBool(bool),
    /// An integer literal: `123`
    LitInt(String),
    /// A float literal: `1.5`
    LitFloat(String),
    /// A variable/expression reference: `model`, `options.model`
    Variable(Expr),
    /// A nested object: `{ "key": value, ... }`
    Object(Vec<Field>),
    /// A raw (pre-serialized) value: `@raw expr`
    Raw(Expr),
}

/// The top-level macro input: a list of fields inside `{ }`.
struct JsonInput {
    fields: Vec<Field>,
}

impl Parse for JsonInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let fields = parse_fields(input)?;
        Ok(JsonInput { fields })
    }
}

fn parse_fields(input: ParseStream) -> syn::Result<Vec<Field>> {
    let mut fields = Vec::new();

    while !input.is_empty() {
        if input.peek(Token![if]) {
            fields.push(parse_conditional(input)?);
        } else {
            fields.push(parse_key_value(input)?);
        }

        // Consume optional trailing comma
        let _ = input.parse::<Token![,]>();
    }

    Ok(fields)
}

fn parse_key_value(input: ParseStream) -> syn::Result<Field> {
    let key: LitStr = input.parse()?;
    input.parse::<Token![:]>()?;
    let value = parse_value(input)?;
    Ok(Field::KeyValue(key.value(), value))
}

fn parse_value(input: ParseStream) -> syn::Result<Value> {
    // @raw expr
    if input.peek(Token![@]) {
        input.parse::<Token![@]>()?;
        let ident: Ident = input.parse()?;
        if ident != "raw" {
            return Err(syn::Error::new(ident.span(), "expected `raw` after `@`"));
        }
        let expr: Expr = input.parse()?;
        return Ok(Value::Raw(expr));
    }

    // String literal
    if input.peek(LitStr) {
        let lit: LitStr = input.parse()?;
        return Ok(Value::LitStr(lit.value()));
    }

    // Bool literal
    if input.peek(LitBool) {
        let lit: LitBool = input.parse()?;
        return Ok(Value::LitBool(lit.value()));
    }

    // Integer literal
    if input.peek(LitInt) {
        let lit: LitInt = input.parse()?;
        return Ok(Value::LitInt(lit.to_string()));
    }

    // Float literal
    if input.peek(LitFloat) {
        let lit: LitFloat = input.parse()?;
        return Ok(Value::LitFloat(lit.to_string()));
    }

    // Nested object
    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        let fields = parse_fields(&content)?;
        return Ok(Value::Object(fields));
    }

    // Otherwise treat as a variable/expression
    let expr: Expr = input.parse()?;
    Ok(Value::Variable(expr))
}

fn parse_conditional(input: ParseStream) -> syn::Result<Field> {
    input.parse::<Token![if]>()?;

    if input.peek(Token![let]) {
        // if let Pat = Expr { fields }
        input.parse::<Token![let]>()?;
        let pat: Pat = Pat::parse_single(input)?;
        input.parse::<Token![=]>()?;
        let expr = Expr::parse_without_eager_brace(input)?;

        let content;
        braced!(content in input);
        let fields = parse_fields(&content)?;

        Ok(Field::IfLet { pat, expr, fields })
    } else {
        // if Expr { fields }
        let expr = Expr::parse_without_eager_brace(input)?;

        let content;
        braced!(content in input);
        let fields = parse_fields(&content)?;

        Ok(Field::If { expr, fields })
    }
}

/// Check if all fields (recursively) are fully static (no variables, no conditionals, no raw).
fn is_all_static(fields: &[Field]) -> bool {
    fields.iter().all(|f| match f {
        Field::KeyValue(_, value) => is_value_static(value),
        Field::IfLet { .. } | Field::If { .. } => false,
    })
}

fn is_value_static(value: &Value) -> bool {
    match value {
        Value::LitStr(_) | Value::LitBool(_) | Value::LitInt(_) | Value::LitFloat(_) => true,
        Value::Object(fields) => is_all_static(fields),
        Value::Variable(_) | Value::Raw(_) => false,
    }
}

/// Build a fully-static string via concat! for objects where all values are literals.
fn static_object_str(fields: &[Field]) -> String {
    let mut parts = Vec::new();

    for field in fields {
        match field {
            Field::KeyValue(key, value) => {
                let val_str = static_value_str(value);
                parts.push(format!("\"{}\":{}", json_escape(key), val_str));
            }
            _ => unreachable!("is_all_static should have returned false"),
        }
    }

    format!("{{{}}}", parts.join(","))
}

fn static_value_str(value: &Value) -> String {
    match value {
        Value::LitStr(s) => format!("\"{}\"", json_escape(s)),
        Value::LitBool(b) => b.to_string(),
        Value::LitInt(n) => n.clone(),
        Value::LitFloat(n) => n.clone(),
        Value::Object(fields) => static_object_str(fields),
        _ => unreachable!("is_value_static should have returned false"),
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\x20' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Generate code that writes fields into a `String` variable named `__json_buf`.
fn gen_dynamic_fields(fields: &[Field], is_first_field: &mut bool) -> TokenStream2 {
    let mut stmts = Vec::new();

    for field in fields {
        match field {
            Field::KeyValue(key, value) => {
                let escaped_key = json_escape(key);
                let comma = if *is_first_field {
                    *is_first_field = false;
                    quote! {}
                } else {
                    quote! { __json_buf.push(','); }
                };
                let write_value = gen_value_write(value);
                stmts.push(quote! {
                    #comma
                    __json_buf.push_str(concat!("\"", #escaped_key, "\":"));
                    #write_value
                });
            }
            Field::IfLet { pat, expr, fields: inner } => {
                // For conditionals, we must handle comma dynamically since we don't
                // know at compile time whether preceding fields have been written.
                let needs_comma = !*is_first_field;
                *is_first_field = false;
                let inner_code = gen_conditional_fields(inner);
                if needs_comma {
                    stmts.push(quote! {
                        if let #pat = #expr {
                            __json_buf.push(',');
                            #inner_code
                        }
                    });
                } else {
                    stmts.push(quote! {
                        if let #pat = #expr {
                            #inner_code
                        }
                    });
                }
            }
            Field::If { expr, fields: inner } => {
                let needs_comma = !*is_first_field;
                *is_first_field = false;
                let inner_code = gen_conditional_fields(inner);
                if needs_comma {
                    stmts.push(quote! {
                        if #expr {
                            __json_buf.push(',');
                            #inner_code
                        }
                    });
                } else {
                    stmts.push(quote! {
                        if #expr {
                            #inner_code
                        }
                    });
                }
            }
        }
    }

    quote! { #(#stmts)* }
}

/// Generate writes for fields inside a conditional (always comma-separated from each other,
/// but the first one doesn't need a leading comma since the caller handles it).
fn gen_conditional_fields(fields: &[Field]) -> TokenStream2 {
    let mut first = true;
    gen_dynamic_fields(fields, &mut first)
}

/// Generate code that writes a value into `__json_buf`.
fn gen_value_write(value: &Value) -> TokenStream2 {
    match value {
        Value::LitStr(s) => {
            let escaped = json_escape(s);
            quote! { __json_buf.push_str(concat!("\"", #escaped, "\"")); }
        }
        Value::LitBool(b) => {
            let s = if *b { "true" } else { "false" };
            quote! { __json_buf.push_str(#s); }
        }
        Value::LitInt(n) => {
            let s = n.as_str();
            quote! { __json_buf.push_str(#s); }
        }
        Value::LitFloat(n) => {
            let s = n.as_str();
            quote! { __json_buf.push_str(#s); }
        }
        Value::Variable(expr) => {
            // At runtime, determine the type via the JsonValue trait.
            quote! { ::anyml_core::json::JsonValue::write_json(&(#expr), &mut __json_buf); }
        }
        Value::Object(fields) => {
            if is_all_static(fields) {
                let s = static_object_str(fields);
                quote! { __json_buf.push_str(#s); }
            } else {
                let mut first = true;
                let inner = gen_dynamic_fields(fields, &mut first);
                quote! {
                    __json_buf.push('{');
                    #inner
                    __json_buf.push('}');
                }
            }
        }
        Value::Raw(expr) => {
            quote! { __json_buf.push_str(&(#expr)); }
        }
    }
}

#[proc_macro]
pub fn json_string(input: TokenStream) -> TokenStream {
    let parsed = syn::parse_macro_input!(input as JsonInput);

    if is_all_static(&parsed.fields) {
        // Fully static — produce a &'static str
        let s = static_object_str(&parsed.fields);
        return (quote! { #s }).into();
    }

    // Dynamic — produce code that builds a String
    let mut first = true;
    let body = gen_dynamic_fields(&parsed.fields, &mut first);

    let expanded = quote! {
        {
            let mut __json_buf = String::new();
            __json_buf.push('{');
            #body
            __json_buf.push('}');
            __json_buf
        }
    };

    expanded.into()
}
