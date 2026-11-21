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
pub use load::{load_dir, load_embedded, parse_recipe};
pub use model::{
    Action, AppxAction, Category, CleanupAction, CleanupTarget, Edition, FormFactor, Hive, I18n,
    Lang, NetAdapterAction, Platform, PowerPlanAction, PowerSettingAction, Profile, Recipe,
    RegKind, RegValue, RegistryAction, RegistryOp, RequiresRestart, Risk, ServiceAction,
    ServiceStartup,
};
pub use safety::is_protected_package;
