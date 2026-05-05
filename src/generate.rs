use std::collections::{BTreeMap, HashSet};

use check_keyword::CheckKeyword;

use codegen::{Field, Scope};
use heck::{ToPascalCase, ToSnekCase};
use indexmap::IndexMap;
use openapiv3::{
    ArrayType, IntegerFormat, IntegerType, NumberFormat, NumberType, ReferenceOr, Schema,
    SchemaKind, StringType, Type, VariantOrUnknownOrEmpty,
};

fn schema_kind_key(sk: &SchemaKind) -> Option<&'static str> {
    match sk {
        SchemaKind::Type(Type::String(s)) if !s.enumeration.is_empty() => Some("enum"),
        SchemaKind::Type(Type::String(_)) => Some("string"),
        SchemaKind::Type(Type::Number(_)) => Some("number"),
        SchemaKind::Type(Type::Integer(_)) => Some("integer"),
        SchemaKind::Type(Type::Object(_)) => Some("object"),
        SchemaKind::Type(Type::Array(_)) => Some("array"),
        SchemaKind::Type(Type::Boolean {}) => Some("boolean"),
        _ => None,
    }
}

fn property_type_key<'a>(
    refor: &ReferenceOr<Box<Schema>>,
    schemas: &'a BTreeMap<String, Schema>,
) -> Option<&'static str> {
    match refor {
        ReferenceOr::Item(s) => schema_kind_key(&s.schema_kind),
        ReferenceOr::Reference { reference } => {
            let key = reference.split('/').last().unwrap();
            schemas.get(key).and_then(|t| schema_kind_key(&t.schema_kind))
        }
    }
}

fn schema_kind_label(sk: &SchemaKind) -> &'static str {
    match sk {
        SchemaKind::Type(Type::String(s)) if !s.enumeration.is_empty() => "string (enum)",
        SchemaKind::Type(Type::String(_)) => "string",
        SchemaKind::Type(Type::Number(_)) => "number",
        SchemaKind::Type(Type::Integer(_)) => "integer",
        SchemaKind::Type(Type::Object(_)) => "object",
        SchemaKind::Type(Type::Array(_)) => "array",
        SchemaKind::Type(Type::Boolean {}) => "boolean",
        SchemaKind::OneOf { .. } => "oneOf",
        SchemaKind::AnyOf { .. } => "anyOf",
        SchemaKind::AllOf { .. } => "allOf",
        SchemaKind::Not { .. } => "not",
        SchemaKind::Any(_) => "any",
    }
}

fn emit_doc(scope: &mut Scope, description: Option<&str>, type_label: &str) {
    if let Some(desc) = description {
        for line in desc.lines() {
            scope.raw(&format!("/// {}", line));
        }
        scope.raw("///");
    }
    scope.raw(&format!("/// Type: `{}`", type_label));
}

fn field_doc_lines(description: Option<&str>, type_label: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    if let Some(desc) = description {
        for line in desc.lines() {
            lines.push(line.to_string());
        }
        lines.push(String::new());
    }
    lines.push(format!("Type: `{}`", type_label));
    lines
}

fn property_doc_info<'a>(
    refor: &'a ReferenceOr<Box<Schema>>,
    schemas: &'a BTreeMap<String, Schema>,
) -> (Option<&'a str>, &'a str) {
    match refor {
        ReferenceOr::Item(s) => (
            s.schema_data.description.as_deref(),
            schema_kind_label(&s.schema_kind),
        ),
        ReferenceOr::Reference { reference } => {
            let key = reference.split('/').last().unwrap();
            if let Some(target) = schemas.get(key) {
                (
                    target.schema_data.description.as_deref(),
                    schema_kind_label(&target.schema_kind),
                )
            } else {
                (None, "reference")
            }
        }
    }
}

fn to_type_name(name: &str) -> String {
    let pascal = name.to_pascal_case();
    let mut out: String = pascal
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if out
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        out.insert(0, '_');
    }
    out
}

pub fn generate(
    schemas: BTreeMap<String, Schema>,
    derivatives: Option<&[&str]>,
    imports: Option<&[(&str, &str)]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
    field_annotations: Option<&[(&str, &str, &str)]>,
) -> String {
    let mut scope = Scope::new();
    if let Some(imports) = imports {
        for (path, name) in imports {
            scope.import(path, name);
        }
    }
    let names: Vec<String> = schemas.keys().cloned().collect();
    for name in names {
        generate_for_schema(
            &mut scope,
            &name,
            &schemas,
            derivatives,
            annotations_before,
            annotations_after,
            field_annotations,
        );
    }
    scope.to_string()
}

fn generate_for_schema(
    scope: &mut Scope,
    name: &str,
    schemas: &BTreeMap<String, Schema>,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
    field_annotations: Option<&[(&str, &str, &str)]>,
) {
    let schema = &schemas[name];
    let safe = to_type_name(name);
    let description = schema.schema_data.description.as_deref();
    let label = schema_kind_label(&schema.schema_kind);
    match &schema.schema_kind {
        SchemaKind::Type(r#type) => generate_struct(
            scope,
            safe,
            r#type.clone(),
            schemas,
            description,
            label,
            derivatives,
            annotations_before,
            annotations_after,
            field_annotations,
        ),
        SchemaKind::OneOf { one_of } => generate_enum(
            scope,
            safe,
            one_of.clone(),
            description,
            label,
            derivatives,
            annotations_before,
            annotations_after,
        ),
        SchemaKind::AnyOf { any_of } => generate_enum(
            scope,
            safe,
            any_of.clone(),
            description,
            label,
            derivatives,
            annotations_before,
            annotations_after,
        ),
        SchemaKind::AllOf { all_of } => generate_all_of(
            scope,
            safe,
            all_of.clone(),
            schemas,
            description,
            label,
            derivatives,
            annotations_before,
            annotations_after,
            field_annotations,
        ),
        _ => {}
    }
}

fn collect_all_of(
    members: &[ReferenceOr<Schema>],
    schemas: &BTreeMap<String, Schema>,
    visited: &mut HashSet<String>,
    props: &mut IndexMap<String, ReferenceOr<Box<Schema>>>,
    required: &mut Vec<String>,
    flatten: &mut Vec<String>,
) {
    for m in members {
        match m {
            ReferenceOr::Reference { reference } => {
                let key = reference.split('/').last().unwrap();
                if !visited.insert(key.to_string()) {
                    continue;
                }
                let Some(s) = schemas.get(key) else { continue };
                match &s.schema_kind {
                    SchemaKind::Type(Type::Object(o)) => {
                        for (k, v) in o.properties.iter() {
                            props.insert(k.clone(), v.clone());
                        }
                        required.extend(o.required.iter().cloned());
                    }
                    SchemaKind::AllOf { all_of } => {
                        collect_all_of(all_of, schemas, visited, props, required, flatten);
                    }
                    SchemaKind::OneOf { .. } | SchemaKind::AnyOf { .. } => {
                        flatten.push(to_type_name(key));
                    }
                    _ => {}
                }
            }
            ReferenceOr::Item(s) => match &s.schema_kind {
                SchemaKind::Type(Type::Object(o)) => {
                    for (k, v) in o.properties.iter() {
                        props.insert(k.clone(), v.clone());
                    }
                    required.extend(o.required.iter().cloned());
                }
                SchemaKind::AllOf { all_of } => {
                    collect_all_of(all_of, schemas, visited, props, required, flatten);
                }
                _ => {}
            },
        }
    }
}

fn generate_all_of(
    scope: &mut Scope,
    name: String,
    members: Vec<ReferenceOr<Schema>>,
    schemas: &BTreeMap<String, Schema>,
    description: Option<&str>,
    type_label: &str,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
    field_annotations: Option<&[(&str, &str, &str)]>,
) {
    let mut props: IndexMap<String, ReferenceOr<Box<Schema>>> = IndexMap::new();
    let mut required: Vec<String> = Vec::new();
    let mut flatten: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    collect_all_of(
        &members,
        schemas,
        &mut visited,
        &mut props,
        &mut required,
        &mut flatten,
    );

    emit_doc(scope, description, type_label);
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
    let required_set: HashSet<String> = required.into_iter().collect();
    let mut used_field_names: HashSet<String> = HashSet::new();

    for (pname, refor) in props {
        let is_required = required_set.contains(&pname);
        let (field_desc, field_label) = property_doc_info(&refor, schemas);
        let type_key = property_type_key(&refor, schemas);
        let nullable_inline = matches!(&refor, ReferenceOr::Item(s) if s.schema_data.nullable);
        let doc_lines = field_doc_lines(field_desc, field_label);
        let t = get_property_type_from_schema_refor(refor.unbox(), is_required);
        let snake = pname.to_snek_case().into_safe();
        used_field_names.insert(snake.clone());
        let mut field = Field::new(&format!("pub {}", &snake), t.as_str());
        field.doc(doc_lines.iter().map(String::as_str).collect());
        let mut annotations: Vec<String> = Vec::new();
        if let (Some(key), Some(mappings)) = (type_key, field_annotations) {
            let is_optional = !is_required || nullable_inline;
            for (k, req_ann, opt_ann) in mappings {
                if *k == key {
                    let ann = if is_optional { *opt_ann } else { *req_ann };
                    annotations.push(ann.to_string());
                }
            }
        }
        if snake != pname {
            annotations.push(format!("#[serde(rename = \"{}\")]", pname));
        }
        if !annotations.is_empty() {
            field.annotation(annotations.iter().map(String::as_str).collect());
        }
        r#struct.push_field(field);
    }

    for ty in flatten {
        let base = ty.to_snek_case().into_safe();
        let mut field_name = base.clone();
        let mut c = 2;
        while used_field_names.contains(&field_name) {
            field_name = format!("{}_{}", base, c);
            c += 1;
        }
        used_field_names.insert(field_name.clone());
        let mut field = Field::new(&format!("pub {}", &field_name), ty.as_str());
        field.annotation(vec!["#[serde(flatten)]"]);
        r#struct.push_field(field);
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
    match sk {
        SchemaKind::Type(r#type) => gen_type_name_for_type(r#type),
        _ => "serde_json::Value".into(),
    }
}

fn get_property_type_from_schema_refor(refor: ReferenceOr<Schema>, is_required: bool) -> String {
    let (t, nullable) = match refor {
        ReferenceOr::Item(i) => (
            gen_property_type_for_schema_kind(i.schema_kind),
            i.schema_data.nullable,
        ),
        ReferenceOr::Reference { reference } => (handle_reference(reference), false),
    };
    if is_required && !nullable {
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
    to_type_name(split.pop().unwrap())
}

fn generate_struct(
    scope: &mut Scope,
    name: String,
    r#type: Type,
    schemas: &BTreeMap<String, Schema>,
    description: Option<&str>,
    type_label: &str,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
    field_annotations: Option<&[(&str, &str, &str)]>,
) {
    match r#type {
        Type::Object(obj) => {
            emit_doc(scope, description, type_label);
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
                let (field_desc, field_label) = property_doc_info(&refor, schemas);
                let type_key = property_type_key(&refor, schemas);
                let nullable_inline = matches!(&refor, ReferenceOr::Item(s) if s.schema_data.nullable);
                let doc_lines = field_doc_lines(field_desc, field_label);
                let t = get_property_type_from_schema_refor(refor.unbox(), is_required);
                let snake = name.to_snek_case().into_safe();
                let mut field = Field::new(&format!("pub {}", &snake), t.as_str());
                field.doc(doc_lines.iter().map(String::as_str).collect());
                let mut annotations: Vec<String> = Vec::new();
                if let (Some(key), Some(mappings)) = (type_key, field_annotations) {
                    let is_optional = !is_required || nullable_inline;
                    for (k, req_ann, opt_ann) in mappings {
                        if *k == key {
                            let ann = if is_optional { *opt_ann } else { *req_ann };
                            annotations.push(ann.to_string());
                        }
                    }
                }
                if snake != name {
                    annotations.push(format!("#[serde(rename = \"{}\")]", name));
                }
                if !annotations.is_empty() {
                    field.annotation(annotations.iter().map(String::as_str).collect());
                }
                r#struct.push_field(field);
            }
        }
        Type::Array(a) => {
            emit_doc(scope, description, type_label);
            scope.raw(&format!("pub type {} = {};", name, gen_array_type(a)));
        }
        Type::String(s) if !s.enumeration.is_empty() => {
            generate_string_enum(
                scope,
                name,
                s,
                description,
                type_label,
                derivatives,
                annotations_before,
                annotations_after,
            );
        }
        t => {
            emit_doc(scope, description, type_label);
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
    description: Option<&str>,
    type_label: &str,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) {
    emit_doc(scope, description, type_label);
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
    description: Option<&str>,
    type_label: &str,
    derivatives: Option<&[&str]>,
    annotations_before: Option<&[(&str, Option<&[&str]>)]>,
    annotations_after: Option<&[(&str, Option<&[&str]>)]>,
) {
    emit_doc(scope, description, type_label);
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
        derivs.extend(derivatives.iter().filter(|d| **d != "Default"));
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

    let mut body = String::new();
    body.push_str(&format!("pub enum {} {{\n", name));
    for (i, t) in types.into_iter().enumerate() {
        match t {
            ReferenceOr::Reference { reference } => {
                let target = to_type_name(reference.split('/').last().unwrap());
                body.push_str(&format!("    {}({}),\n", target, target));
            }
            ReferenceOr::Item(s) => {
                let vname = inline_variant_name(&s, &name, i);
                match s.schema_kind {
                    SchemaKind::Type(Type::Object(obj)) if !obj.properties.is_empty() => {
                        let required: HashSet<String> = obj.required.into_iter().collect();
                        if obj.properties.len() == 1 {
                            // Single-property inline object: emit a tuple variant so the
                            // wire format `{"<PropName>": <value>}` matches serde's default
                            // external tagging without double-nesting.
                            let (pname, refor) = obj.properties.into_iter().next().unwrap();
                            let is_required = required.contains(&pname);
                            let ty = get_property_type_from_schema_refor(refor.unbox(), is_required);
                            let variant = pname.to_pascal_case();
                            if variant != pname {
                                body.push_str(&format!(
                                    "    #[serde(rename = \"{}\")]\n",
                                    pname
                                ));
                            }
                            body.push_str(&format!("    {}({}),\n", variant, ty));
                        } else {
                            body.push_str(&format!("    {} {{\n", vname));
                            for (pname, refor) in obj.properties {
                                let is_required = required.contains(&pname);
                                let ty = get_property_type_from_schema_refor(refor.unbox(), is_required);
                                let snake = pname.to_snek_case().into_safe();
                                if snake != pname {
                                    body.push_str(&format!(
                                        "        #[serde(rename = \"{}\")]\n",
                                        pname
                                    ));
                                }
                                body.push_str(&format!("        {}: {},\n", snake, ty));
                            }
                            body.push_str("    },\n");
                        }
                    }
                    other => {
                        let ty = gen_property_type_for_schema_kind(other);
                        body.push_str(&format!("    {}({}),\n", vname, ty));
                    }
                }
            }
        }
    }
    body.push_str("}");
    scope.raw(&body);
}

fn inline_variant_name(s: &Schema, parent: &str, i: usize) -> String {
    if let Some(title) = s.schema_data.title.as_deref() {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_pascal_case();
        }
    }
    format!("{}Variant{}", parent, i)
}
