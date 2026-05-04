use openapi_struct_gen::generate;

#[test]
fn test_generate() {
    generate(
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/example-schema.yaml"),
        concat!(env!("CARGO_TARGET_TMPDIR"), "/gen.rs"),
        Some(&["Clone", "Serialize", "Deserialize"]),
        Some(&[("serde", "Serialize"), ("serde", "Deserialize")]),
        Some(&[(r#"#[skip_serializing_none]"#, None)]),
        Some(&[(
            r#"#[serde(rename_all = "camelCase")]"#,
            Some(&["SearchRequest"]),
        )]),
        Some(&[(
            "integer",
            r#"#[serde_as(as = "DisplayFromStr")]"#,
            r#"#[serde_as(as = "Option<DisplayFromStr>")]"#,
        )]),
    )
    .unwrap();
}
