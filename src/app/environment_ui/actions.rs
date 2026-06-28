use slint::ComponentHandle;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::ui::AppWindow;

use super::super::{
    request_editor_ui::refresh_variable_table,
    variable_ui::{add_variable_text, delete_variable_text, is_global_scope, update_variable_text},
};
use super::{
    profiles::{EnvironmentProfiles, EnvironmentWorkspace},
    rows::{ENVIRONMENT_FILE_NAME, persist_environment_workspace, refresh_environment_rows},
};

pub(in crate::app) fn wire_environment_actions(
    app: &AppWindow,
    initial_workspace: Option<EnvironmentWorkspace>,
) {
    let environment_profiles = Arc::new(Mutex::new(EnvironmentProfiles::from_workspace(
        initial_workspace,
        app.get_environment_name().as_str(),
        app.get_environment_variables().as_str(),
    )));
    let environment_path = Arc::new(PathBuf::from(ENVIRONMENT_FILE_NAME));
    refresh_environment_rows(app, &environment_profiles);

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_select_environment(move |environment| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let variables = profiles
            .lock()
            .map(|mut profiles| {
                profiles.switch_to(
                    environment.as_str(),
                    app.get_environment_variables().as_str(),
                )
            })
            .unwrap_or_default();
        app.set_environment_name(environment);
        app.set_environment_draft_name(app.get_environment_name());
        app.set_environment_variables(variables.into());
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_environment_name_changed(move |environment| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let variables = profiles
            .lock()
            .map(|mut profiles| {
                profiles.switch_to(
                    environment.as_str(),
                    app.get_environment_variables().as_str(),
                )
            })
            .unwrap_or_default();
        app.set_environment_name(environment);
        app.set_environment_draft_name(app.get_environment_name());
        app.set_environment_variables(variables.into());
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_add_environment(move |environment| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let environment = environment.trim().to_string();
        if environment.is_empty() {
            app.set_activity("Enter an environment name before adding it.".into());
            return;
        }

        let variables = profiles
            .lock()
            .map(|mut profiles| {
                profiles.switch_to(&environment, app.get_environment_variables().as_str())
            })
            .unwrap_or_default();
        app.set_environment_name(environment.into());
        app.set_environment_draft_name(app.get_environment_name());
        app.set_environment_variables(variables.into());
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_rename_environment(move |old_name, new_name| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some((renamed_name, values)) = profiles.lock().ok().and_then(|mut profiles| {
            profiles.rename(
                old_name.as_str(),
                new_name.as_str(),
                app.get_environment_variables().as_str(),
            )
        }) else {
            app.set_activity(
                "Rename requires a selected environment and a new unused name.".into(),
            );
            return;
        };

        app.set_environment_name(renamed_name.into());
        app.set_environment_draft_name(app.get_environment_name());
        app.set_environment_variables(values.into());
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_move_environment(move |environment, delta| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let moved = profiles
            .lock()
            .map(|mut profiles| {
                profiles.move_profile(
                    environment.as_str(),
                    delta,
                    app.get_environment_variables().as_str(),
                )
            })
            .unwrap_or(false);
        if !moved {
            app.set_activity("Environment cannot move further in that direction.".into());
            return;
        }

        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_delete_environment(move |environment| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        let Some((next_name, next_values)) = profiles.lock().ok().and_then(|mut profiles| {
            profiles.delete(
                environment.as_str(),
                app.get_environment_variables().as_str(),
            )
        }) else {
            app.set_activity("Select a saved environment before deleting it.".into());
            return;
        };

        app.set_environment_name(next_name.into());
        app.set_environment_draft_name(app.get_environment_name());
        app.set_environment_variables(next_values.into());
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_update_variable_row(move |row_id, scope, name, value| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if is_global_scope(scope.as_str()) {
            let updated = update_variable_text(
                app.get_global_variables().as_str(),
                row_id,
                name.as_str(),
                value.as_str(),
            );
            app.set_global_variables(updated.into());
        } else {
            let updated = update_variable_text(
                app.get_environment_variables().as_str(),
                row_id,
                name.as_str(),
                value.as_str(),
            );
            app.set_environment_variables(updated.into());
        }
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles.clone();
    let path = environment_path.clone();
    app.on_add_variable(move |scope| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if is_global_scope(scope.as_str()) {
            let updated = add_variable_text(app.get_global_variables().as_str(), "GLOBAL_VAR");
            app.set_global_variables(updated.into());
        } else {
            if app.get_environment_name().trim().is_empty() {
                app.set_environment_name("dev".into());
                app.set_environment_draft_name("dev".into());
                if let Ok(mut profiles) = profiles.lock() {
                    profiles.set_active_name("dev");
                }
            }
            let updated = add_variable_text(app.get_environment_variables().as_str(), "ENV_VAR");
            app.set_environment_variables(updated.into());
        }
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });

    let weak_app = app.as_weak();
    let profiles = environment_profiles;
    let path = environment_path;
    app.on_delete_variable_row(move |row_id, scope| {
        let Some(app) = weak_app.upgrade() else {
            return;
        };
        if app.get_busy() {
            return;
        }

        if is_global_scope(scope.as_str()) {
            let updated = delete_variable_text(app.get_global_variables().as_str(), row_id);
            app.set_global_variables(updated.into());
        } else {
            let updated = delete_variable_text(app.get_environment_variables().as_str(), row_id);
            app.set_environment_variables(updated.into());
        }
        refresh_variable_table(&app);
        persist_environment_workspace(&app, &profiles, path.as_path());
        refresh_environment_rows(&app, &profiles);
    });
}
