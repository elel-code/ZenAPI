use anyhow::anyhow;
use slint::{ModelRc, VecModel};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use crate::ui::{AppWindow, EnvironmentRow};

use super::super::variable_ui::variable_ui_entries;
use super::profiles::{EnvironmentProfiles, EnvironmentWorkspace, save_environment_workspace};

pub(in crate::app) const ENVIRONMENT_FILE_NAME: &str = ".zenapi-environments.json";

pub(in crate::app) fn apply_environment_workspace(
    app: &AppWindow,
    workspace: &EnvironmentWorkspace,
) {
    app.set_global_variables(workspace.global_variables.clone().into());
    app.set_environment_name(workspace.active_name.clone().into());
    app.set_environment_draft_name(workspace.active_name.clone().into());
    app.set_environment_variables(
        workspace
            .values_by_name
            .get(workspace.active_name.trim())
            .cloned()
            .unwrap_or_default()
            .into(),
    );
}

pub(in crate::app::environment_ui) fn persist_environment_workspace(
    app: &AppWindow,
    profiles: &Arc<Mutex<EnvironmentProfiles>>,
    path: &Path,
) {
    let result = profiles
        .lock()
        .map_err(|_| anyhow!("environment profile state is unavailable"))
        .and_then(|mut profiles| {
            profiles.set_active_name(app.get_environment_name().as_str());
            profiles.save_active(app.get_environment_variables().as_str());
            let workspace =
                EnvironmentWorkspace::from_profiles(app.get_global_variables().as_str(), &profiles);
            save_environment_workspace(path, &workspace)
        });

    if let Err(error) = result {
        app.set_activity(format!("Environment save failed: {error}").into());
    }
}

pub(in crate::app::environment_ui) fn refresh_environment_rows(
    app: &AppWindow,
    profiles: &Arc<Mutex<EnvironmentProfiles>>,
) {
    if let Ok(profiles) = profiles.lock() {
        app.set_environment_rows(environment_rows_model(&profiles));
    }
}

pub(in crate::app) fn environment_rows_model(
    profiles: &EnvironmentProfiles,
) -> ModelRc<EnvironmentRow> {
    let names = profiles.ordered_names();
    ModelRc::new(VecModel::from_iter(names.into_iter().map(|name| {
        let values = profiles
            .values_by_name
            .get(&name)
            .map_or("", String::as_str);
        let variable_count = variable_ui_entries(values).len();
        EnvironmentRow {
            label: environment_display_label(&name).into(),
            detail: format!("{variable_count} env variable(s)").into(),
            tone: environment_tone(&name).into(),
            name: name.into(),
        }
    })))
}

fn environment_display_label(name: &str) -> String {
    match name.trim() {
        "dev" => "Development".to_string(),
        "test" => "Staging".to_string(),
        "prod" => "Production".to_string(),
        "local" => "Local".to_string(),
        other if other.is_empty() => "No Environment".to_string(),
        other => other.to_string(),
    }
}

fn environment_tone(name: &str) -> &'static str {
    if name.trim() == "prod" {
        "error"
    } else {
        "inactive"
    }
}
