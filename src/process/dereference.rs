use std::collections::HashMap;

use crate::error::Error;
use crate::schema::{Schema, SchemaScope};
use url::Url;

use serde_json::Value;

pub struct Dereferencer;

pub struct DereferencerContext {
    pub schemas: HashMap<String, Schema>,
    pub current: Vec<Url>,
    pub scope: SchemaScope,
}

#[derive(Default)]
pub struct DereferencerOptions {
    pub skip_root_internal_references: bool,
}

impl DereferencerOptions {
    pub fn with_skip_root_internal_references(&mut self, value: bool) -> &mut Self {
        self.skip_root_internal_references = value;
        self
    }

    pub fn process(&self, schema: &mut Schema) {
        let original = schema.clone();

        let mut root = schema.get_body_mut();
        let mut context = DereferencerContext {
            schemas: HashMap::new(),
            current: vec![],
            scope: SchemaScope::default(),
        };

        // add original schema copy
        context.current.push(original.get_url().clone());
        context
            .schemas
            .insert(original.get_url().to_string(), original);

        process_node(&mut root, &self, &mut context);
    }
}

impl DereferencerContext {
    pub fn resolve(&mut self, url: Url) {
        let key = url.to_string();

        self.schemas
            .entry(key)
            .or_insert_with(|| Schema::load_url(url.clone()).unwrap());

        self.current.push(url);
    }
}

impl Dereferencer {
    pub fn options() -> DereferencerOptions {
        DereferencerOptions {
            skip_root_internal_references: false,
        }
    }
}

fn ref_to_url(mixed: String, context: &mut DereferencerContext) -> Url {
    // todo: find some better solution
    let schema_url = if !mixed.starts_with("http://") && !mixed.starts_with('#') {
        let current_url = context.current.last().unwrap().clone();

        current_url.join(&mixed).unwrap().to_string()
    } else {
        mixed
    };

    Url::parse(&schema_url).unwrap()
}

fn dereference(
    reference: String,
    node: &mut Value,
    options: &DereferencerOptions,
    context: &mut DereferencerContext,
) -> Value {
    if context.scope.len() > 50 {
        panic!("Infinite reference occured!");
    }

    match parse_url(reference) {
        Ok((address, fragment)) => {
            if options.skip_root_internal_references
                && context.current.len() == 1
                && address.is_none()
            {
                return node.clone();
            }

            if let Some(path) = address.clone() {
                let url = ref_to_url(path, context);
                context.resolve(url);
            }

            let name = context.current.last().unwrap();
            let spec = context.schemas.get(&name.to_string()).unwrap();
            let schema = spec.get_body();

            let mut resolved = match fragment {
                Some(real_fragment) => match schema.pointer(real_fragment.as_ref()) {
                    Some(p) => Some(p.clone()),
                    None => {
                        log::warn!("{}.$ref{} not resolved", context.scope, real_fragment);
                        None
                    }
                },
                None => Some(schema.clone()),
            };

            if let Some(ref mut data) = resolved {
                process_node(data, options, context);
            }

            if address.is_some() {
                context.current.pop();
            }

            resolved.unwrap_or_else(|| node.clone())
        }
        Err(err) => {
            log::warn!("{}.$ref: {}", context.scope, err);
            node.clone()
        }
    }
}

fn process_ref(root: &mut Value, options: &DereferencerOptions, context: &mut DereferencerContext) {
    match root.as_object_mut().unwrap().get_mut("$ref").unwrap() {
        Value::String(reference) => {
            log::trace!("{}.$ref", context.scope);

            let mut dereferenced = dereference(reference.clone(), root, options, context);
            let result = dereferenced.as_object_mut().unwrap();

            for (key, value) in root.as_object().unwrap() {
                if key == "$ref" {
                    continue;
                }

                result.insert(key.clone(), value.clone());
            }

            *root = dereferenced;
        }
        _ => {
            log::warn!("{}.$ref has to be a string", context.scope);
        }
    }
}

fn process_node(
    root: &mut Value,
    options: &DereferencerOptions,
    context: &mut DereferencerContext,
) {
    match root {
        Value::Object(ref mut map) => {
            if map.contains_key("$ref") {
                process_ref(root, options, context)
            } else {
                for (property, value) in map.into_iter() {
                    context.scope.property(property);
                    process_node(value, options, context);
                    context.scope.pop();
                }
            }
        }
        Value::Array(a) => {
            for (index, mut x) in a.iter_mut().enumerate() {
                context.scope.index(index);
                process_node(&mut x, options, context);
                context.scope.pop();
            }
        }
        _ => {}
    }
}

fn parse_url(reference: String) -> Result<(Option<String>, Option<String>), Error> {
    let parts = reference.split('#').collect::<Vec<&str>>();

    match parts.len() {
        2 => {
            if parts[0].is_empty() {
                Ok((None, Some(parts[1].to_string())))
            } else {
                Ok((Some(parts[0].to_string()), Some(parts[1].to_string())))
            }
        }
        1 => Ok((Some(parts[0].to_string()), None)),
        _ => Err(Error::DereferenceError(format!(
            "Cannot parse: {}",
            reference
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn spec_from_file(file: &str) -> Schema {
        let url = Url::parse(&format!("file://{}/{}", env!("CARGO_MANIFEST_DIR"), file)).unwrap();
        Schema::load_url(url).unwrap()
    }

    #[test]
    #[should_panic(expected = "Infinite reference occured!")]
    fn test_infinite_ref() {
        let mut spec = spec_from_file("resources/test/json-schemas/07-with-infinite-ref.json");
        Dereferencer::options().process(&mut spec);
    }

    #[test]
    fn test_with_local_reference() {
        let mut spec = spec_from_file("resources/test/json-schemas/06-with-local-reference.json");
        Dereferencer::options().process(&mut spec);

        let expected = json!({
          "$id": "https://example.com/arrays.schema.json",
          "$schema": "http://json-schema.org/draft-07/schema#",
          "description": "A representation of a person, company, organization, or place",
          "type": "object",
          "properties": {
            "fruits": {
              "type": "array",
              "items": {
                "type": "string"
              }
            },
            "vegetables": {
              "type": "array",
              "items": {
                  "$id": "https://example.com/person.schema.json",
                  "$schema": "http://json-schema.org/draft-07/schema#",
                  "title": "Person",
                  "type": "object",
                  "properties": {
                    "firstName": {
                      "type": "string",
                      "description": "The person's first name."
                    },
                    "lastName": {
                      "type": "string",
                      "description": "The person's last name."
                    },
                    "age": {
                      "description": "Age in years which must be equal to or greater than zero.",
                      "type": "integer",
                      "minimum": 0
                    }
                  }
                }
            }
          }
        });

        assert_eq!(spec.get_body().to_string(), expected.to_string());
    }

    #[test]
    fn test_with_nested_remote_external_reference() {
        let mut spec =
            spec_from_file("resources/test/json-schemas/05-with-nested-remote-external-ref.json");
        Dereferencer::options().process(&mut spec);

        let expected = json!({
          "$id": "https://example.com/arrays.schema.json",
          "$schema": "http://json-schema.org/draft-07/schema#",
          "description": "Just a test",
          "type": "object",
          "properties": {
            "contexts": {
              "type": "array",
              "items": {
                  "enum": [
                      "docker"
                  ]
              }
            }
          }
        });

        assert_eq!(spec.get_body().to_string(), expected.to_string());
    }

    #[test]
    fn test_with_nested_external_reference() {
        let mut spec =
            spec_from_file("resources/test/json-schemas/04-with-nested-external-ref.json");
        Dereferencer::options().process(&mut spec);

        let expected = json!({
          "$id": "https://example.com/arrays.schema.json",
          "$schema": "http://json-schema.org/draft-07/schema#",
          "description": "Just a test",
          "type": "object",
          "properties": {
            "contexts": {
              "type": "array",
              "items": {
                  "title": "Docker",
                  "description": "Builds and deployments are normally run on the Bamboo agent’s native operating system",
                  "anyOf": [
                      {
                          "type": "string"
                      },
                      {
                          "type": "object",
                          "properties": {
                              "image": {
                                  "type": "string"
                              },
                              "volumes": {
                                  "type": "object",
                                  "default": {}
                              },
                              "use-default-volumes": {
                                  "type": "boolean",
                                  "default": false
                              }
                          },
                          "required": [
                              "image"
                          ]
                      }
                  ]
              }
            }
          }
        });

        assert_eq!(spec.get_body().to_string(), expected.to_string());
    }

    #[test]
    fn test_simple_with_external_reference() {
        let mut spec =
            spec_from_file("resources/test/json-schemas/03-simple-with-external-ref.json");
        Dereferencer::options().process(&mut spec);

        let expected = json!({
          "$id": "https://example.com/arrays.schema.json",
          "$schema": "http://json-schema.org/draft-07/schema#",
          "description": "Just a test",
          "type": "object",
          "properties": {
            "contexts": {
              "type": "array",
              "items": {
                  "type": "string",
                  "format": "regex",
                  "pattern": "http://schema.org",
                  "description": "override the @context property to ensure the schema.org URI is used"
              }
            }
          }
        });

        assert_eq!(spec.get_body().to_string(), expected.to_string());
    }

    #[test]
    fn test_simple_with_reference() {
        let mut spec = spec_from_file("resources/test/json-schemas/02-simple-with-reference.json");
        Dereferencer::options().process(&mut spec);

        let expected = json!({
            "$id": "https://example.com/arrays.schema.json",
            "$schema": "http://json-schema.org/draft-07/schema#",
            "description": "A representation of a person, company, organization, or place",
            "type": "object",
            "properties": {
              "fruits": {
                "type": "array",
                "items": {
                  "type": "string"
                }
              },
              "vegetables": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": [ "veggieName", "veggieLike" ],
                    "properties": {
                        "veggieName": {
                        "type": "string",
                        "description": "The name of the vegetable."
                        },
                        "veggieLike": {
                        "type": "boolean",
                        "description": "Do I like this vegetable?"
                        }
                    }
                }
              }
            },
            "definitions": {
              "veggie": {
                "type": "object",
                "required": [ "veggieName", "veggieLike" ],
                "properties": {
                  "veggieName": {
                    "type": "string",
                    "description": "The name of the vegetable."
                  },
                  "veggieLike": {
                    "type": "boolean",
                    "description": "Do I like this vegetable?"
                  }
                }
              }
            }
        });

        assert_eq!(spec.get_body().to_string(), expected.to_string());
    }
}
