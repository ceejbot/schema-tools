//! Implementation of the `process merge_soft` subcommand.
//!
//! This subcommand is a variation on `merge_allof` that *does not* move the first-mentioned
//! allOf to be a sibling of the properties.

use crate::{
    resolver::SchemaResolver, schema::Schema, scope::SchemaScope, storage::SchemaStorage, tools,
};
use serde_json::Value;

pub struct Merger;

#[derive(Debug, Clone)]
pub struct FlattenAllOfOptions {
    pub leave_invalid_properties: bool,
    pub filter: tools::Filter,
}

impl FlattenAllOfOptions {
    pub fn with_leave_invalid_properties(&mut self, value: bool) -> &mut Self {
        self.leave_invalid_properties = value;
        self
    }

    pub fn with_filter(&mut self, value: tools::Filter) -> &mut Self {
        self.filter = value;
        self
    }

    pub fn process(&self, schema: &mut Schema, storage: &SchemaStorage) {
        let resolver = SchemaResolver::new(schema, storage);

        let root = schema.get_body_mut();
        let mut scope = SchemaScope::default();

        process_node(root, self, &mut scope, &resolver);
    }
}

impl Merger {
    pub fn options() -> FlattenAllOfOptions {
        FlattenAllOfOptions {
            leave_invalid_properties: false,
            filter: tools::Filter::default(),
        }
    }
}

fn process_soft_merge(
    root: &mut Value,
    options: &FlattenAllOfOptions,
    scope: &mut SchemaScope,
    resolver: &SchemaResolver,
) {
    if !options.filter.check(root, true) {
        return log::info!("allOf skipped because of filter");
    }

    let Some(root_mutable) = root.as_object_mut() else {
        log::info!("can't get mutable ownership of this schema");
        return;
    };
    // sheer paranoia, because we can unwrap safely here.
    let Some(all_of_item) = root_mutable.get_mut("allOf") else {
        log::debug!("allOf skipped because it doesn't exist");
        return;
    };

    let Value::Array(schemas) = all_of_item else {
        log::info!("skipping schema with non-array allOf; this is a bug in your schema generator");
        return;
    };

    let Some(first_schema) = schemas.pop() else {
        return log::info!(
            "skipping schema with empty allOf; this is a bug in your schema generator"
        );
    };
    // We know we have at least one entry. We merge all other entries into the first one,
    // then merge that into the parent.
    log::debug!("{}.allOf", scope);

    let mut first = resolver
        .resolve(&first_schema, scope, |v, ss| {
            let mut node = v.clone();
            process_node(&mut node, options, ss, resolver);
            Ok(node)
        })
        .unwrap();

    schemas.iter().for_each(|xs| {
        let value = resolver
            .resolve(xs, scope, |v, ss| {
                let mut node = v.clone();
                process_node(&mut node, options, ss, resolver);
                Ok(node)
            })
            .unwrap();
        merge_properties(&mut first, value);
    });

    // todo: leave_invalid_properties vs
    root_mutable.remove("allOf");
    merge_properties(root, first);
}

fn process_node(
    root: &mut Value,
    options: &FlattenAllOfOptions,
    scope: &mut SchemaScope,
    resolver: &SchemaResolver,
) {
    match root {
        Value::Object(ref mut map) => {
            // todo: allOf deep
            // go deeper first
            {
                for (property, value) in map.into_iter() {
                    scope.any(property);
                    process_node(value, options, scope, resolver);
                    scope.pop();
                }
            }

            // process allOf
            if map.contains_key("allOf") {
                process_soft_merge(root, options, scope, resolver)
            }
        }
        Value::Array(a) => {
            for (index, x) in a.iter_mut().enumerate() {
                scope.index(index);
                process_node(x, options, scope, resolver);
                scope.pop();
            }
        }
        _ => {}
    }
}

static SKIP_PROPS: [&str; 3] = ["allOf", "oneOf", "discriminator"];

fn merge_properties(a: &mut Value, b: Value) {
    match (a, b) {
        (a @ &mut Value::Object(_), Value::Object(b)) => {
            let a = a.as_object_mut().unwrap();
            for (k, v) in b {
                if SKIP_PROPS.contains(&k.as_str()) {
                    continue;
                }
                merge_properties(a.entry(k).or_insert(Value::Null), v);
            }
        }
        (a @ &mut Value::Array(_), Value::Array(b)) => {
            let a = a.as_array_mut().unwrap();
            for v in b {
                if !a.contains(&v) {
                    a.push(v);
                }
            }
        }
        (a, b) => *a = b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Client;

    #[test]
    fn test_merge_soft_single() {
        let input =
            serde_json::from_str(include_str!("fixtures/merge_soft_single_input.json")).unwrap();
        let expected: Value =
            serde_json::from_str(include_str!("fixtures/merge_soft_single_expected.json")).unwrap();

        let mut schema = Schema::from_json(input);
        let client = Client::new();
        let ss = SchemaStorage::new(&schema, &client);
        Merger::options().process(&mut schema, &ss);
        assert_eq!(schema.get_body(), &expected);
    }

    #[test]
    fn test_merge_soft_double() {
        let input =
            serde_json::from_str(include_str!("fixtures/merge_soft_double_input.json")).unwrap();
        let expected: Value =
            serde_json::from_str(include_str!("fixtures/merge_soft_double_expected.json")).unwrap();

        let mut schema = Schema::from_json(input);
        let client = Client::new();
        let ss = SchemaStorage::new(&schema, &client);
        Merger::options().process(&mut schema, &ss);
        assert_eq!(schema.get_body(), &expected);
    }

    #[test]
    fn test_merge_soft_props_only() {
        let input =
            serde_json::from_str(include_str!("fixtures/merge_soft_propsonly_input.json")).unwrap();
        let expected: Value =
            serde_json::from_str(include_str!("fixtures/merge_soft_propsonly_expected.json"))
                .unwrap();

        let mut schema = Schema::from_json(input);
        let client = Client::new();
        let ss = SchemaStorage::new(&schema, &client);
        Merger::options().process(&mut schema, &ss);
        assert_eq!(schema.get_body(), &expected);
    }

    #[test]
    fn test_merge_soft_complex() {
        let input =
            serde_json::from_str(include_str!("fixtures/merge_soft_complex_input.json")).unwrap();
        let expected: Value =
            serde_json::from_str(include_str!("fixtures/merge_soft_complex_expected.json"))
                .unwrap();

        let mut schema = Schema::from_json(input);
        let client = Client::new();
        let ss = SchemaStorage::new(&schema, &client);
        Merger::options().process(&mut schema, &ss);
        assert_eq!(schema.get_body(), &expected);
    }
}
