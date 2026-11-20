//! Turns a `RawRecipe` (as written) into a validated `Recipe`.

use std::collections::HashSet;
use std::path::Path;

use serde_yaml_ng::Value;

use super::error::RecipeError;
use super::model::{
    Action, I18n, RawAction, RawRecipe, RawRegistryAction, Recipe, RegKind, RegValue,
    RegistryAction, RegistryOp, ServiceAction,
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
    match raw {
        RawAction::Registry(r) => convert_registry(i, r).map(Action::Registry),
        RawAction::Service(s) => check_service(i, &s).map(|()| Action::Service(s)),
    }
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

    Ok(RegistryAction {
        hive: r.hive,
        key: r.key,
        value: r.value,
        op,
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
