use anyhow::{Result, anyhow, bail};
use std::{fs, path::Path};

pub(super) fn write_text_file(path: &str, contents: &str, label: &str) -> Result<()> {
    let path = path.trim();
    if path.is_empty() {
        bail!("{label} path is required");
    }

    let output_path = Path::new(path);
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|err| anyhow!("create {label} directory {}: {err}", parent.display()))?;
    }
    fs::write(output_path, contents)
        .map_err(|err| anyhow!("write {label} {}: {err}", output_path.display()))
}

pub(super) fn read_text_file(path: &str, label: &str) -> Result<String> {
    let path = path.trim();
    if path.is_empty() {
        bail!("{label} path is required");
    }

    let input_path = Path::new(path);
    fs::read_to_string(input_path)
        .map_err(|err| anyhow!("read {label} {}: {err}", input_path.display()))
}
