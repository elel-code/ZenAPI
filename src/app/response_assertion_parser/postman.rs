use anyhow::{Result, anyhow, bail};
use zenapi::assertions::{ResponseAssertion, ResponseAssertionKind};

use super::js::{
    call_argument_after, expect_equal_argument, expect_upper_bound_argument,
    expect_within_arguments, parse_js_string_literal, split_js_arguments, strip_js_string_value,
};
use super::{parse_response_size_bytes, parse_response_time_ms, parse_status};

mod json;

pub(super) use self::json::normalize_pm_json_type_name;
use self::json::parse_pm_json_assertion;

pub(super) fn parse_pm_test_assertion_line(line: &str) -> Result<Option<ResponseAssertion>> {
    let trimmed = line.trim();
    if !trimmed.starts_with("pm.test(") {
        return Ok(None);
    }

    let (name, body_start) = parse_pm_test_name(trimmed)?;
    let body = &trimmed[body_start..];
    let kind = if let Some(status) = parse_pm_status_assertion(body)? {
        status
    } else if let Some(metric) = parse_pm_metric_assertion(body)? {
        metric
    } else if let Some(header) = parse_pm_header_assertion(body) {
        header
    } else if let Some(body_contains) = parse_pm_body_assertion(body) {
        body_contains
    } else if let Some(json) = parse_pm_json_assertion(body) {
        json
    } else {
        bail!("unsupported pm.test assertion");
    };

    Ok(Some(ResponseAssertion { name, kind }))
}

fn parse_pm_test_name(line: &str) -> Result<(String, usize)> {
    let start = "pm.test(".len();
    let rest = line[start..].trim_start();
    let skipped = line[start..].len() - rest.len();
    let quote = rest
        .chars()
        .next()
        .ok_or_else(|| anyhow!("pm.test expects a quoted name"))?;
    if quote != '"' && quote != '\'' {
        bail!("pm.test expects a quoted name");
    }
    let Some((name, consumed)) = parse_js_string_literal(rest) else {
        bail!("pm.test name is not closed");
    };
    Ok((name, start + skipped + consumed))
}

fn parse_pm_status_assertion(body: &str) -> Result<Option<ResponseAssertionKind>> {
    if let Some(value) = call_argument_after(body, "pm.response.to.have.status(") {
        return Ok(Some(ResponseAssertionKind::StatusEquals {
            status: parse_status(value, "pm.response.to.have.status")?,
        }));
    }

    if body.contains("pm.response.to.be.success") {
        return Ok(Some(ResponseAssertionKind::StatusInRange {
            min: 200,
            max: 299,
        }));
    }

    for subject in ["pm.response.code", "pm.response.status"] {
        if let Some(value) = expect_equal_argument(body, subject) {
            return Ok(Some(ResponseAssertionKind::StatusEquals {
                status: parse_status(value, "pm.expect status")?,
            }));
        }
        if let Some((min, max)) = expect_within_arguments(body, subject) {
            return Ok(Some(ResponseAssertionKind::StatusInRange {
                min: parse_status(min, "pm.expect status range")?,
                max: parse_status(max, "pm.expect status range")?,
            }));
        }
    }

    Ok(None)
}

fn parse_pm_metric_assertion(body: &str) -> Result<Option<ResponseAssertionKind>> {
    if let Some(value) = expect_upper_bound_argument(body, "pm.response.responseTime") {
        return Ok(Some(ResponseAssertionKind::ResponseTimeBelow {
            max_ms: parse_response_time_ms(value, "pm.expect response time")?,
        }));
    }

    if let Some(value) = expect_upper_bound_argument(body, "pm.response.responseSize") {
        return Ok(Some(ResponseAssertionKind::ResponseSizeBelow {
            max_bytes: parse_response_size_bytes(value, "pm.expect response size")?,
        }));
    }

    Ok(None)
}

fn parse_pm_header_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if let Some(args) = call_argument_after(body, "pm.response.to.not.have.header(") {
        let parts = split_js_arguments(args);
        let name = parts.first().map(|value| strip_js_string_value(value))?;
        return Some(ResponseAssertionKind::HeaderNotExists { name });
    }

    if let Some(args) = call_argument_after(body, "pm.response.to.have.header(") {
        let parts = split_js_arguments(args);
        let name = parts.first().map(|value| strip_js_string_value(value))?;
        if let Some(value) = parts.get(1) {
            return Some(ResponseAssertionKind::HeaderEquals {
                name,
                value: strip_js_string_value(value),
            });
        }
        return Some(ResponseAssertionKind::HeaderExists { name });
    }

    if let Some(value) = call_argument_after(body, "pm.response.headers.has(") {
        let name = strip_js_string_value(value);
        if body.contains(".to.be.false") || body.contains(".to.equal(false)") {
            return Some(ResponseAssertionKind::HeaderNotExists { name });
        }
        return Some(ResponseAssertionKind::HeaderExists { name });
    }

    let marker = "pm.response.headers.get(";
    let start = body.find(marker)? + marker.len();
    let (name, consumed) = parse_js_string_literal(body[start..].trim_start())?;
    let after_name_start =
        start + (body[start..].len() - body[start..].trim_start().len()) + consumed;
    let after_name = &body[after_name_start..];
    let value = expect_equal_argument(after_name, "")?;
    Some(ResponseAssertionKind::HeaderEquals {
        name,
        value: strip_js_string_value(value),
    })
}

fn parse_pm_body_assertion(body: &str) -> Option<ResponseAssertionKind> {
    if !body.contains("pm.response.text()") && !body.contains("pm.response.body") {
        return None;
    }

    for marker in [".to.include(", ".to.contain(", ".to.have.string("] {
        if let Some(value) = call_argument_after(body, marker) {
            return Some(ResponseAssertionKind::BodyContains {
                text: strip_js_string_value(value),
            });
        }
    }

    for marker in [
        ".to.not.include(",
        ".to.not.contain(",
        ".to.not.have.string(",
    ] {
        if let Some(value) = call_argument_after(body, marker) {
            return Some(ResponseAssertionKind::BodyNotContains {
                text: strip_js_string_value(value),
            });
        }
    }

    for subject in ["pm.response.text()", "pm.response.body"] {
        if let Some(value) = expect_equal_argument(body, subject) {
            return Some(ResponseAssertionKind::BodyEquals {
                text: strip_js_string_value(value),
            });
        }
    }

    None
}
