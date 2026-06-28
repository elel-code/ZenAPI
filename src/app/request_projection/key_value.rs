use anyhow::{Result, bail};

use super::super::collection_tree::format_header_lines;

pub(in crate::app) fn parse_key_value_lines(
    input: &str,
    field_name: &str,
) -> Result<Vec<(String, String)>> {
    input
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                None
            } else {
                Some((index + 1, line))
            }
        })
        .map(|(line_number, line)| {
            let Some((key, value)) = split_key_value_line(line) else {
                bail!("{field_name} line {line_number} must use key=value or key: value");
            };
            let key = key.trim();
            if key.is_empty() {
                bail!("{field_name} line {line_number} has an empty name");
            }
            Ok((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

pub(in crate::app) fn apply_header_preset(input: &str, preset: &str) -> Result<String> {
    let (name, value) = match preset {
        "accept-json" => ("Accept", "application/json"),
        "content-json" => ("Content-Type", "application/json"),
        "bearer-token" => ("Authorization", "Bearer {{token}}"),
        _ => bail!("unknown header preset: {preset}"),
    };
    let mut headers = parse_key_value_lines(input, "header")?;
    upsert_pair(&mut headers, name.to_string(), value.to_string(), true);
    Ok(format_header_lines(&headers))
}

pub(in crate::app) fn split_key_value_line(line: &str) -> Option<(&str, &str)> {
    let separator = match (line.find('='), line.find(':')) {
        (Some(eq), Some(colon)) => eq.min(colon),
        (Some(eq), None) => eq,
        (None, Some(colon)) => colon,
        (None, None) => return None,
    };

    Some((&line[..separator], &line[separator + 1..]))
}

pub(in crate::app) fn upsert_pair(
    pairs: &mut Vec<(String, String)>,
    name: String,
    value: String,
    case_insensitive: bool,
) {
    if let Some((_, existing_value)) = pairs.iter_mut().find(|(existing_name, _)| {
        if case_insensitive {
            existing_name.eq_ignore_ascii_case(&name)
        } else {
            existing_name == &name
        }
    }) {
        *existing_value = value;
    } else {
        pairs.push((name, value));
    }
}
