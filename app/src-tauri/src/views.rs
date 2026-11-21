//! What the interface shows, built from the engine without changing anything: every recipe
//! with its texts, its state on this PC and the exact changes it would make.

use pco_core::backend::{RegData, RegRead, RegistryBackend, ServiceBackend, ServiceStart};
use pco_core::engine::{Engine, RecipePlan, RecipeStatus, Snapshot};
use pco_core::guard::SystemInfo;
use pco_core::journal::Journal;
use pco_core::recipe::{Category, Lang, Profile, Recipe, RegValue, RequiresRestart, Risk};
use pco_helper::protocol::{NotApplicableReason, Request};
use serde::{Deserialize, Serialize};

/// What the Tweaks page reads: every recipe, and whether this PC is a laptop (for the
/// Laptop profile of "Recommended").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Catalog {
    pub recipes: Vec<RecipeView>,
    pub laptop: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    pub risk: Risk,
    pub profiles: Vec<Profile>,
    pub impact: String,
    pub why: String,
    pub breaks: String,
    pub restart: RequiresRestart,
    pub sources: Vec<String>,
    pub verified: bool,
    pub status: StatusView,
    pub changes: Vec<ChangeView>,
    /// Applying it shows the UAC prompt (HKLM or a service).
    pub needs_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum StatusView {
    NotApplied,
    Applied,
    AlreadyInPlace,
    ModifiedOutside,
    NotApplicable {
        reason: NotApplicableReason,
    },
    /// The state could not be read (shown, not hidden).
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeView {
    pub target: String,
    pub current: StateView,
    pub desired: StateView,
    pub in_place: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StateView {
    Missing,
    Value { text: String },
    Service { start: ServiceStart },
}

fn reg_text(data: &RegData) -> String {
    match data.to_value() {
        Some(RegValue::Dword(n)) => n.to_string(),
        Some(RegValue::Qword(n)) => n.to_string(),
        Some(RegValue::String(s)) => format!("\"{s}\""),
        None => format!("({} bytes, type {})", data.bytes.len(), data.reg_type),
    }
}

fn state_view(s: &Snapshot) -> StateView {
    match s {
        Snapshot::Registry { state, .. } => match state {
            RegRead::KeyMissing | RegRead::ValueMissing => StateView::Missing,
            RegRead::Present(data) => StateView::Value {
                text: reg_text(data),
            },
        },
        Snapshot::Service { state, .. } => StateView::Service { start: state.start },
    }
}

fn status_view(plan: &RecipePlan) -> StatusView {
    match &plan.status {
        RecipeStatus::NotApplied => StatusView::NotApplied,
        RecipeStatus::Applied => StatusView::Applied,
        RecipeStatus::AlreadyInPlace => StatusView::AlreadyInPlace,
        RecipeStatus::ModifiedOutside => StatusView::ModifiedOutside,
        RecipeStatus::NotApplicable(n) => StatusView::NotApplicable {
            reason: pco_helper::serve::not_applicable(n.clone()),
        },
    }
}

/// Every recipe as the interface shows it. Only reads: give it read-only backends.
pub fn recipe_views(
    recipes: &[Recipe],
    lang: Lang,
    registry: &mut dyn RegistryBackend,
    services: &mut dyn ServiceBackend,
    journal: &mut Journal,
    system: SystemInfo,
) -> Vec<RecipeView> {
    let engine = Engine::new(registry, services, journal, system);
    recipes
        .iter()
        .map(|r| {
            let (status, changes) = match engine.plan(r) {
                Ok(plan) => (
                    status_view(&plan),
                    plan.actions
                        .iter()
                        .map(|a| ChangeView {
                            target: a.current.target(),
                            current: state_view(&a.current),
                            desired: state_view(&a.desired),
                            in_place: a.in_place,
                        })
                        .collect(),
                ),
                Err(e) => (
                    StatusView::Error {
                        message: e.to_string(),
                    },
                    Vec::new(),
                ),
            };
            RecipeView {
                id: r.id.clone(),
                name: r.name.get(lang).to_owned(),
                description: r.description.get(lang).to_owned(),
                category: r.category,
                risk: r.risk,
                profiles: r.profiles.clone(),
                impact: r.impact.get(lang).to_owned(),
                why: r.why.get(lang).to_owned(),
                breaks: r.breaks.get(lang).to_owned(),
                restart: r.requires_restart,
                sources: r.sources.clone(),
                verified: r.verified,
                status,
                changes,
                needs_admin: r.needs_admin(),
            }
        })
        .collect()
}

/// What the interface may ask the helper for (checked again by the helper itself).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HelperRequest {
    Apply {
        ids: Vec<String>,
    },
    Undo {
        ids: Vec<String>,
    },
    UndoAll,
    Bench {
        target: String,
        label: String,
        runs: u32,
        duration: u32,
        warmup: u32,
    },
}

impl HelperRequest {
    /// The helper request, if every ID looks like a recipe ID.
    pub fn to_request(&self) -> Result<Request, String> {
        let check = |ids: &Vec<String>| {
            if ids.is_empty() {
                return Err("no recipes".to_owned());
            }
            match ids
                .iter()
                .find(|id| !pco_helper::protocol::is_recipe_id(id))
            {
                Some(bad) => Err(format!("not a recipe ID: {bad}")),
                None => Ok(ids.clone()),
            }
        };
        Ok(match self {
            HelperRequest::Apply { ids } => Request::Apply { ids: check(ids)? },
            HelperRequest::Undo { ids } => Request::Undo { ids: check(ids)? },
            HelperRequest::UndoAll => Request::UndoAll,
            HelperRequest::Bench {
                target,
                label,
                runs,
                duration,
                warmup,
            } => {
                let request = Request::Bench {
                    target: target.clone(),
                    after: label == "after",
                    runs: *runs,
                    duration: *duration,
                    warmup: *warmup,
                };
                // The same checks the helper makes, before showing a UAC prompt.
                pco_helper::protocol::parse_request(&request.to_line())
                    .map_err(|r| format!("{r:?}"))?
            }
        })
    }
}

pub fn lang_from(code: &str) -> Lang {
    if code.eq_ignore_ascii_case("ro") {
        Lang::Ro
    } else {
        Lang::En
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use pco_core::backend::memory::{MemoryRegistry, MemoryServices};
    use pco_core::backend::{ReadOnlyRegistry, ServiceState};
    use pco_core::recipe::{self, Edition, Hive};

    fn recipes() -> Vec<Recipe> {
        recipe::load_embedded(pco_core::embedded::RECIPES).unwrap()
    }

    fn system() -> SystemInfo {
        SystemInfo {
            build: 26100,
            edition: Some(Edition::Pro),
            form_factor: None,
        }
    }

    fn views(reg: MemoryRegistry, svc: MemoryServices, journal: &mut Journal) -> Vec<RecipeView> {
        let mut reg = ReadOnlyRegistry(reg);
        let mut svc = svc;
        recipe_views(&recipes(), Lang::Ro, &mut reg, &mut svc, journal, system())
    }

    fn find<'a>(views: &'a [RecipeView], id: &str) -> &'a RecipeView {
        views.iter().find(|v| v.id == id).unwrap()
    }

    #[test]
    fn fresh_pc_shows_every_recipe_with_its_changes() {
        let mut journal = Journal::open_in_memory().unwrap();
        let svc = MemoryServices::new().with_service(
            "Fax",
            ServiceState {
                start: ServiceStart::Manual,
                delayed_auto_start: false,
            },
        );
        let v = views(MemoryRegistry::new(), svc, &mut journal);
        assert_eq!(v.len(), recipes().len());

        let dvr = find(&v, "gaming.game-dvr-off");
        assert_eq!(dvr.name, "Captură în fundal (Game DVR) oprită");
        assert_eq!(dvr.status, StatusView::NotApplied);
        assert!(!dvr.needs_admin);
        assert_eq!(dvr.changes.len(), 2);
        assert_eq!(
            dvr.changes[0].target,
            r"HKCU\System\GameConfigStore\GameDVR_Enabled"
        );
        assert_eq!(dvr.changes[0].current, StateView::Missing);
        assert_eq!(
            dvr.changes[0].desired,
            StateView::Value { text: "0".into() }
        );

        let fax = find(&v, "services.fax-disabled");
        assert!(fax.needs_admin);
        assert_eq!(
            fax.changes[0].desired,
            StateView::Service {
                start: ServiceStart::Disabled
            }
        );

        // Serialized the way the interface reads it.
        let json = serde_json::to_value(dvr).unwrap();
        assert_eq!(json["status"]["code"], "not_applied");
        assert_eq!(json["category"], "gaming");
        assert_eq!(json["risk"], "safe");
        assert_eq!(json["changes"][0]["desired"]["kind"], "value");
        assert_eq!(json["needsAdmin"], false);
    }

    #[test]
    fn already_set_and_not_applicable() {
        let mut journal = Journal::open_in_memory().unwrap();
        let reg = MemoryRegistry::new().with_value(
            Hive::CurrentUser,
            r"Software\Microsoft\GameBar",
            "AutoGameModeEnabled",
            RegData::from_value(&RegValue::Dword(1)),
        );
        let v = views(reg, MemoryServices::new(), &mut journal);
        assert_eq!(
            find(&v, "gaming.game-mode-on").status,
            StatusView::AlreadyInPlace
        );
        assert_eq!(
            find(&v, "services.fax-disabled").status,
            StatusView::NotApplicable {
                reason: NotApplicableReason::ServiceMissing { name: "Fax".into() }
            }
        );
    }

    /// The real registry and services of this PC, read through the read-only wrappers the
    /// app uses: every recipe gets a state, and nothing can be written.
    #[cfg(windows)]
    #[test]
    fn reads_this_pc_without_changing_it() {
        use pco_core::backend::windows::{WindowsRegistry, WindowsServices};
        let mut journal = Journal::open_in_memory().unwrap();
        let mut reg = ReadOnlyRegistry(WindowsRegistry::system());
        let mut svc = WindowsServices::read_only();
        let v = recipe_views(
            &recipes(),
            Lang::En,
            &mut reg,
            &mut svc,
            &mut journal,
            system(),
        );
        assert_eq!(v.len(), recipes().len());
        for r in &v {
            assert!(
                !matches!(r.status, StatusView::Error { .. }),
                "{}: {:?}",
                r.id,
                r.status
            );
        }
        let data = RegData::from_value(&RegValue::Dword(1));
        assert!(reg
            .write(Hive::CurrentUser, r"Software\PcOptimizerTest", "x", &data)
            .is_err());
    }

    #[test]
    fn requests_are_checked_before_starting_the_helper() {
        let ok: HelperRequest =
            serde_json::from_str(r#"{"op":"apply","ids":["gaming.game-mode-on"]}"#).unwrap();
        assert_eq!(
            ok.to_request(),
            Ok(Request::Apply {
                ids: vec!["gaming.game-mode-on".into()]
            })
        );
        let all: HelperRequest = serde_json::from_str(r#"{"op":"undo_all"}"#).unwrap();
        assert_eq!(all.to_request(), Ok(Request::UndoAll));
        let bad: HelperRequest =
            serde_json::from_str(r#"{"op":"apply","ids":["C:\\x.yaml"]}"#).unwrap();
        assert!(bad.to_request().is_err());
        let empty: HelperRequest = serde_json::from_str(r#"{"op":"undo","ids":[]}"#).unwrap();
        assert!(empty.to_request().is_err());
        assert!(serde_json::from_str::<HelperRequest>(r#"{"op":"run","cmd":"x"}"#).is_err());
    }
}
