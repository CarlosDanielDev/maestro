//! Tokenizer for the placeholder language.
//!
//! Hand-written state-machine scanner. Single-line placeholders only —
//! newline inside a placeholder body returns `UnterminatedPlaceholder`.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

use std::collections::BTreeMap;

use super::Token;
use crate::templates::TemplateError;

pub(crate) fn tokenize(input: &str, source_path: &str) -> Result<Vec<Token>, TemplateError> {
    let bytes = input.as_bytes();
    let mut tokens = Vec::new();
    let mut text_start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'{' && bytes[i + 1] == b'{' {
            if text_start < i {
                tokens.push(Token::Text(input[text_start..i].to_string()));
            }
            let placeholder_start = i;
            let body_start = i + 2;
            let close = find_close(bytes, body_start).ok_or_else(|| {
                TemplateError::UnterminatedPlaceholder {
                    offset: placeholder_start,
                    source_path: source_path.to_string(),
                }
            })?;
            let body = &input[body_start..close];
            let (name, args) = parse_placeholder_body(body, placeholder_start, source_path)?;
            tokens.push(Token::Placeholder {
                name,
                args,
                offset: placeholder_start,
            });
            i = close + 2;
            text_start = i;
        } else {
            i += 1;
        }
    }
    if text_start < bytes.len() {
        tokens.push(Token::Text(input[text_start..].to_string()));
    }
    Ok(tokens)
}

fn find_close(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    let mut in_string = false;
    while i + 1 < bytes.len() {
        let b = bytes[i];
        if !in_string && b == b'\n' {
            return None;
        }
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if b == b'}' && bytes[i + 1] == b'}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_placeholder_body(
    body: &str,
    offset: usize,
    source_path: &str,
) -> Result<(String, BTreeMap<String, String>), TemplateError> {
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
        i += 1;
    }
    let name_start = i;
    while i < bytes.len()
        && (bytes[i].is_ascii_uppercase() || bytes[i].is_ascii_digit() || bytes[i] == b'_')
    {
        i += 1;
    }
    if i == name_start {
        return Err(TemplateError::InvalidPlaceholder {
            name: String::new(),
            offset,
            source_path: source_path.to_string(),
            reason: "empty placeholder name".to_string(),
        });
    }
    let name = body[name_start..i].to_string();
    let mut args = BTreeMap::new();
    loop {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let key_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_lowercase() || bytes[i] == b'_') {
            i += 1;
        }
        if i == key_start {
            return Err(TemplateError::InvalidPlaceholder {
                name,
                offset,
                source_path: source_path.to_string(),
                reason: format!("unexpected character at body byte {}", i),
            });
        }
        let key = body[key_start..i].to_string();
        if i >= bytes.len() || bytes[i] != b'=' {
            return Err(TemplateError::InvalidPlaceholder {
                name,
                offset,
                source_path: source_path.to_string(),
                reason: format!("expected `=` after argument key `{key}`"),
            });
        }
        i += 1;
        if i >= bytes.len() || bytes[i] != b'"' {
            return Err(TemplateError::InvalidPlaceholder {
                name,
                offset,
                source_path: source_path.to_string(),
                reason: format!("expected `\"` to open value for `{key}`"),
            });
        }
        i += 1;
        let value = read_string_value(bytes, &mut i, &name, &key, offset, source_path)?;
        args.insert(key, value);
    }
    Ok((name, args))
}

fn read_string_value(
    bytes: &[u8],
    i: &mut usize,
    name: &str,
    key: &str,
    offset: usize,
    source_path: &str,
) -> Result<String, TemplateError> {
    let mut buf: Vec<u8> = Vec::new();
    loop {
        if *i >= bytes.len() {
            return Err(TemplateError::InvalidPlaceholder {
                name: name.to_string(),
                offset,
                source_path: source_path.to_string(),
                reason: format!("unterminated string for `{key}`"),
            });
        }
        let b = bytes[*i];
        if b == b'\\' && *i + 1 < bytes.len() {
            let next = bytes[*i + 1];
            match next {
                b'"' => buf.push(b'"'),
                b'\\' => buf.push(b'\\'),
                b'n' => buf.push(b'\n'),
                b't' => buf.push(b'\t'),
                other => {
                    return Err(TemplateError::InvalidPlaceholder {
                        name: name.to_string(),
                        offset,
                        source_path: source_path.to_string(),
                        reason: format!("unknown escape `\\{}`", other as char),
                    });
                }
            }
            *i += 2;
            continue;
        }
        if b == b'"' {
            *i += 1;
            break;
        }
        if b < 0x20 && b != b'\t' {
            return Err(TemplateError::InvalidPlaceholder {
                name: name.to_string(),
                offset,
                source_path: source_path.to_string(),
                reason: format!("control byte 0x{b:02x} in value for `{key}`"),
            });
        }
        buf.push(b);
        *i += 1;
    }
    String::from_utf8(buf).map_err(|e| TemplateError::InvalidPlaceholder {
        name: name.to_string(),
        offset,
        source_path: source_path.to_string(),
        reason: format!("invalid UTF-8 in value for `{key}`: {e}"),
    })
}
