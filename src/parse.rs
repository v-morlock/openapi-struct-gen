use heck::ToUpperCamelCase;
use indexmap::IndexMap;
use openapiv3::{
    MediaType, OpenAPI, PathItem, ReferenceOr, RequestBody, Response, Schema, StatusCode,
};
use std::collections::BTreeMap;

/// Component schemas that are a bare `$ref` (type aliases), name -> target name.
pub type Aliases = BTreeMap<String, String>;

pub fn parse_schema(oapi: OpenAPI) -> (BTreeMap<String, Schema>, Aliases) {
    let mut schemas: BTreeMap<String, Schema> = BTreeMap::new();
    let mut aliases = Aliases::new();
    if let Some(components) = oapi.components {
        gather_from_schemas(&mut schemas, &mut aliases, components.schemas);
        gather_from_responses(&mut schemas, components.responses);
        gather_from_bodies(&mut schemas, components.request_bodies);
    }
    gather_from_paths(&mut schemas, oapi.paths.paths);
    (schemas, aliases)
}

fn gather_from_schemas(
    schemas: &mut BTreeMap<String, Schema>,
    aliases: &mut Aliases,
    map: IndexMap<String, ReferenceOr<Schema>>,
) {
    for (name, refor) in map.into_iter() {
        match refor {
            ReferenceOr::Item(i) => {
                schemas.insert(name, i);
            }
            ReferenceOr::Reference { reference } => {
                let target = reference.split('/').last().unwrap().to_owned();
                aliases.insert(name, target);
            }
        }
    }
}

fn from_single_media_type(schemas: &mut BTreeMap<String, Schema>, name: String, mt: MediaType) {
    if let Some(refor) = mt.schema {
        if let ReferenceOr::Item(i) = refor {
            schemas.insert(name, i);
        }
    }
}

fn from_single_response<F>(schemas: &mut BTreeMap<String, Schema>, name_modifier: F, resp: Response)
where
    F: Fn(String) -> String,
{
    for (mtn, mt) in resp.content.into_iter() {
        from_single_media_type(schemas, name_modifier(mtn), mt);
    }
}

fn from_single_body<F>(schemas: &mut BTreeMap<String, Schema>, name_modifier: F, body: RequestBody)
where
    F: Fn(String) -> String,
{
    for (mtn, mt) in body.content.into_iter() {
        from_single_media_type(schemas, name_modifier(mtn), mt);
    }
}

fn gather_from_responses(
    schemas: &mut BTreeMap<String, Schema>,
    map: IndexMap<String, ReferenceOr<Response>>,
) {
    for (name, refor) in map.into_iter() {
        if let ReferenceOr::Item(i) = refor {
            from_single_response(
                schemas,
                |mtn| generate_response_name(name.clone(), None, None, false, mtn),
                i,
            );
        }
    }
}

fn gather_from_bodies(
    schemas: &mut BTreeMap<String, Schema>,
    map: IndexMap<String, ReferenceOr<RequestBody>>,
) {
    for (name, refor) in map.into_iter() {
        if let ReferenceOr::Item(i) = refor {
            from_single_body(
                schemas,
                |mtn| generate_request_body_name(name.clone(), None, mtn),
                i,
            );
        }
    }
}

fn generate_response_name(
    name: String,
    method: Option<&str>,
    status_code: Option<StatusCode>,
    is_default: bool,
    media_type: String,
) -> String {
    format!(
        "{}{}{}{}Response{}",
        name.to_upper_camel_case(),
        method.unwrap_or_default().to_upper_camel_case(),
        status_code.map(|sc| sc.to_string()).unwrap_or_default(),
        media_type
            .split("/")
            .skip(1)
            .next()
            .unwrap()
            .to_upper_camel_case(),
        if is_default { "Default" } else { "" },
    )
}

fn generate_request_body_name(name: String, method: Option<&str>, media_type: String) -> String {
    format!(
        "{}{}{}RequestBody",
        name.to_upper_camel_case(),
        method.unwrap_or_default().to_upper_camel_case(),
        media_type
            .split("/")
            .skip(1)
            .next()
            .unwrap()
            .to_upper_camel_case()
    )
}

fn from_single_path_item(schemas: &mut BTreeMap<String, Schema>, name: String, pi: PathItem) {
    for (method, operation) in pi.into_iter() {
        if let Some(ReferenceOr::Item(body)) = operation.request_body {
            from_single_body(
                schemas,
                |mtn| generate_request_body_name(name.clone(), Some(method), mtn),
                body,
            );
        }

        if let Some(ReferenceOr::Item(resp)) = operation.responses.default {
            from_single_response(
                schemas,
                |mtn| generate_response_name(name.clone(), Some(method), None, true, mtn),
                resp,
            );
        }
        for (status_code, refor_resp) in operation.responses.responses.into_iter() {
            if let ReferenceOr::Item(resp) = refor_resp {
                from_single_response(
                    schemas,
                    |mtn| {
                        generate_response_name(
                            name.clone(),
                            Some(method),
                            Some(status_code.clone()),
                            false,
                            mtn,
                        )
                    },
                    resp,
                );
            }
        }
    }
}

fn gather_from_paths(
    schemas: &mut BTreeMap<String, Schema>,
    map: IndexMap<String, ReferenceOr<PathItem>>,
) {
    for (name, refor) in map.into_iter() {
        if let ReferenceOr::Item(pi) = refor {
            from_single_path_item(schemas, name, pi);
        }
    }
}
