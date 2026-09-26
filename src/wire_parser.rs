// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Splits wire argv into multiplexer flags and the child command line.

const DEFAULT_TIMEOUT: u32 = 300;

/// A parsed `exec` invocation.
#[derive(Debug, PartialEq)]
pub struct ParsedExecArgs {
    pub timeout_secs: u32,
    pub no_timeout: bool,
    pub child_cmd: String,
    pub notify_pattern: Option<String>,
}

/// Why an argv could not be parsed.
#[derive(Debug, PartialEq)]
pub enum ParseError {
    NoCommand,
    InvalidTimeout(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NoCommand => write!(f, "no command provided"),
            ParseError::InvalidTimeout(v) => write!(f, "invalid --vt-timeout value: {v}"),
        }
    }
}

/// Parse `exec` argv: leading flags, then everything else as the command.
pub fn parse_exec_args(tokens: &[&str]) -> Result<ParsedExecArgs, ParseError> {
    let (timeout_secs, no_timeout, notify_pattern, child_start) = parse_vt_flags(tokens)?;

    if child_start >= tokens.len() {
        return Err(ParseError::NoCommand);
    }

    let child_cmd = tokens[child_start..].join(" ");
    if child_cmd.is_empty() {
        return Err(ParseError::NoCommand);
    }

    Ok(ParsedExecArgs {
        timeout_secs,
        no_timeout,
        child_cmd,
        notify_pattern,
    })
}

/// A parsed `serve` invocation.
#[derive(Debug, PartialEq)]
pub struct ParsedServeArgs {
    pub timeout_secs: u32,
    pub no_timeout: bool,
    pub child_cmd: String,
    pub pattern: Option<String>,
}

fn parse_vt_flags(tokens: &[&str]) -> Result<(u32, bool, Option<String>, usize), ParseError> {
    let mut timeout_secs = DEFAULT_TIMEOUT;
    let mut no_timeout = false;
    let mut notify_pattern = None;
    let mut i = 0;

    while i < tokens.len() {
        let tok = tokens[i];

        if tok == "--" {
            return Ok((timeout_secs, no_timeout, notify_pattern.clone(), i + 1));
        }

        if tok == "--vt-timeout" {
            let val_str = tokens
                .get(i + 1)
                .ok_or_else(|| ParseError::InvalidTimeout("missing value".to_string()))?;
            let val: i64 = val_str
                .parse()
                .map_err(|_| ParseError::InvalidTimeout(val_str.to_string()))?;
            if val < 0 {
                return Err(ParseError::InvalidTimeout(val_str.to_string()));
            }
            if val == 0 {
                no_timeout = true;
                timeout_secs = 0;
            } else {
                timeout_secs = val as u32;
            }
            i += 2;
            continue;
        }

        if tok == "--notify" {
            let val_str = tokens.get(i + 1).ok_or(ParseError::NoCommand)?;
            notify_pattern = Some(val_str.to_string());
            i += 2;
            continue;
        }

        return Ok((timeout_secs, no_timeout, notify_pattern.clone(), i));
    }

    Ok((timeout_secs, no_timeout, notify_pattern, i))
}

/// Parse `serve` argv, where a trailing token is the ready pattern.
pub fn parse_serve_args(tokens: &[&str]) -> Result<ParsedServeArgs, ParseError> {
    let (timeout_secs, no_timeout, _, child_start) = parse_vt_flags(tokens)?;

    if child_start >= tokens.len() {
        return Err(ParseError::NoCommand);
    }

    let remaining = &tokens[child_start..];

    if remaining.len() == 1 {
        return Ok(ParsedServeArgs {
            timeout_secs,
            no_timeout,
            child_cmd: remaining[0].to_string(),
            pattern: None,
        });
    }

    let pattern = remaining.last().unwrap().to_string();
    let cmd = remaining[..remaining.len() - 1].join(" ");

    Ok(ParsedServeArgs {
        timeout_secs,
        no_timeout,
        child_cmd: cmd,
        pattern: Some(pattern),
    })
}

#[cfg(test)]
#[path = "tests/wire_parser.rs"]
mod tests;
