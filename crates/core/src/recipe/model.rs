//! Recipe data model.
//!
//! Public types are the *validated* form of a recipe. The `Raw*` types mirror the YAML
//! one-to-one and are only used during loading (see `validate.rs`).

use std::fmt;
use std::path::PathBuf;

use serde::Deserialize;

/// A text in both supported languages.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct I18n {
    pub ro: String,
    pub en: String,
}

impl I18n {
    pub fn get(&self, lang: Lang) -> &str {
        match lang {
            Lang::Ro => &self.ro,
            Lang::En => &self.en,
        }
    }
}

/// The two supported languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Ro,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Windows,
    Gaming,
    Power,
    Input,
    Network,
    Audio,
    Debloat,
    Privacy,
    Cleanup,
    GpuNvidia,
    GpuAmd,
    Services,
    Advanced,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Moderate,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Recommended,
    Laptop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RequiresRestart {
    None,
    Explorer,
    Signout,
    Reboot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edition {
    Home,
    Pro,
    Enterprise,
    Education,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FormFactor {
    #[default]
    Any,
    Desktop,
    Laptop,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Platform {
    pub min_build: u32,
    #[serde(default)]
    pub max_build: Option<u32>,
    /// Empty means "all editions".
    #[serde(default)]
    pub editions: Vec<Edition>,
    #[serde(default)]
    pub form_factor: FormFactor,
}

/// Registry root. Only the two hives the app is allowed to touch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, serde::Serialize)]
pub enum Hive {
    #[serde(rename = "HKCU")]
    CurrentUser,
    #[serde(rename = "HKLM")]
    LocalMachine,
}

impl Hive {
    pub fn as_str(self) -> &'static str {
        match self {
            Hive::CurrentUser => "HKCU",
            Hive::LocalMachine => "HKLM",
        }
    }

    /// Writing under HKLM needs administrator rights.
    pub fn needs_admin(self) -> bool {
        matches!(self, Hive::LocalMachine)
    }
}

impl fmt::Display for Hive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegKind {
    Dword,
    Qword,
    String,
}

/// A typed registry value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegValue {
    Dword(u32),
    Qword(u64),
    String(String),
}

impl RegValue {
    pub fn kind(&self) -> RegKind {
        match self {
            RegValue::Dword(_) => RegKind::Dword,
            RegValue::Qword(_) => RegKind::Qword,
            RegValue::String(_) => RegKind::String,
        }
    }
}

/// What a registry action does to its value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegistryOp {
    Set(RegValue),
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RegistryAction {
    pub hive: Hive,
    /// Path below the hive, without a leading or trailing backslash.
    pub key: String,
    /// Value name inside the key.
    pub value: String,
    pub op: RegistryOp,
    /// Apply only if the value currently equals this (e.g. the Windows default); otherwise
    /// the recipe is "not applicable", so a value the user chose is never overwritten.
    pub only_if: Option<RegValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStartup {
    Automatic,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServiceAction {
    /// Service (key) name, e.g. `Fax` — not the display name.
    pub name: String,
    pub startup: ServiceStartup,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    Registry(RegistryAction),
    Service(ServiceAction),
}

impl Action {
    pub fn needs_admin(&self) -> bool {
        match self {
            Action::Registry(r) => r.hive.needs_admin(),
            Action::Service(_) => true,
        }
    }
}

/// A validated recipe: one documented, reversible tweak.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub id: String,
    pub version: u32,
    pub name: I18n,
    pub description: I18n,
    pub category: Category,
    pub risk: Risk,
    pub profiles: Vec<Profile>,
    pub impact: I18n,
    pub why: I18n,
    pub breaks: I18n,
    pub requires_restart: RequiresRestart,
    pub platform: Platform,
    pub sources: Vec<String>,
    pub verified: bool,
    pub actions: Vec<Action>,
    /// File the recipe was loaded from (for error messages and `pco list`).
    pub source_file: PathBuf,
}

impl Recipe {
    pub fn needs_admin(&self) -> bool {
        self.actions.iter().any(Action::needs_admin)
    }
}

// ---------------------------------------------------------------------------
// Raw (as-written) forms, used only while loading.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRecipe {
    pub id: String,
    pub version: u32,
    pub name: I18n,
    pub description: I18n,
    pub category: Category,
    pub risk: Risk,
    pub profiles: Vec<Profile>,
    pub impact: I18n,
    pub why: I18n,
    pub breaks: I18n,
    pub requires_restart: RequiresRestart,
    pub platform: Platform,
    pub sources: Vec<String>,
    pub verified: bool,
    pub actions: Vec<RawAction>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum RawAction {
    Registry(RawRegistryAction),
    Service(ServiceAction),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawRegistryAction {
    pub hive: Hive,
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub kind: Option<RegKind>,
    #[serde(default)]
    pub set: Option<serde_yaml_ng::Value>,
    #[serde(default)]
    pub delete: Option<bool>,
    #[serde(default)]
    pub only_if: Option<serde_yaml_ng::Value>,
}
