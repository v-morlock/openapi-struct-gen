use std::collections::{BTreeMap, HashSet};

use check_keyword::CheckKeyword;

use codegen::{Field, Scope};
use heck::{ToPascalCase, ToSnekCase};
use openapiv3::{
    ArrayType, IntegerFormat, IntegerType, NumberFormat, NumberType, ReferenceOr, Schema,
    SchemaKind, StringType, Type, VariantOrUnknownOrEmpty,
};

pub fn generate(
    schemas: BTreeMap<String, Schema>,
    derivatives: Option<&[&str]>,
    imports: Option<&[(&str, &str)]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) -> String {
    let mut scope = Scope::new();
    if let Some(imports) = imports {
        for (path, name) in imports {
            scope.import(path, name);
        }
    }
    for (name, schema) in schemas.into_iter() {
        generate_for_schema(
            &mut scope,
            name,
            schema,
            derivatives,
            annotations_before,
            annotations_after,
        );
    }
    scope.to_string()
}

fn generate_for_schema(
    scope: &mut Scope,
    name: String,
    schema: Schema,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) {
    match schema.schema_kind {
        SchemaKind::Type(r#type) => generate_struct(
            scope,
            name,
            r#type,
            derivatives,
            annotations_before,
            annotations_after,
        ),
        SchemaKind::OneOf { one_of } => generate_enum(
            scope,
            name,
            one_of,
            derivatives,
            annotations_before,
            annotations_after,
        ),
        SchemaKind::AnyOf { any_of } => generate_enum(
            scope,
            name,
            any_of,
            derivatives,
            annotations_before,
            annotations_after,
        ),
        _ => panic!("Does not support 'allOf', 'not' and 'any'"),
    }
}

fn get_number_type(t: NumberType) -> String {
    if let VariantOrUnknownOrEmpty::Item(f) = t.format {
        if f == NumberFormat::Double {
            "f64".into()
        } else {
            "f32".into()
        }
    } else {
        "f32".into()
    }
}

fn get_integer_type(t: IntegerType) -> String {
    if let VariantOrUnknownOrEmpty::Item(f) = t.format {
        if f == IntegerFormat::Int64 {
            "i64".into()
        } else {
            "i32".into()
        }
    } else {
        "i32".into()
    }
}

fn gen_type_name_for_type(t: Type) -> String {
    match t {
        Type::String(_) => "String".into(),
        Type::Number(f) => get_number_type(f),
        Type::Integer(f) => get_integer_type(f),
        Type::Object(o) => {
            if let Some(openapiv3::AdditionalProperties::Schema(reference)) =
                o.additional_properties
            {
                if let ReferenceOr::Reference { reference } = *reference {
                    format!(
                        "std::collections::BTreeMap<String, {}>",
                        reference.split('/').last().unwrap()
                    )
                } else {
                    "std::collections::BTreeMap<String, serde_json::Value>".into()
                }
            } else {
                "std::collections::BTreeMap<String, serde_json::Value>".into()
            }
        }
        Type::Array(a) => gen_array_type(a),
        Type::Boolean {} => "bool".into(),
    }
}

fn gen_property_type_for_schema_kind(sk: SchemaKind) -> String {
    let t = match sk {
        SchemaKind::Type(r#type) => r#type,
        _ => panic!("Does not support 'oneOf', 'anyOf' 'allOf', 'not' and 'any'"),
    };
    gen_type_name_for_type(t)
}

fn get_property_type_from_schema_refor(refor: ReferenceOr<Schema>, is_required: bool) -> String {
    let t = match refor {
        ReferenceOr::Item(i) => gen_property_type_for_schema_kind(i.schema_kind),
        ReferenceOr::Reference { reference } => handle_reference(reference),
    };
    if is_required {
        t
    } else {
        format!("Option<{}>", t)
    }
}

fn gen_array_type(a: ArrayType) -> String {
    let inner_type = if let Some(items) = a.items {
        get_property_type_from_schema_refor(items.unbox(), true)
    } else {
        todo!();
    };
    format!("Vec<{}>", inner_type)
}

fn handle_reference(reference: String) -> String {
    let mut split = reference.split("/").into_iter().collect::<Vec<_>>();
    if split[0] != "#" {
        unreachable!();
    }
    if split[1] != "components" {
        panic!("Trying to load from something other than components");
    }
    if split[2] != "schemas" {
        panic!("Only references to schemas are supported");
    }
    split.pop().unwrap().to_owned()
}

fn generate_struct(
    scope: &mut Scope,
    name: String,
    r#type: Type,

    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) {
    match r#type {
        Type::Object(obj) => {
            if let Some(annotations) = annotations_before {
                for (annotation, exceptions) in annotations {
                    let is_exception = if let Some(exceptions) = exceptions {
                        exceptions.iter().any(|e| **e == *name.as_str())
                    } else {
                        false
                    };
                    if !is_exception {
                        scope.raw(annotation);
                    }
                }
            }
            let mut derivs = vec!["Debug"];
            if let Some(derivatives) = derivatives {
                derivs.extend(derivatives);
            }
            scope.raw(&format!("#[derive({})]", derivs.join(", ")));

            if let Some(annotations) = annotations_after {
                for (annotation, exceptions) in annotations {
                    let is_exception = if let Some(exceptions) = exceptions {
                        exceptions.iter().any(|e| **e == *name.as_str())
                    } else {
                        false
                    };
                    if !is_exception {
                        scope.raw(annotation);
                    }
                }
            }

            let r#struct = scope.new_struct(&name).vis("pub");
            let required = obj.required.into_iter().collect::<HashSet<String>>();
            for (name, refor) in obj.properties {
                let is_required = required.contains(&name);
                let t = get_property_type_from_schema_refor(refor.unbox(), is_required);
                let snake = name.to_snek_case().into_safe();
                let mut field = Field::new(&format!("pub {}", &snake), t.as_str());
                if snake != name {
                    field.annotation(vec![&format!("#[serde(rename = \"{}\")]", name)]);
                }
                r#struct.push_field(field);
            }
        }
        Type::Array(a) => {
            scope.raw(&format!("pub type {} = {};", name, gen_array_type(a)));
        }
        Type::String(s) if !s.enumeration.is_empty() => {
            generate_string_enum(
                scope,
                name,
                s,
                derivatives,
                annotations_before,
                annotations_after,
            );
        }
        t => {
            scope.raw(&format!(
                "pub type {} = {};",
                name,
                gen_type_name_for_type(t)
            ));
        }
    }
}

fn generate_string_enum(
    scope: &mut Scope,
    name: String,
    s: StringType,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) {
    if let Some(annotations) = annotations_before {
        for (annotation, exceptions) in annotations {
            let is_exception = exceptions
                .map(|e| e.iter().any(|e| *e == name.as_str()))
                .unwrap_or(false);
            if !is_exception {
                scope.raw(annotation);
            }
        }
    }

    let mut derivs = vec!["Debug"];
    if let Some(derivatives) = derivatives {
        derivs.extend(derivatives);
    }
    scope.raw(&format!("#[derive({})]", derivs.join(", ")));

    if let Some(annotations) = annotations_after {
        for (annotation, exceptions) in annotations {
            let is_exception = exceptions
                .map(|e| e.iter().any(|e| *e == name.as_str()))
                .unwrap_or(false);
            if !is_exception {
                scope.raw(annotation);
            }
        }
    }

    let has_default = derivs.iter().any(|d| *d == "Default");
    let r#enum = scope.new_enum(&name).vis("pub");
    for (i, variant) in s.enumeration.into_iter().flatten().enumerate() {
        let pascal = variant.to_pascal_case();
        let default_attr = if has_default && i == 0 {
            "#[default]\n    "
        } else {
            ""
        };
        if pascal == variant {
            if default_attr.is_empty() {
                r#enum.new_variant(&pascal);
            } else {
                r#enum.new_variant(&format!("{}{}", default_attr, pascal));
            }
        } else {
            r#enum.new_variant(&format!(
                "{}#[serde(rename = \"{}\")]\n    {}",
                default_attr, variant, pascal
            ));
        }
    }
}

fn generate_enum(
    scope: &mut Scope,
    name: String,
    types: Vec<ReferenceOr<Schema>>,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) {
    if let Some(annotations) = annotations_before {
        for (annotation, exceptions) in annotations {
            let is_exception = if let Some(exceptions) = exceptions {
                exceptions.iter().any(|e| **e == *name.as_str())
            } else {
                false
            };
            if !is_exception {
                scope.raw(annotation);
            }
        }
    }

    let mut derivs = vec!["Debug"];
    if let Some(derivatives) = derivatives {
        derivs.extend(derivatives);
    }
    scope.raw(&format!("#[derive({})]", derivs.join(", ")));

    if let Some(annotations) = annotations_after {
        for (annotation, exceptions) in annotations {
            let is_exception = if let Some(exceptions) = exceptions {
                exceptions.iter().any(|e| **e == *name.as_str())
            } else {
                false
            };
            if !is_exception {
                scope.raw(annotation);
            }
        }
    }
    let r#enum = scope.new_enum(&name).vis("pub");

    for t in types.into_iter() {
        let t = get_property_type_from_schema_refor(t, true);
        r#enum.new_variant(&t).tuple(&t);
    }
}
