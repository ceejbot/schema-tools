use std::fmt::Display;

use crate::commands::GetSchemaCommand;
use clap::{Parser, Subcommand};
use schematools::process::flatten_allof;
use schematools::storage::SchemaStorage;
use schematools::tools;
use schematools::Client;

use crate::error::Error;
use schematools::process::{dereference, merge_allof, merge_openapi, name};
use schematools::schema::{path_to_url, Schema};

#[cfg(feature = "semver")]
pub mod bump_openapi;
pub mod patch;

#[derive(Clone, Debug, Parser)]
pub struct Opts {
    #[clap(subcommand)]
    pub command: Command,
}

impl Display for Opts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.command {
            Command::MergeOpenapi(_) => write!(f, "merge_openapi"),
            #[cfg(feature = "semver")]
            Command::BumpOpenapi(_) => write!(f, "bump_openapi"),
            Command::MergeAllOf(_) => write!(f, "merge_allof"),
            Command::FlattenAllOf(_) => write!(f, "flatten_allof"),
            Command::Dereference(_) => write!(f, "dereference"),
            Command::Name(_) => write!(f, "name"),
            #[cfg(feature = "json-patch")]
            Command::Patch(_) => write!(f, "patch"),
        }
    }
}

#[derive(Subcommand, Clone, Debug)]
pub enum Command {
    /// Merges openapi specifications
    MergeOpenapi(MergeOpenapiOpts),

    /// Bumps version of openapi specifications
    #[cfg(feature = "semver")]
    BumpOpenapi(BumpOpenapiOpts),

    /// Merges each occurrence of allOf to one json schema
    MergeAllOf(MergeAllOfOpts),

    /// A soft version of the allOf merge that does not touch single allOf instances.
    FlattenAllOf(MergeAllOfOpts),

    /// Recursively resolves all $ref occurrences in a schema file
    Dereference(DereferenceOpts),

    /// Create missing titles for all schemas in openapi specification file
    Name(NameOpts),

    // Apply json patch to schema
    #[cfg(feature = "json-patch")]
    Patch(PatchOpts),
}

#[derive(Clone, Debug, Parser)]
pub struct MergeOpenapiOpts {
    /// Path to json/yaml file
    pub file: String,

    /// Openapi file to merge with
    #[clap(long)]
    with: String,

    /// Should change tags of all endpoints of merged openapi
    #[clap(long)]
    retag: Option<String>,

    /// Should add info.x-version- attribute to openapi specification
    #[clap(long)]
    add_version: Option<String>,

    #[clap(flatten)]
    output: crate::commands::Output,

    #[clap(flatten)]
    verbose: crate::commands::Verbosity,
}

#[derive(Clone, Debug, Parser)]
pub struct BumpOpenapiOpts {
    /// Path to json/yaml file
    pub file: String,

    /// Path to previous version of openapi specification
    #[clap(long)]
    original: String,

    /// Type of bump
    #[clap(short, long, default_value = "x-version")]
    #[cfg(feature = "semver")]
    kind: bump_openapi::BumpKind,

    #[clap(flatten)]
    output: crate::commands::Output,

    #[clap(flatten)]
    verbose: crate::commands::Verbosity,
}

#[derive(Clone, Debug, Parser)]
pub struct MergeAllOfOpts {
    /// Path to json/yaml file
    pub file: Vec<String>,

    /// Leave invalid properties on allOf level
    #[clap(long)]
    leave_invalid_properties: bool,

    /// Filters to be applied on each root.allOf element
    #[clap(long, required = false)]
    filter: Vec<String>,

    #[clap(flatten)]
    output: crate::commands::Output,

    #[clap(flatten)]
    verbose: crate::commands::Verbosity,
}

#[derive(Clone, Debug, Parser)]
pub struct DereferenceOpts {
    /// Path to json/yaml file
    pub file: Vec<String>,

    /// Leaves internal references intact in root schema file
    #[clap(long)]
    skip_root_internal_references: bool,

    /// Creates internal references if refs where pointing to same place
    #[clap(long)]
    create_internal_references: bool,

    /// List of hostnames to skip dereference
    #[clap(long)]
    skip_references: Vec<String>,

    #[clap(flatten)]
    output: crate::commands::Output,

    #[clap(flatten)]
    verbose: crate::commands::Verbosity,
}

#[derive(Clone, Debug, Parser)]
#[allow(dead_code)]
pub struct NameOpts {
    /// Path to json/yaml file with openapi specification
    file: String,

    /// Reverts order of operationId generator to resource+method+version
    #[clap(long)]
    resource_method_version: bool,

    /// Should overwrite existing titles
    #[clap(long)]
    overwrite: bool,

    /// Should overwrite ambiguous titles
    #[clap(long)]
    overwrite_ambiguous: bool,

    /// Base name of parsed schema
    #[clap(long)]
    base_name: Option<String>,

    #[clap(flatten)]
    output: crate::commands::Output,

    #[clap(flatten)]
    verbose: crate::commands::Verbosity,
}

#[derive(Clone, Debug, Parser)]
pub struct PatchOpts {
    /// Path to json/yaml file with schema
    file: String,

    #[clap(subcommand)]
    pub action: patch::Action,

    #[clap(flatten)]
    output: crate::commands::Output,

    #[clap(flatten)]
    verbose: crate::commands::Verbosity,
}

impl GetSchemaCommand for Opts {
    fn get_schema(&self, client: &Client) -> Result<Schema, Error> {
        match &self.command {
            Command::MergeAllOf(opts) => {
                let urls = opts
                    .file
                    .iter()
                    .map(|s| path_to_url(s.clone()))
                    .collect::<Result<Vec<_>, _>>()?;

                Schema::load_urls(urls).map_err(Error::Schematools)
            }
            Command::FlattenAllOf(opts) => {
                let urls = opts
                    .file
                    .iter()
                    .map(|s| path_to_url(s.clone()))
                    .collect::<Result<Vec<_>, _>>()?;

                Schema::load_urls(urls).map_err(Error::Schematools)
            }
            Command::MergeOpenapi(opts) => Schema::load_url_with_client(
                path_to_url(opts.file.clone()).map_err(Error::Schematools)?,
                client,
            )
            .map_err(Error::Schematools),
            #[cfg(feature = "semver")]
            Command::BumpOpenapi(opts) => Schema::load_url_with_client(
                path_to_url(opts.file.clone()).map_err(Error::Schematools)?,
                client,
            )
            .map_err(Error::Schematools),
            Command::Dereference(opts) => {
                let urls = opts
                    .file
                    .iter()
                    .map(|s| path_to_url(s.clone()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(Error::Schematools)?;

                Schema::load_urls_with_client(urls, client).map_err(Error::Schematools)
            }
            Command::Name(opts) => Schema::load_url_with_client(
                path_to_url(opts.file.clone()).map_err(Error::Schematools)?,
                client,
            )
            .map_err(Error::Schematools),
            #[cfg(feature = "json-patch")]
            Command::Patch(opts) => Schema::load_url_with_client(
                path_to_url(opts.file.clone()).map_err(Error::Schematools)?,
                client,
            )
            .map_err(Error::Schematools),
        }
    }
}

impl Opts {
    pub fn run(&self, schema: &mut Schema, storage: &SchemaStorage) -> Result<(), Error> {
        match &self.command {
            Command::MergeAllOf(opts) => {
                merge_allof::Merger::options()
                    .with_leave_invalid_properties(opts.leave_invalid_properties)
                    .with_filter(tools::Filter::new(&opts.filter)?)
                    .process(schema, storage);
                Ok(())
            }
            Command::FlattenAllOf(opts) => {
                flatten_allof::Merger::options()
                    .with_leave_invalid_properties(opts.leave_invalid_properties)
                    .with_filter(tools::Filter::new(&opts.filter)?)
                    .process(schema, storage);
                Ok(())
            }
            Command::MergeOpenapi(opts) => {
                let merge = Schema::load_url(path_to_url(opts.with.clone())?)?;

                merge_openapi::Merger::options(merge)
                    .with_retag(opts.retag.clone())
                    .with_add_version(opts.add_version.clone())
                    .process(schema)
                    .map_err(Error::Schematools)
            }
            #[cfg(feature = "semver")]
            Command::BumpOpenapi(opts) => {
                let original = Schema::load_url(path_to_url(opts.original.clone())?)?;

                ::schematools::process::bump_openapi::Bumper::options(original)
                    .with_kind(opts.kind.into())
                    .process(schema)
                    .map_err(Error::Schematools)
            }
            Command::Dereference(opts) => {
                dereference::Dereferencer::options()
                    .with_skip_root_internal_references(opts.skip_root_internal_references)
                    .with_create_internal_references(opts.create_internal_references)
                    .with_skip_references(opts.skip_references.clone())
                    .process(schema, storage);
                Ok(())
            }
            Command::Name(opts) => {
                //name::JsonSchemaNamer::options()
                //    .with_base_name(opts.base_name.clone())
                //    .with_overwrite(opts.overwrite)
                //    .process(schema)

                name::OpenapiNamer::options()
                    .with_resource_method_version(opts.resource_method_version)
                    .with_overwrite(opts.overwrite)
                    .with_overwrite_ambiguous(opts.overwrite_ambiguous)
                    .process(schema)
                    .map_err(Error::Schematools)
            }
            #[cfg(feature = "json-patch")]
            Command::Patch(opts) => {
                let action = opts.action.clone().into();
                ::schematools::process::patch::execute(schema, &action).map_err(Error::Schematools)
            }
        }
    }
}

pub fn execute(opts: Opts, client: &Client) -> Result<(), Error> {
    let mut schema = opts.get_schema(client)?;
    let storage = &SchemaStorage::new(&schema, client);

    // todo: ...
    match &opts.command {
        Command::MergeAllOf(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?; // todo: ...
            o.output.show(schema.get_body());

            Ok(())
        }
        Command::FlattenAllOf(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?; // todo: ...
            o.output.show(schema.get_body());

            Ok(())
        }
        Command::MergeOpenapi(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?;
            o.output.show(schema.get_body());

            Ok(())
        }
        #[cfg(feature = "semver")]
        Command::BumpOpenapi(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?;
            o.output.show(schema.get_body());

            Ok(())
        }
        Command::Dereference(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?;
            o.output.show(schema.get_body());

            Ok(())
        }
        Command::Name(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?;
            o.output.show(schema.get_body());

            Ok(())
        }
        #[cfg(feature = "json-patch")]
        Command::Patch(o) => {
            o.verbose.start()?;
            opts.run(&mut schema, storage)?;
            o.output.show(schema.get_body());

            Ok(())
        }
    }
}
