This crate generates Rust structures from OpenAPI 3.0 definitions.

## Example

### Cargo.toml:

```toml
[dependencies]
serde = "1.0.142"
openapi-struct-gen = "*"

[build-dependencies]
openapi-struct-gen = { version = "*", features = ["build"] }
```

### build.rs:
```rust
use openapi_struct_gen::generate;

fn main() {
    generate(
        format!(
            "{}/{}",
            std::env::var("CARGO_MANIFEST_DIR").unwrap(),
            "api.yaml"
        ),
        format!("{}/{}", std::env::var("OUT_DIR").unwrap(), "oapi.rs"),
        Some(&["Clone", "Serialize", "Deserialize"]),
        Some(&[("serde", "Serialize"), ("serde", "Deserialize")]),
        Some(&[(r#"#[skip_serializing_none]"#, None)]),
        Some(&[(r#"#[serde(rename_all = "camelCase")]"#, Some(&["Struct"]))]),
        None,
        true,
    ).unwrap();
}
```

The first aparameter is path to oapi schema.
The second is the target output rust file.
The third is derive statements.
The fourth is use statements, being tuples of the path to an object and the object
the fifth is annotations that are to be put before the derive statement. Sometimes such are required, like serde\_with. Each annotation consists of a tuple - the annotation itself and optional list of structs that are not to have this annotation
The sixth is annotations that are to be put after the derive statement. Most annotations would be applied like that.
Each annotation consists of a tuple - the annotation itself and optional list of structs that are not to have this annotation
The seventh is optional field annotation mappings, as tuples of `(schema_type, required_annotation, optional_annotation)`.
The eighth controls whether generated `Option<T>` fields emit `#[serde(skip_serializing_if = "Option::is_none")]`.

### code:
```rust
openapi_struct_gen::include!("oapi");
```

## Goals
* Generate Rust structures from Open API 3.0 definitions

## Non Goals
* Generate web servers and clients
