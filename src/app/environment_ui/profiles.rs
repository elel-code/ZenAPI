use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(in crate::app) struct EnvironmentProfiles {
    pub(in crate::app) active_name: String,
    pub(in crate::app) values_by_name: BTreeMap<String, String>,
    pub(in crate::app) order: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(in crate::app) struct EnvironmentWorkspace {
    pub(in crate::app) active_name: String,
    pub(in crate::app) global_variables: String,
    pub(in crate::app) values_by_name: BTreeMap<String, String>,
    pub(in crate::app) order: Vec<String>,
}

impl EnvironmentProfiles {
    pub(in crate::app) fn new(active_name: &str, active_values: &str) -> Self {
        let mut profiles = Self {
            active_name: active_name.trim().to_string(),
            values_by_name: BTreeMap::new(),
            order: Vec::new(),
        };
        profiles.save_active(active_values);
        profiles
    }

    pub(in crate::app) fn from_workspace(
        workspace: Option<EnvironmentWorkspace>,
        fallback_name: &str,
        fallback_values: &str,
    ) -> Self {
        let Some(workspace) = workspace else {
            return Self::new(fallback_name, fallback_values);
        };

        let mut profiles = Self {
            active_name: workspace.active_name.trim().to_string(),
            values_by_name: workspace.values_by_name,
            order: workspace.order,
        };
        if profiles.active_name.is_empty() {
            profiles.active_name = fallback_name.trim().to_string();
        }
        if !profiles.active_name.is_empty()
            && !profiles.values_by_name.contains_key(&profiles.active_name)
        {
            profiles
                .values_by_name
                .insert(profiles.active_name.clone(), fallback_values.to_string());
        }
        profiles.normalize_order();
        profiles
    }

    pub(in crate::app) fn switch_to(&mut self, next_name: &str, current_values: &str) -> String {
        self.save_active(current_values);
        self.active_name = next_name.trim().to_string();
        self.ensure_ordered_name(&self.active_name.clone());
        self.values_by_name
            .get(&self.active_name)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::app) fn save_active(&mut self, values: &str) {
        let active_name = self.active_name.trim();
        if !active_name.is_empty() {
            let active_name = active_name.to_string();
            self.values_by_name
                .insert(active_name.clone(), values.to_string());
            self.ensure_ordered_name(&active_name);
        }
    }

    pub(in crate::app) fn set_active_name(&mut self, active_name: &str) {
        self.active_name = active_name.trim().to_string();
    }

    pub(in crate::app) fn delete(
        &mut self,
        name: &str,
        current_values: &str,
    ) -> Option<(String, String)> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }

        self.save_active(current_values);
        self.values_by_name.remove(name)?;
        self.order.retain(|ordered_name| ordered_name != name);

        if self.active_name == name {
            self.active_name = self.ordered_names().into_iter().next().unwrap_or_default();
        }

        let values = self
            .values_by_name
            .get(&self.active_name)
            .cloned()
            .unwrap_or_default();
        Some((self.active_name.clone(), values))
    }

    pub(in crate::app) fn rename(
        &mut self,
        old_name: &str,
        new_name: &str,
        current_values: &str,
    ) -> Option<(String, String)> {
        let old_name = old_name.trim();
        let new_name = new_name.trim();
        if old_name.is_empty() || new_name.is_empty() || old_name == new_name {
            return None;
        }
        if self.values_by_name.contains_key(new_name) {
            return None;
        }

        self.save_active(current_values);
        let values = self.values_by_name.remove(old_name)?;
        self.values_by_name
            .insert(new_name.to_string(), values.clone());
        if let Some(index) = self.order.iter().position(|name| name == old_name) {
            self.order[index] = new_name.to_string();
        } else {
            self.ensure_ordered_name(new_name);
        }
        if self.active_name == old_name {
            self.active_name = new_name.to_string();
        }

        Some((new_name.to_string(), values))
    }

    pub(in crate::app) fn move_profile(
        &mut self,
        name: &str,
        delta: i32,
        current_values: &str,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() || delta == 0 {
            return false;
        }

        self.save_active(current_values);
        self.normalize_order();
        let Some(index) = self
            .order
            .iter()
            .position(|ordered_name| ordered_name == name)
        else {
            return false;
        };
        let next_index = index as i32 + delta;
        if next_index < 0 || next_index >= self.order.len() as i32 {
            return false;
        }
        self.order.swap(index, next_index as usize);
        true
    }

    pub(super) fn ordered_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for name in &self.order {
            if self.values_by_name.contains_key(name) && !names.iter().any(|saved| saved == name) {
                names.push(name.clone());
            }
        }
        for name in self.values_by_name.keys() {
            if !names.iter().any(|saved| saved == name) {
                names.push(name.clone());
            }
        }
        if !self.active_name.trim().is_empty()
            && !names.iter().any(|saved| saved == &self.active_name)
        {
            names.push(self.active_name.clone());
        }
        names
    }

    fn normalize_order(&mut self) {
        self.order = self.ordered_names();
    }

    fn ensure_ordered_name(&mut self, name: &str) {
        let name = name.trim();
        if !name.is_empty() && !self.order.iter().any(|ordered_name| ordered_name == name) {
            self.order.push(name.to_string());
        }
    }
}

impl EnvironmentWorkspace {
    pub(in crate::app) fn from_profiles(
        global_variables: &str,
        profiles: &EnvironmentProfiles,
    ) -> Self {
        Self {
            active_name: profiles.active_name.trim().to_string(),
            global_variables: global_variables.to_string(),
            values_by_name: profiles.values_by_name.clone(),
            order: profiles.ordered_names(),
        }
    }
}

pub(in crate::app) fn load_environment_workspace(
    path: &Path,
) -> Result<Option<EnvironmentWorkspace>> {
    if !path.exists() {
        return Ok(None);
    }

    let body = fs::read_to_string(path)
        .map_err(|error| anyhow!("read environment file {}: {error}", path.display()))?;
    let workspace = serde_json::from_str(&body)
        .map_err(|error| anyhow!("parse environment file {}: {error}", path.display()))?;
    Ok(Some(workspace))
}

pub(in crate::app) fn save_environment_workspace(
    path: &Path,
    workspace: &EnvironmentWorkspace,
) -> Result<()> {
    let body = serde_json::to_string_pretty(workspace)
        .map_err(|error| anyhow!("serialize environment workspace: {error}"))?;
    fs::write(path, body)
        .map_err(|error| anyhow!("write environment file {}: {error}", path.display()))
}
