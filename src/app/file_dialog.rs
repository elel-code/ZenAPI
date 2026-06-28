use std::path::{Path, PathBuf};

pub(super) fn pick_file_path(
    title: &str,
    current_path: &str,
    filters: &[(&str, &[&str])],
) -> Option<String> {
    configure_file_dialog(title, current_path, filters)
        .pick_file()
        .map(dialog_path_string)
}

pub(super) fn pick_save_path(
    title: &str,
    current_path: &str,
    default_name: &str,
    filters: &[(&str, &[&str])],
) -> Option<String> {
    configure_file_dialog(title, current_path, filters)
        .set_file_name(default_name)
        .save_file()
        .map(dialog_path_string)
}

fn configure_file_dialog(
    title: &str,
    current_path: &str,
    filters: &[(&str, &[&str])],
) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    let current_path = current_path.trim();
    if !current_path.is_empty() {
        let path = Path::new(current_path);
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            dialog = dialog.set_directory(parent);
        }
    }
    for (name, extensions) in filters {
        dialog = dialog.add_filter(*name, extensions);
    }
    dialog
}

fn dialog_path_string(path: PathBuf) -> String {
    path.to_string_lossy().to_string()
}

pub(super) fn default_dialog_file_name(current_path: &str, fallback: &str) -> String {
    Path::new(current_path.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(fallback)
        .to_string()
}
