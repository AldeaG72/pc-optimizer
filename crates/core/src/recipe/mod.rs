//! Recipes: YAML files describing one documented, reversible tweak each.
//!
//! The format is defined by `recipes/schema/recipe.schema.json`. The Rust types here enforce
//! the same rules (a test checks that both agree) plus a few stricter ones the JSON schema
//! cannot express, such as "a `dword` value must fit in 32 bits".

mod error;
mod load;
mod model;
mod safety;
mod validate;

pub use error::{LoadErrors, RecipeError};
pub use load::{load_dir, parse_recipe};
pub use model::{
    Action, Category, Edition, FormFactor, Hive, I18n, Platform, Profile, Recipe, RegKind,
    RegValue, RegistryAction, RegistryOp, RequiresRestart, Risk, ServiceAction, ServiceStartup,
};
