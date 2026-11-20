//! User-facing texts (CLAUDE.md: "textele pentru utilizator în fișiere de traducere RO/EN").

use std::collections::HashMap;
use std::fmt::Display;

use pco_core::recipe::Lang;

const RO: &str = include_str!("../i18n/ro.yaml");
const EN: &str = include_str!("../i18n/en.yaml");

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

impl Texts {
    pub fn load(lang: Lang) -> Result<Self, String> {
        let map: HashMap<String, String> = serde_yaml_ng::from_str(source(lang))
            .map_err(|e| format!("translation file for {lang:?} is invalid: {e}"))?;
        Ok(Self { lang, map })
    }

    pub fn lang(&self) -> Lang {
        self.lang
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

    /// The text for `key` with `{name}` placeholders filled in.
    pub fn f(&self, key: &str, args: &[(&str, &dyn Display)]) -> String {
        let mut text = self.t(key).to_owned();
        for (name, value) in args {
            text = text.replace(&format!("{{{name}}}"), &value.to_string());
        }
        text
    }
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
