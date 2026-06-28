pub(super) fn expect_equal_argument<'a>(body: &'a str, subject: &str) -> Option<&'a str> {
    let haystack = if subject.is_empty() {
        body
    } else {
        &body[body.find(subject)? + subject.len()..]
    };

    for marker in [
        ".to.equal(",
        ".to.eql(",
        ".to.be.equal(",
        ".to.be.eql(",
        ".to.deep.equal(",
        ".to.deep.eql(",
    ] {
        if let Some(value) = call_argument_after(haystack, marker) {
            return Some(value);
        }
    }

    None
}

pub(super) fn expect_equal_argument_after_subject(after_subject: &str) -> Option<&str> {
    let chain = trim_js_subject_suffix(after_subject);

    for marker in [
        ".to.equal(",
        ".to.eql(",
        ".to.be.equal(",
        ".to.be.eql(",
        ".to.deep.equal(",
        ".to.deep.eql(",
    ] {
        if let Some(value) = call_argument_at_start(chain, marker) {
            return Some(value);
        }
    }

    None
}

pub(super) fn expect_not_equal_argument_after_subject(after_subject: &str) -> Option<&str> {
    let chain = trim_js_subject_suffix(after_subject);

    for marker in [
        ".to.not.equal(",
        ".to.not.eql(",
        ".to.not.be.equal(",
        ".to.not.be.eql(",
        ".to.not.deep.equal(",
        ".to.not.deep.eql(",
    ] {
        if let Some(value) = call_argument_at_start(chain, marker) {
            return Some(value);
        }
    }

    None
}

pub(super) fn expect_within_arguments<'a>(
    body: &'a str,
    subject: &str,
) -> Option<(&'a str, &'a str)> {
    let haystack = &body[body.find(subject)? + subject.len()..];
    let args = call_argument_after(haystack, ".to.be.within(")?;
    let parts = split_js_arguments(args);
    Some((*parts.first()?, *parts.get(1)?))
}

pub(super) fn expect_upper_bound_argument<'a>(body: &'a str, subject: &str) -> Option<&'a str> {
    let haystack = &body[body.find(subject)? + subject.len()..];
    for marker in [
        ".to.be.below(",
        ".to.be.lessThan(",
        ".to.be.at.most(",
        ".to.be.lte(",
        ".to.be.most(",
    ] {
        if let Some(value) = call_argument_after(haystack, marker) {
            return Some(value);
        }
    }
    None
}

pub(super) fn call_argument_after<'a>(body: &'a str, marker: &str) -> Option<&'a str> {
    let start = body.find(marker)? + marker.len();
    let end = find_js_call_argument_end(body, start)?;
    Some(body[start..end].trim())
}

pub(super) fn call_argument_after_subject<'a>(
    after_subject: &'a str,
    marker: &str,
) -> Option<&'a str> {
    call_argument_at_start(trim_js_subject_suffix(after_subject), marker)
}

fn call_argument_at_start<'a>(body: &'a str, marker: &str) -> Option<&'a str> {
    if !body.starts_with(marker) {
        return None;
    }

    let start = marker.len();
    let end = find_js_call_argument_end(body, start)?;
    Some(body[start..end].trim())
}

pub(super) fn trim_js_subject_suffix(after_subject: &str) -> &str {
    let chain = after_subject.trim_start();
    chain
        .strip_prefix(')')
        .map(str::trim_start)
        .unwrap_or(chain)
}

fn find_js_call_argument_end(input: &str, start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for (offset, character) in input[start..].char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '"' | '\'' => quote = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' => {
                if depth == 0 {
                    return Some(start + offset);
                }
                depth -= 1;
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    None
}

pub(super) fn split_js_arguments(input: &str) -> Vec<&str> {
    let mut arguments = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;

    for (index, character) in input.char_indices() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
            continue;
        }

        match character {
            '"' | '\'' => quote = Some(character),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let argument = input[start..index].trim();
                if !argument.is_empty() {
                    arguments.push(argument);
                }
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }

    let argument = input[start..].trim();
    if !argument.is_empty() {
        arguments.push(argument);
    }
    arguments
}

pub(super) fn parse_js_string_literal(input: &str) -> Option<(String, usize)> {
    let mut chars = input.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }

    let mut value = String::new();
    let mut escaped = false;
    for (index, character) in chars {
        if escaped {
            value.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == quote {
            return Some((value, index + character.len_utf8()));
        } else {
            value.push(character);
        }
    }

    None
}

pub(super) fn strip_js_string_value(value: &str) -> String {
    parse_js_string_literal(value.trim())
        .map(|(value, _)| value)
        .unwrap_or_else(|| value.trim().to_string())
}

pub(super) fn is_js_identifier_boundary(input: &str, start: usize, end: usize) -> bool {
    let before = input[..start].chars().next_back();
    let after = input[end..].chars().next();
    !before.is_some_and(is_js_identifier_character)
        && !after.is_some_and(is_js_identifier_character)
}

fn is_js_identifier_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_' || character == '$'
}

pub(super) fn parse_js_member_path(input: &str) -> Option<(String, usize)> {
    let mut path = Vec::new();
    let mut consumed = 0usize;

    while consumed < input.len() {
        let rest = &input[consumed..];
        if let Some(member) = rest.strip_prefix('.') {
            let name = member
                .chars()
                .take_while(|character| {
                    character.is_ascii_alphanumeric() || *character == '_' || *character == '$'
                })
                .collect::<String>();
            if name.is_empty() {
                break;
            }
            consumed += 1 + name.len();
            path.push(name);
            continue;
        }

        if let Some(indexer) = rest.strip_prefix('[') {
            let end = indexer.find(']')?;
            let selector = indexer[..end].trim();
            let segment = parse_js_string_literal(selector)
                .map(|(value, _)| value)
                .unwrap_or_else(|| selector.to_string());
            if segment.is_empty() {
                break;
            }
            consumed += 1 + end + 1;
            path.push(segment);
            continue;
        }

        break;
    }

    (!path.is_empty()).then(|| (path.join("."), consumed))
}

pub(super) fn join_json_path(base: &str, property: &str) -> String {
    if base.is_empty() {
        property.to_string()
    } else {
        format!("{base}.{property}")
    }
}
