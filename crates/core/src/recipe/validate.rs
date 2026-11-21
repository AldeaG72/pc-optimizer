//! Turns a `RawRecipe` (as written) into a validated `Recipe`.

use std::collections::HashSet;
use std::path::Path;

use serde_yaml_ng::Value;

use super::error::RecipeError;
use super::model::{
    Action, AppxAction, I18n, NetAdapterAction, PowerPlanAction, PowerSettingAction, RawAction,
    RawRecipe, RawRegistryAction, Recipe, RegKind, RegValue, RegistryAction, RegistryOp,
    ServiceAction,
};
use super::safety;

/// Lowest Windows 11 build (21H2).
pub const MIN_WINDOWS_11_BUILD: u32 = 22000;

/// A validation problem: (field path, message).
type Problem = (String, String);

pub(crate) fn validate(raw: RawRecipe, path: &Path) -> Result<Recipe, RecipeError> {
    let invalid = |(field, message): Problem| RecipeError::Invalid {
        path: path.to_path_buf(),
        field,
        message,
    };

    check_id(&raw.id).map_err(|m| invalid(("id".into(), m)))?;
    if raw.version < 1 {
        return Err(invalid(("version".into(), "must be 1 or higher".into())));
    }
    for (field, text) in [
        ("name", &raw.name),
        ("description", &raw.description),
        ("impact", &raw.impact),
        ("why", &raw.why),
        ("breaks", &raw.breaks),
    ] {
        check_i18n(field, text).map_err(invalid)?;
    }

    let mut seen = HashSet::new();
    if !raw.profiles.iter().all(|p| seen.insert(*p)) {
        return Err(invalid((
            "profiles".into(),
            "each profile may appear only once".into(),
        )));
    }

    if raw.platform.min_build < MIN_WINDOWS_11_BUILD {
        return Err(invalid((
            "platform.min_build".into(),
            format!("must be at least {MIN_WINDOWS_11_BUILD} (Windows 11 only)"),
        )));
    }
    if let Some(max) = raw.platform.max_build {
        if max < raw.platform.min_build {
            return Err(invalid((
                "platform.max_build".into(),
                "must not be lower than `min_build`".into(),
            )));
        }
    }

    if raw.sources.is_empty() {
        return Err(invalid((
            "sources".into(),
            "at least one source is required: no tweak without a source".into(),
        )));
    }
    for (i, source) in raw.sources.iter().enumerate() {
        check_url(source).map_err(|m| invalid((format!("sources[{i}]"), m)))?;
    }

    if raw.actions.is_empty() {
        return Err(invalid((
            "actions".into(),
            "at least one action is required".into(),
        )));
    }
    check_cleanup(&raw).map_err(invalid)?;
    let actions = raw
        .actions
        .into_iter()
        .enumerate()
        .map(|(i, a)| convert_action(i, a))
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;

    for (i, action) in actions.iter().enumerate() {
        if let Err((field, reason)) = safety::check(action) {
            return Err(RecipeError::Forbidden {
                path: path.to_path_buf(),
                field: format!("actions[{i}].{field}"),
                reason,
            });
        }
    }

    Ok(Recipe {
        id: raw.id,
        version: raw.version,
        name: raw.name,
        description: raw.description,
        category: raw.category,
        risk: raw.risk,
        profiles: raw.profiles,
        impact: raw.impact,
        why: raw.why,
        breaks: raw.breaks,
        requires_restart: raw.requires_restart,
        platform: raw.platform,
        sources: raw.sources,
        verified: raw.verified,
        actions,
        source_file: path.to_path_buf(),
    })
}

/// Same rule as the schema: `^[a-z0-9]+(\.[a-z0-9-]+)+$`, e.g. `gaming.game-dvr-off`.
fn check_id(id: &str) -> Result<(), String> {
    let msg = || {
        format!(
            "`{id}` is not a valid id: use lowercase `category.name`, \
             e.g. `gaming.game-dvr-off`"
        )
    };
    let mut parts = id.split('.');
    let first = parts.next().unwrap_or_default();
    if first.is_empty()
        || !first
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    {
        return Err(msg());
    }
    let mut rest = 0;
    for part in parts {
        rest += 1;
        if part.is_empty()
            || !part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(msg());
        }
    }
    if rest == 0 {
        return Err(msg());
    }
    Ok(())
}

fn check_i18n(field: &str, text: &I18n) -> Result<(), Problem> {
    for (lang, value) in [("ro", &text.ro), ("en", &text.en)] {
        if value.trim().is_empty() {
            return Err((format!("{field}.{lang}"), "text must not be empty".into()));
        }
    }
    Ok(())
}

fn check_url(source: &str) -> Result<(), String> {
    let rest = source
        .strip_prefix("https://")
        .or_else(|| source.strip_prefix("http://"));
    match rest {
        Some(rest) if !rest.is_empty() && !source.chars().any(char::is_whitespace) => Ok(()),
        _ => Err(format!("`{source}` is not a web address (https://...)")),
    }
}

fn convert_action(i: usize, raw: RawAction) -> Result<Action, Problem> {
    let field = |name: &str| format!("actions[{i}].{name}");
    match raw {
        RawAction::Registry(r) => convert_registry(i, r).map(Action::Registry),
        RawAction::Service(s) => check_service(i, &s).map(|()| Action::Service(s)),
        RawAction::PowerPlan(p) => Ok(Action::PowerPlan(PowerPlanAction {
            scheme: guid(&p.scheme).map_err(|m| (field("scheme"), m))?,
        })),
        RawAction::PowerSetting(p) => {
            if p.ac.is_none() && p.dc.is_none() {
                return Err((
                    field("ac"),
                    "a power setting needs `ac`, `dc` or both".into(),
                ));
            }
            Ok(Action::PowerSetting(PowerSettingAction {
                subgroup: guid(&p.subgroup).map_err(|m| (field("subgroup"), m))?,
                setting: guid(&p.setting).map_err(|m| (field("setting"), m))?,
                ac: p.ac,
                dc: p.dc,
            }))
        }
        RawAction::NetAdapter(n) => {
            if n.value.trim().is_empty() || n.value.trim() != n.value || n.value.contains('\\') {
                return Err((
                    field("value"),
                    "value name must not be empty, start/end with spaces or contain `\\`".into(),
                ));
            }
            Ok(Action::NetAdapter(NetAdapterAction {
                set: convert_value(n.kind, n.set).map_err(|m| (field("set"), m))?,
                value: n.value,
                advertised: n.advertised.unwrap_or(true),
            }))
        }
        RawAction::Appx(a) => {
            check_package(&a.package).map_err(|m| (field("package"), m))?;
            Ok(Action::Appx(AppxAction { package: a.package }))
        }
        RawAction::Cleanup(c) => Ok(Action::Cleanup(c)),
    }
}

/// A GUID as `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx` (braces allowed), stored lowercase.
pub(crate) fn guid(text: &str) -> Result<String, String> {
    let bare = text.trim_start_matches('{').trim_end_matches('}');
    let groups: Vec<&str> = bare.split('-').collect();
    let shape_ok = groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()));
    if shape_ok {
        Ok(bare.to_ascii_lowercase())
    } else {
        Err(format!(
            "`{text}` is not a GUID like 381b4222-f694-41f0-9685-ff5bb260df2e"
        ))
    }
}

/// Package names go into a PowerShell command, so only the characters Windows allows in
/// them: letters, digits, `.` and `-`.
pub(crate) fn check_package(name: &str) -> Result<(), String> {
    let ok = (3..=100).contains(&name.len())
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "`{name}` is not a package name like Clipchamp.Clipchamp"
        ))
    }
}

/// Cleanup deletes files and has no undo: it cannot share a recipe with undoable actions,
/// and is never ticked by a "Recommended" button.
fn check_cleanup(raw: &RawRecipe) -> Result<(), Problem> {
    let cleanups = raw
        .actions
        .iter()
        .filter(|a| matches!(a, RawAction::Cleanup(_)))
        .count();
    if cleanups == 0 {
        return Ok(());
    }
    if cleanups != raw.actions.len() {
        return Err((
            "actions".into(),
            "cleanup actions cannot be mixed with other actions (they cannot be undone)".into(),
        ));
    }
    if !raw.profiles.is_empty() {
        return Err((
            "profiles".into(),
            "a cleanup cannot be in a profile: it always asks first".into(),
        ));
    }
    Ok(())
}

fn convert_registry(i: usize, r: RawRegistryAction) -> Result<RegistryAction, Problem> {
    let field = |name: &str| format!("actions[{i}].{name}");

    check_key(&r.key).map_err(|m| (field("key"), m))?;
    if r.value.trim().is_empty() || r.value.trim() != r.value {
        return Err((
            field("value"),
            "value name must not be empty or start/end with spaces".into(),
        ));
    }

    let op = match (r.kind, r.set, r.delete) {
        (Some(kind), Some(set), None) => {
            RegistryOp::Set(convert_value(kind, set).map_err(|m| (field("set"), m))?)
        }
        (None, None, Some(true)) => RegistryOp::Delete,
        (_, _, Some(false)) => {
            return Err((
                field("delete"),
                "must be `true` when present; leave it out to set a value".into(),
            ))
        }
        (_, _, Some(true)) => {
            return Err((
                field("delete"),
                "`delete: true` cannot be combined with `kind` or `set`".into(),
            ))
        }
        (None, Some(_), None) => {
            return Err((
                field("kind"),
                "required when `set` is present (dword, qword or string)".into(),
            ))
        }
        (Some(_), None, None) => {
            return Err((field("set"), "required when `kind` is present".into()))
        }
        (None, None, None) => {
            return Err((
                field("set"),
                "a registry action needs `kind` + `set`, or `delete: true`".into(),
            ))
        }
    };

    let only_if = match (r.only_if, &op) {
        (None, _) => None,
        (Some(_), RegistryOp::Delete) => {
            return Err((
                field("only_if"),
                "`only_if` can only be used together with `set`".into(),
            ))
        }
        (Some(expected), RegistryOp::Set(target)) => {
            let expected =
                convert_value(target.kind(), expected).map_err(|m| (field("only_if"), m))?;
            if &expected == target {
                return Err((
                    field("only_if"),
                    "`only_if` equals `set`, so it checks nothing".into(),
                ));
            }
            Some(expected)
        }
    };

    Ok(RegistryAction {
        hive: r.hive,
        key: r.key,
        value: r.value,
        op,
        only_if,
    })
}

fn convert_value(kind: RegKind, value: Value) -> Result<RegValue, String> {
    match (kind, value) {
        (RegKind::Dword, Value::Number(n)) => n
            .as_u64()
            .and_then(|v| u32::try_from(v).ok())
            .map(RegValue::Dword)
            .ok_or_else(|| "a dword must be a whole number from 0 to 4294967295".into()),
        (RegKind::Qword, Value::Number(n)) => n
            .as_u64()
            .map(RegValue::Qword)
            .ok_or_else(|| "a qword must be a whole number from 0 to 18446744073709551615".into()),
        (RegKind::String, Value::String(s)) => Ok(RegValue::String(s)),
        (RegKind::String, _) => {
            Err("kind is `string`, so the value must be quoted text, e.g. \"506\"".into())
        }
        (RegKind::Dword | RegKind::Qword, Value::String(_)) => {
            Err("kind is a number type, so the value must be a number without quotes".into())
        }
        (RegKind::Dword | RegKind::Qword, _) => Err("the value must be a whole number".into()),
        (RegKind::Binary, _) => Err("binary values cannot be written by a recipe".into()),
    }
}

fn check_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() || key.trim() != key {
        return Err("key must not be empty or start/end with spaces".into());
    }
    if key.starts_with('\\') || key.ends_with('\\') || key.contains("\\\\") {
        return Err("key must not start/end with `\\` or contain `\\\\`".into());
    }
    let upper = key.to_ascii_uppercase();
    if ["HKEY_", "HKCU\\", "HKLM\\", "HKCU:", "HKLM:"]
        .iter()
        .any(|p| upper.starts_with(p))
    {
        return Err("write the hive in `hive`, not at the start of `key`".into());
    }
    Ok(())
}

fn check_service(i: usize, s: &ServiceAction) -> Result<(), Problem> {
    if s.name.trim().is_empty() || s.name.trim() != s.name || s.name.contains(['\\', '/']) {
        return Err((
            format!("actions[{i}].name"),
            "service name must be the short service name, e.g. `Fax`".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        for ok in ["gaming.game-dvr-off", "a.b", "x1.y-2.z"] {
            assert!(check_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", "gaming", "Gaming.x", "gaming.", ".x", "a..b", "a-b.c", "a.b_c", "a.B",
        ] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn values() {
        let n = |v: u64| Value::Number(v.into());
        assert_eq!(convert_value(RegKind::Dword, n(0)), Ok(RegValue::Dword(0)));
        assert_eq!(
            convert_value(RegKind::Dword, n(u64::from(u32::MAX))),
            Ok(RegValue::Dword(u32::MAX))
        );
        assert!(convert_value(RegKind::Dword, n(u64::from(u32::MAX) + 1)).is_err());
        assert_eq!(
            convert_value(RegKind::Qword, n(u64::MAX)),
            Ok(RegValue::Qword(u64::MAX))
        );
        assert!(convert_value(RegKind::Dword, Value::Number((-1i64).into())).is_err());
        assert!(convert_value(RegKind::Dword, Value::String("1".into())).is_err());
        assert!(convert_value(RegKind::String, n(1)).is_err());
        assert!(convert_value(RegKind::Dword, Value::Bool(true)).is_err());
    }

    #[test]
    fn keys() {
        assert!(check_key(r"Software\Microsoft\GameBar").is_ok());
        for bad in [
            "",
            " Software",
            r"\Software",
            r"Software\",
            r"Software\\X",
            r"HKCU\Software",
            r"HKEY_CURRENT_USER\Software",
            r"hklm\Software",
        ] {
            assert!(check_key(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn guids_and_packages() {
        assert_eq!(
            guid("{8C5E7FDA-E8BF-4A96-9A85-A6E23A8C635C}").as_deref(),
            Ok("8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c")
        );
        for bad in [
            "",
            "high-performance",
            "8c5e7fda-e8bf-4a96-9a85",
            "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635z",
        ] {
            assert!(guid(bad).is_err(), "{bad}");
        }
        assert!(check_package("Clipchamp.Clipchamp").is_ok());
        assert!(check_package("Microsoft.BingNews").is_ok());
        for bad in ["", "ab", "a b", "x;Remove-Item", "-x.y", "a'b", "a$b"] {
            assert!(check_package(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn urls() {
        assert!(check_url("https://learn.microsoft.com/x").is_ok());
        for bad in [
            "",
            "https://",
            "learn.microsoft.com",
            "https://a b",
            "ftp://x",
        ] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }
}
