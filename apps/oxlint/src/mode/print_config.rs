use oxc_linter::{ConfigStoreBuilder, ExternalPluginStore, Oxlintrc};
use serde_json::{Value, json};

use crate::{cli::CliRunResult, lint::print_and_flush_stdout};

pub fn run_print_config(
    store_builder: &ConfigStoreBuilder,
    external_plugin_store: &ExternalPluginStore,
    oxlintrc: Oxlintrc,
    stdout: &mut dyn std::io::Write,
) -> CliRunResult {
    let config_file = store_builder.resolve_final_config_file(oxlintrc);
    let mut config_json: Value = serde_json::from_str(&config_file).unwrap();

    if let Some(config_object) = config_json.as_object_mut() {
        let plugins_value = config_object.entry("plugins").or_insert_with(|| json!([]));
        if let Some(plugins) = plugins_value.as_array_mut() {
            let mut plugin_names = external_plugin_store.plugin_names().collect::<Vec<_>>();
            plugin_names.sort_unstable();

            for plugin_name in plugin_names {
                if plugins.iter().any(|entry| entry.as_str() == Some(plugin_name)) {
                    continue;
                }
                plugins.push(Value::String(plugin_name.to_string()));
            }
        }
    }

    print_and_flush_stdout(stdout, &serde_json::to_string_pretty(&config_json).unwrap());
    print_and_flush_stdout(stdout, "\n");

    CliRunResult::PrintConfigResult
}
