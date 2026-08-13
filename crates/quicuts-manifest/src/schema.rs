//! Raw serde mirror of the PTSG YAML manifest schema, deliberately tolerant:
//! every field optional where real-world files omit it, booleans accept
//! `true`/`True`/strings, `Keys` items may be ints or strings.

use serde::{Deserialize, Deserializer};

/// Bool that tolerates YAML bools, "true"/"True"/"false"/"False" strings,
/// integers, and null. Missing fields use `Default` (false).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LaxBool(pub bool);

impl<'de> Deserialize<'de> for LaxBool {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = LaxBool;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a boolean-ish value")
            }
            fn visit_bool<E>(self, v: bool) -> Result<LaxBool, E> {
                Ok(LaxBool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<LaxBool, E> {
                Ok(LaxBool(v != 0))
            }
            fn visit_u64<E>(self, v: u64) -> Result<LaxBool, E> {
                Ok(LaxBool(v != 0))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<LaxBool, E> {
                match v.trim().to_ascii_lowercase().as_str() {
                    "true" | "yes" | "on" | "1" => Ok(LaxBool(true)),
                    _ => Ok(LaxBool(false)),
                }
            }
            fn visit_unit<E>(self) -> Result<LaxBool, E> {
                Ok(LaxBool(false))
            }
            fn visit_none<E>(self) -> Result<LaxBool, E> {
                Ok(LaxBool(false))
            }
            fn visit_some<D: Deserializer<'de>>(self, de: D) -> Result<LaxBool, D::Error> {
                de.deserialize_any(V)
            }
        }
        de.deserialize_any(V)
    }
}

/// String-or-list field: `TitleMatch: "- Gmail"` and
/// `TitleMatch: ["- Gmail", "– Gmail"]` both parse. Null/missing = empty.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct LaxStringList(pub Vec<String>);

impl<'de> Deserialize<'de> for LaxStringList {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = LaxStringList;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string or a list of strings")
            }
            fn visit_str<E>(self, v: &str) -> Result<LaxStringList, E> {
                Ok(LaxStringList(vec![v.to_string()]))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<LaxStringList, A::Error> {
                let mut out = Vec::new();
                while let Some(item) = seq.next_element::<String>()? {
                    out.push(item);
                }
                Ok(LaxStringList(out))
            }
            fn visit_unit<E>(self) -> Result<LaxStringList, E> {
                Ok(LaxStringList(Vec::new()))
            }
            fn visit_none<E>(self) -> Result<LaxStringList, E> {
                Ok(LaxStringList(Vec::new()))
            }
            fn visit_some<D: Deserializer<'de>>(self, de: D) -> Result<LaxStringList, D::Error> {
                de.deserialize_any(V)
            }
        }
        de.deserialize_any(V)
    }
}

/// A `Keys` list item: YAML may give an unquoted int (`91`) or a string
/// (`"A"`, `"91"`, `"<Enter>"`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum RawKeyToken {
    Num(u64),
    Str(String),
    /// A bare `~` in YAML parses as null (Adobe.Illustrator writes the tilde
    /// key that way); treated as the literal "~" key downstream.
    Null,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct RawKeyCombo {
    pub win: LaxBool,
    pub ctrl: LaxBool,
    pub shift: LaxBool,
    pub alt: LaxBool,
    pub keys: Option<Vec<RawKeyToken>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawEntry {
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub recommended: LaxBool,
    #[serde(default)]
    pub shortcut: Option<Vec<RawKeyCombo>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawCategory {
    pub section_name: Option<String>,
    #[serde(default)]
    pub properties: Option<Vec<RawEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawManifest {
    #[serde(default)]
    pub package_name: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub window_filter: Option<String>,
    #[serde(default)]
    pub background_process: LaxBool,
    /// Quicuts extension (ADR 0003): host class ("browser") for hosted
    /// collections — apps living inside a host app instead of their own exe.
    #[serde(default)]
    pub host: Option<String>,
    /// Quicuts extension: case-insensitive substring(s) matched against the
    /// foreground window title to auto-detect the hosted app (experimental).
    /// A single string or a list of strings.
    #[serde(default)]
    pub title_match: LaxStringList,
    /// Quicuts extension: image file next to the manifest for the rail icon.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub shortcuts: Option<Vec<RawCategory>>,
}
