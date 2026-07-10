use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
};

use rustc_hash::FxHashSet;
use schemars::{JsonSchema, r#gen, schema::Schema};
use serde::{Deserialize, Deserializer, Serialize};

use oxc_config::GlobSet;

use crate::{LintPlugins, OxlintEnv, OxlintGlobals, config::OxlintRules};

use super::{
    external_plugins::{ExternalPluginEntry, external_plugins_schema},
    settings::OxlintSettings,
};

// nominal wrapper required to add JsonSchema impl
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct OxlintOverrides(Vec<OxlintOverride>);

impl Deref for OxlintOverrides {
    type Target = Vec<OxlintOverride>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OxlintOverrides {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for OxlintOverrides {
    type Item = OxlintOverride;
    type IntoIter = <Vec<OxlintOverride> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a OxlintOverrides {
    type Item = &'a OxlintOverride;
    type IntoIter = std::slice::Iter<'a, OxlintOverride>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl OxlintOverrides {
    #[inline]
    pub fn empty() -> Self {
        Self(Vec::new())
    }

    // must be explicitly defined to make serde happy
    /// Returns `true` if the overrides list has no elements.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl JsonSchema for OxlintOverrides {
    fn schema_name() -> String {
        "OxlintOverrides".to_owned()
    }

    fn schema_id() -> Cow<'static, str> {
        Cow::Borrowed("OxlintOverrides")
    }

    fn json_schema(r#gen: &mut r#gen::SchemaGenerator) -> Schema {
        r#gen.subschema_for::<Vec<OxlintOverride>>()
    }
}

#[derive(Debug, Default, Clone, Serialize, JsonSchema)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct OxlintOverride {
    /// A list of glob patterns to override.
    ///
    /// ## Example
    /// `[ "*.test.ts", "*.spec.ts" ]`
    pub files: GlobSet,

    /// A list of glob patterns to exclude from this override.
    ///
    /// Files matching these patterns are not globally ignored; this override
    /// simply does not apply to them.
    ///
    /// ## Example
    /// `[ "*.generated.ts", "fixtures/**" ]`
    #[serde(default, skip_serializing_if = "GlobSet::is_empty")]
    pub exclude_files: GlobSet,

    /// Environments enable and disable collections of global variables.
    pub env: Option<OxlintEnv>,

    /// Enabled or disabled specific global variables.
    pub globals: Option<OxlintGlobals>,

    /// Plugin-specific configuration for both built-in and custom plugins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<OxlintSettings>,

    /// Optionally change what plugins are enabled for this override. When
    /// omitted, the base config's plugins are used.
    #[serde(default)]
    pub plugins: Option<LintPlugins>,

    /// JS plugins for this override, allows usage of ESLint plugins with Oxlint.
    ///
    /// Read more about JS plugins in
    /// [the docs](https://oxc.rs/docs/guide/usage/linter/js-plugins.html).
    ///
    /// Note: JS plugins are in alpha and not subject to semver.
    #[serde(rename = "jsPlugins", default, skip_serializing_if = "Option::is_none")]
    #[schemars(schema_with = "external_plugins_schema")]
    pub external_plugins: Option<FxHashSet<ExternalPluginEntry>>,

    /// Internal ID for `languageOptions` loaded from `oxlint.config.ts`.
    #[serde(rename = "_languageOptionsId", default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub language_options_id: Option<u32>,

    /// Internal parser-presence flag for `languageOptions` loaded from `oxlint.config.ts`.
    #[serde(
        rename = "_languageOptionsHasParser",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(skip)]
    pub language_options_has_parser: Option<bool>,

    #[serde(default)]
    pub rules: OxlintRules,
}

#[derive(Debug, Default, Deserialize)]
#[expect(dead_code)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PublicOxlintOverride {
    pub files: GlobSet,
    #[serde(default)]
    pub exclude_files: GlobSet,
    pub env: Option<OxlintEnv>,
    pub globals: Option<OxlintGlobals>,
    #[serde(default)]
    pub settings: Option<OxlintSettings>,
    #[serde(default)]
    pub plugins: Option<LintPlugins>,
    #[serde(rename = "jsPlugins", default)]
    pub external_plugins: Option<FxHashSet<ExternalPluginEntry>>,
    #[serde(default)]
    pub rules: OxlintRules,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InternalOxlintOverride {
    pub files: GlobSet,
    #[serde(default)]
    pub exclude_files: GlobSet,
    pub env: Option<OxlintEnv>,
    pub globals: Option<OxlintGlobals>,
    #[serde(default)]
    pub settings: Option<OxlintSettings>,
    #[serde(default)]
    pub plugins: Option<LintPlugins>,
    #[serde(rename = "jsPlugins", default)]
    pub external_plugins: Option<FxHashSet<ExternalPluginEntry>>,
    #[serde(rename = "_languageOptionsId", default)]
    pub language_options_id: Option<u32>,
    #[serde(rename = "_languageOptionsHasParser", default)]
    pub language_options_has_parser: Option<bool>,
    #[serde(default)]
    pub rules: OxlintRules,
}

fn strip_internal_language_options_fields(value: &mut serde_json::Value) {
    if let serde_json::Value::Object(object) = value {
        object.remove("_languageOptionsId");
        object.remove("_languageOptionsHasParser");
    }
}

impl<'de> Deserialize<'de> for OxlintOverride {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw_value = serde_json::Value::deserialize(deserializer)?;
        let mut public_value = raw_value.clone();
        strip_internal_language_options_fields(&mut public_value);

        let _: PublicOxlintOverride =
            serde_json::from_value(public_value).map_err(serde::de::Error::custom)?;
        let raw: InternalOxlintOverride =
            serde_json::from_value(raw_value).map_err(serde::de::Error::custom)?;

        Ok(Self {
            files: raw.files,
            exclude_files: raw.exclude_files,
            env: raw.env,
            globals: raw.globals,
            settings: raw.settings,
            plugins: raw.plugins,
            external_plugins: raw.external_plugins,
            language_options_id: raw.language_options_id,
            language_options_has_parser: raw.language_options_has_parser,
            rules: raw.rules,
        })
    }
}
#[cfg(test)]
mod test {
    use crate::config::{globals::GlobalValue, plugins::LintPlugins};

    use super::*;
    use serde_json::{from_value, json};

    #[test]
    fn test_parsing_plugins() {
        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
        }))
        .unwrap();
        assert_eq!(config.plugins, None);

        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
            "plugins": [],
        }))
        .unwrap();
        assert_eq!(config.plugins, Some(LintPlugins::empty()));

        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
            "plugins": ["typescript", "react"],
        }))
        .unwrap();
        assert_eq!(config.plugins, Some(LintPlugins::REACT | LintPlugins::TYPESCRIPT));
    }

    #[test]
    fn test_parsing_exclude_files() {
        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
            "excludeFiles": ["*.generated.tsx"],
        }))
        .unwrap();

        assert!(config.exclude_files.is_match("App.generated.tsx"));
        assert!(!config.exclude_files.is_match("App.tsx"));
    }

    #[test]
    fn test_parsing_globals() {
        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
        }))
        .unwrap();
        assert!(config.globals.is_none());

        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
            "globals": {
                "Foo": "readable"
            },
        }))
        .unwrap();

        assert_eq!(*config.globals.unwrap().get("Foo").unwrap(), GlobalValue::Readonly);
    }

    #[test]
    fn test_parsing_env() {
        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
        }))
        .unwrap();
        assert!(config.env.is_none());

        let config: OxlintOverride = from_value(json!({
            "files": ["*.tsx"],
            "env": {
                "es2022": true,
                "es2023": false,
            },
        }))
        .unwrap();

        let env = &config.env.unwrap();
        assert!(env.contains("es2022"));
        assert!(!env.contains("es2023"));
    }
}
