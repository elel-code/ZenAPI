use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub initial_value: String,
    pub current_value: String,
    pub scope: VariableScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableScope {
    Global,
    Environment { name: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableStore {
    variables: Vec<Variable>,
}

impl VariableStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn upsert(&mut self, variable: Variable) {
        if let Some(existing) = self
            .variables
            .iter_mut()
            .find(|existing| existing.name == variable.name && existing.scope == variable.scope)
        {
            *existing = variable;
        } else {
            self.variables.push(variable);
        }
    }

    pub fn remove(&mut self, name: &str, scope: &VariableScope) {
        let name = name.trim();
        self.variables
            .retain(|variable| !(variable.name == name && &variable.scope == scope));
    }

    pub fn resolve(&self, name: &str, active_environment: Option<&str>) -> Option<&str> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }

        active_environment
            .and_then(|environment| {
                self.variables.iter().find(|variable| {
                    variable.name == name
                        && matches!(
                            &variable.scope,
                            VariableScope::Environment { name } if name == environment
                        )
                })
            })
            .or_else(|| {
                self.variables.iter().find(|variable| {
                    variable.name == name && variable.scope == VariableScope::Global
                })
            })
            .map(Variable::effective_value)
    }

    pub fn variables(&self) -> &[Variable] {
        &self.variables
    }
}

impl Variable {
    pub fn global(name: impl Into<String>, initial_value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            initial_value: initial_value.into(),
            current_value: String::new(),
            scope: VariableScope::Global,
        }
    }

    pub fn environment(
        environment: impl Into<String>,
        name: impl Into<String>,
        initial_value: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            initial_value: initial_value.into(),
            current_value: String::new(),
            scope: VariableScope::Environment {
                name: environment.into(),
            },
        }
    }

    pub fn with_current_value(mut self, current_value: impl Into<String>) -> Self {
        self.current_value = current_value.into();
        self
    }

    fn effective_value(&self) -> &str {
        if self.current_value.is_empty() {
            &self.initial_value
        } else {
            &self.current_value
        }
    }
}

pub fn replace_variables(
    input: &str,
    store: &VariableStore,
    active_environment: Option<&str>,
) -> Result<String> {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}}") else {
            output.push_str(&rest[start..]);
            return Ok(output);
        };

        let name = after_open[..end].trim();
        let value = store
            .resolve(name, active_environment)
            .ok_or_else(|| anyhow!("unknown variable: {name}"))?;
        output.push_str(value);
        rest = &after_open[end + 2..];
    }

    output.push_str(rest);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_global_variables() {
        let mut store = VariableStore::new();
        store.upsert(Variable::global("baseUrl", "https://api.zenapi.local"));

        let rendered = replace_variables("{{baseUrl}}/users", &store, None).expect("render");

        assert_eq!(rendered, "https://api.zenapi.local/users");
    }

    #[test]
    fn environment_variables_override_globals() {
        let mut store = VariableStore::new();
        store.upsert(Variable::global("baseUrl", "https://prod.example.com"));
        store.upsert(Variable::environment(
            "dev",
            "baseUrl",
            "http://localhost:8080",
        ));

        let rendered =
            replace_variables("{{ baseUrl }}/health", &store, Some("dev")).expect("render");

        assert_eq!(rendered, "http://localhost:8080/health");
    }

    #[test]
    fn current_value_overrides_initial_value() {
        let mut store = VariableStore::new();
        store.upsert(Variable::global("token", "initial").with_current_value("current"));

        let rendered = replace_variables("Bearer {{token}}", &store, None).expect("render");

        assert_eq!(rendered, "Bearer current");
    }

    #[test]
    fn unknown_variables_return_errors() {
        let store = VariableStore::new();

        let error = replace_variables("{{missing}}", &store, None).expect_err("missing variable");

        assert_eq!(error.to_string(), "unknown variable: missing");
    }

    #[test]
    fn removes_variables_by_name_and_scope() {
        let mut store = VariableStore::new();
        store.upsert(Variable::global("token", "global"));
        store.upsert(Variable::environment("dev", "token", "dev"));

        store.remove(
            "token",
            &VariableScope::Environment {
                name: "dev".to_string(),
            },
        );

        assert_eq!(store.resolve("token", Some("dev")), Some("global"));

        store.remove("token", &VariableScope::Global);

        assert_eq!(store.resolve("token", Some("dev")), None);
    }

    #[test]
    fn unmatched_opening_delimiter_is_left_unchanged() {
        let store = VariableStore::new();

        let rendered = replace_variables("http://{{baseUrl", &store, None).expect("render");

        assert_eq!(rendered, "http://{{baseUrl");
    }
}
