//! User-facing texts (CLAUDE.md: "textele pentru utilizator în fișiere de traducere RO/EN"),
//! from `i18n/ro.yaml` and `i18n/en.yaml` at the repository root: one source for `pco` and
//! the app.
//!
//! A text may hold `{name}` placeholders and plural choices `{name|one|few|other}`: the form
//! is picked by the number given for `name`, following the language's rules (Romanian
//! "1 rulare", "3 rulări", "20 de rulări"; English "1 run", "3 runs"). English needs only
//! `{name|one|other}`.

use std::collections::HashMap;
use std::fmt::Display;

use crate::recipe::Lang;

const RO: &str = include_str!("../../../i18n/ro.yaml");
const EN: &str = include_str!("../../../i18n/en.yaml");

/// The raw translation file for a language (tests compare the two).
pub fn source(lang: Lang) -> &'static str {
    match lang {
        Lang::Ro => RO,
        Lang::En => EN,
    }
}

pub struct Texts {
    lang: Lang,
    map: HashMap<String, String>,
}

/// CLDR plural category: 0 = one, 1 = few, 2 = other.
fn category(lang: Lang, n: i64) -> usize {
    let n = n.unsigned_abs();
    match lang {
        Lang::En => {
            if n == 1 {
                0
            } else {
                2
            }
        }
        // Romanian: one = 1; few = 0, or 2–19 after the hundreds (2, 19, 101, 119); other
        // = the rest ("20 de", "100 de").
        Lang::Ro => {
            if n == 1 {
                0
            } else if n == 0 || (1..=19).contains(&(n % 100)) {
                1
            } else {
                2
            }
        }
    }
}

/// Picks one of `forms` (one, few, other; or one, other) for `n`.
fn plural_form<'a>(lang: Lang, n: i64, forms: &[&'a str]) -> &'a str {
    let wanted = category(lang, n);
    let index = match (wanted, forms.len()) {
        (0, _) => 0,
        (_, 2) => 1,
        (c, len) => c.min(len - 1),
    };
    forms.get(index).copied().unwrap_or_default()
}

impl Texts {
    pub fn load(lang: Lang) -> Result<Self, String> {
        let map: HashMap<String, String> = serde_yaml_ng::from_str(source(lang))
            .map_err(|e| format!("translation file for {lang:?} is invalid: {e}"))?;
        Ok(Self { lang, map })
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// Every text whose key starts with `prefix` (the app takes its own `app.` texts).
    pub fn with_prefix(&self, prefix: &str) -> HashMap<String, String> {
        self.map
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The text for `key`; the key itself if it is missing (tests make sure none are).
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.map.get(key).map_or(key, String::as_str)
    }

    /// Like [`Texts::t`], but as a `&String` usable as a message argument.
    pub fn text(&self, key: &str) -> &String {
        static MISSING: String = String::new();
        self.map.get(key).unwrap_or(&MISSING)
    }

    /// The text for `key` with `{name}` placeholders and `{name|…}` plural choices filled in.
    pub fn f(&self, key: &str, args: &[(&str, &dyn Display)]) -> String {
        format_text(self.lang, self.t(key), args)
    }
}

/// Fills in a text (see the module docs). Unknown placeholders stay as they are.
pub fn format_text(lang: Lang, text: &str, args: &[(&str, &dyn Display)]) -> String {
    let values: Vec<(&str, String)> = args.iter().map(|(k, v)| (*k, v.to_string())).collect();
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &after[..end];
        let mut parts = inner.split('|');
        let name = parts.next().unwrap_or_default();
        let forms: Vec<&str> = parts.collect();
        match values.iter().find(|(k, _)| *k == name) {
            Some((_, value)) if forms.is_empty() => out.push_str(value),
            Some((_, value)) => {
                let n = value.trim().parse::<i64>().unwrap_or(0);
                out.push_str(plural_form(lang, n, &forms));
            }
            None => {
                out.push('{');
                out.push_str(inner);
                out.push('}');
            }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// The Windows display language: Romanian if it is Romanian, English otherwise.
pub fn system_lang() -> Lang {
    #[cfg(windows)]
    {
        // SAFETY: no arguments, returns a LANGID.
        let langid = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        // Primary language id 0x18 is Romanian.
        if langid & 0x3ff == 0x18 {
            return Lang::Ro;
        }
        Lang::En
    }
    #[cfg(not(windows))]
    {
        match std::env::var("LANG") {
            Ok(v) if v.to_ascii_lowercase().starts_with("ro") => Lang::Ro,
            _ => Lang::En,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ro(n: i64) -> String {
        format_text(Lang::Ro, "{n} {n|rulare|rulări|de rulări}", &[("n", &n)])
    }

    #[test]
    fn romanian_plurals() {
        assert_eq!(ro(0), "0 rulări");
        assert_eq!(ro(1), "1 rulare");
        assert_eq!(ro(2), "2 rulări");
        assert_eq!(ro(19), "19 rulări");
        assert_eq!(ro(20), "20 de rulări");
        assert_eq!(ro(100), "100 de rulări");
        assert_eq!(ro(101), "101 rulări");
        assert_eq!(ro(119), "119 rulări");
        assert_eq!(ro(120), "120 de rulări");
    }

    #[test]
    fn english_plurals_and_placeholders() {
        let en = |n: i64| {
            format_text(
                Lang::En,
                "{n} {n|run|runs} for {game}",
                &[("n", &n), ("game", &"cs2")],
            )
        };
        assert_eq!(en(1), "1 run for cs2");
        assert_eq!(en(0), "0 runs for cs2");
        assert_eq!(en(3), "3 runs for cs2");
        // Unknown placeholders and stray braces are left alone.
        assert_eq!(format_text(Lang::En, "{x} {", &[]), "{x} {");
    }

    #[test]
    fn both_files_load() {
        for lang in [Lang::Ro, Lang::En] {
            assert!(Texts::load(lang).is_ok());
        }
    }
}
