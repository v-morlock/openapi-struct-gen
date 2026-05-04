use heck::ToUpperCamelCase;
use openapiv3::{ReferenceOr, Schema, SchemaKind, Type};
use std::collections::BTreeMap;

pub fn normalize(schemas: &mut BTreeMap<String, Schema>) {
    let mut queue: Vec<String> = schemas.keys().cloned().collect();
    while let Some(name) = queue.pop() {
        let Some(mut schema) = schemas.remove(&name) else {
            continue;
        };
        walk(&name, &mut schema.schema_kind, schemas, &mut queue);
        schemas.insert(name, schema);
    }
}

fn walk(
    parent: &str,
    kind: &mut SchemaKind,
    schemas: &mut BTreeMap<String, Schema>,
    queue: &mut Vec<String>,
) {
    match kind {
        SchemaKind::Type(t) => walk_type(parent, t, schemas, queue),
        SchemaKind::AllOf { all_of } => {
            for (i, m) in all_of.iter_mut().enumerate() {
                lift_or_recurse(&format!("{}AllOf{}", parent, i), m, schemas, queue);
            }
        }
        SchemaKind::AnyOf { any_of } => {
            for (i, m) in any_of.iter_mut().enumerate() {
                let suggested = variant_name(parent, i, m);
                if let ReferenceOr::Item(s) = m {
                    walk(&suggested, &mut s.schema_kind, schemas, queue);
                }
            }
        }
        SchemaKind::OneOf { one_of } => {
            for (i, m) in one_of.iter_mut().enumerate() {
                let suggested = variant_name(parent, i, m);
                if let ReferenceOr::Item(s) = m {
                    walk(&suggested, &mut s.schema_kind, schemas, queue);
                }
            }
        }
        _ => {}
    }
}

fn walk_type(
    parent: &str,
    t: &mut Type,
    schemas: &mut BTreeMap<String, Schema>,
    queue: &mut Vec<String>,
) {
    match t {
        Type::Object(o) => {
            for (prop_name, slot) in o.properties.iter_mut() {
                let suggested = format!("{}{}", parent, prop_name.to_upper_camel_case());
                lift_or_recurse_boxed(&suggested, slot, schemas, queue);
            }
        }
        Type::Array(a) => {
            if let Some(items) = a.items.as_mut() {
                let suggested = format!("{}Item", parent);
                lift_or_recurse_boxed(&suggested, items, schemas, queue);
            }
        }
        _ => {}
    }
}

fn variant_name(parent: &str, i: usize, slot: &ReferenceOr<Schema>) -> String {
    if let ReferenceOr::Item(s) = slot {
        if let Some(title) = s.schema_data.title.as_deref() {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                return trimmed.to_upper_camel_case();
            }
        }
    }
    format!("{}Variant{}", parent, i)
}

fn is_complex(kind: &SchemaKind) -> bool {
    match kind {
        SchemaKind::Type(Type::Object(o)) => !o.properties.is_empty(),
        SchemaKind::Type(Type::String(s)) => !s.enumeration.is_empty(),
        SchemaKind::Type(_) => false,
        SchemaKind::AllOf { .. } | SchemaKind::AnyOf { .. } | SchemaKind::OneOf { .. } => true,
        _ => false,
    }
}

fn unique_name(suggested: &str, schemas: &BTreeMap<String, Schema>) -> String {
    if !schemas.contains_key(suggested) {
        return suggested.to_string();
    }
    for i in 2..u32::MAX {
        let candidate = format!("{}{}", suggested, i);
        if !schemas.contains_key(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

fn preferred_name(suggested: &str, item: &Schema) -> String {
    if let Some(title) = item.schema_data.title.as_deref() {
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            return trimmed.to_upper_camel_case();
        }
    }
    suggested.to_string()
}

fn lift_or_recurse(
    suggested: &str,
    slot: &mut ReferenceOr<Schema>,
    schemas: &mut BTreeMap<String, Schema>,
    queue: &mut Vec<String>,
) {
    let item = match slot {
        ReferenceOr::Reference { .. } => return,
        ReferenceOr::Item(s) => s,
    };
    if is_complex(&item.schema_kind) {
        let suggested = preferred_name(suggested, item);
        let name = unique_name(&suggested, schemas);
        let placeholder = ReferenceOr::Reference {
            reference: format!("#/components/schemas/{}", name),
        };
        let taken = std::mem::replace(slot, placeholder);
        if let ReferenceOr::Item(s) = taken {
            schemas.insert(name.clone(), s);
            queue.push(name);
        }
    } else {
        walk(suggested, &mut item.schema_kind, schemas, queue);
    }
}

fn lift_or_recurse_boxed(
    suggested: &str,
    slot: &mut ReferenceOr<Box<Schema>>,
    schemas: &mut BTreeMap<String, Schema>,
    queue: &mut Vec<String>,
) {
    let item = match slot {
        ReferenceOr::Reference { .. } => return,
        ReferenceOr::Item(s) => s,
    };
    if is_complex(&item.schema_kind) {
        let suggested = preferred_name(suggested, item);
        let name = unique_name(&suggested, schemas);
        let placeholder: ReferenceOr<Box<Schema>> = ReferenceOr::Reference {
            reference: format!("#/components/schemas/{}", name),
        };
        let taken = std::mem::replace(slot, placeholder);
        if let ReferenceOr::Item(s) = taken {
            schemas.insert(name.clone(), *s);
            queue.push(name);
        }
    } else {
        walk(suggested, &mut item.schema_kind, schemas, queue);
    }
}
