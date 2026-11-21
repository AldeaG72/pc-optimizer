//! Startup programs as the interface shows them (3.12). Reads only; changes go through the
//! helper (`startup` request), which lists them again itself.

use pco_core::backend::{BackendError, RegistryBackend};
use pco_core::startup::{self, Advice, Impact, Location};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupView {
    pub id: String,
    pub name: String,
    pub command: String,
    pub location: Location,
    pub enabled: bool,
    /// Turning it on or off shows the UAC prompt (an entry for every user).
    pub needs_admin: bool,
    pub impact: Impact,
    pub advice: Option<Advice>,
    /// Translation key suffix of what the program is (`app.startup.what.<key>`).
    pub what: Option<&'static str>,
    pub size: Option<u64>,
}

pub fn views(
    reg: &dyn RegistryBackend,
    folders: &[(Location, Vec<String>)],
) -> Result<Vec<StartupView>, BackendError> {
    let mut out: Vec<StartupView> = startup::list(reg, folders)?
        .into_iter()
        .map(|e| StartupView {
            size: startup::exe_size(&e.command),
            impact: e.known.map_or(Impact::Unknown, |k| k.impact),
            advice: e.known.map(|k| k.advice),
            what: e.known.map(|k| k.key),
            needs_admin: e.location.needs_admin(),
            id: e.id,
            name: e.name,
            command: e.command,
            location: e.location,
            enabled: e.enabled,
        })
        .collect();
    out.sort_by_key(|v| (v.name.to_lowercase(), v.id.clone()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use pco_core::backend::memory::MemoryRegistry;
    use pco_core::backend::{ReadOnlyRegistry, RegData};
    use pco_core::recipe::{Hive, RegValue};

    #[test]
    fn known_programs_get_impact_and_advice() {
        let reg = ReadOnlyRegistry(MemoryRegistry::new().with_value(
            Hive::LocalMachine,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            "SecurityHealth",
            RegData::from_value(&RegValue::String(
                r"%windir%\system32\SecurityHealthSystray.exe".into(),
            )),
        ));
        let v = views(&reg, &[]).unwrap();
        assert_eq!(v.len(), 1);
        assert!(v[0].needs_admin);
        assert_eq!(v[0].advice, Some(Advice::Keep));
        assert_eq!(v[0].what, Some("security"));
        let json = serde_json::to_value(&v[0]).unwrap();
        assert_eq!(json["location"], "machine-run");
        assert_eq!(json["impact"], "low");
    }
}
