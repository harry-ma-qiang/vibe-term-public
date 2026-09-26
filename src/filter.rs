// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Secret redaction: replaces literals, regex-matched secret shapes and
//! user-supplied words in a screen with a block character, keeping layout.

use crate::emulator::Screen;
use regex::Regex;
use std::io;
use std::path::{Path, PathBuf};

const MIN_SECRET_LENGTH: usize = 6;
const REDACT_CHAR: char = '█';

/// Environment variable naming the file of extra literals to redact.
pub const ENV_FILTER_LITERALS: &str = "VIBE_TERM_FILTER_LITERALS";
const DEFAULT_LITERALS_FILE: &str = "filter-literals.txt";

/// Three redaction layers for one pane: literals, regex shapes and a word list.
pub struct SecretFilter {
    literals: Vec<String>,
    regex_patterns: Vec<Regex>,
    blocklist: Vec<String>,
    enabled: bool,
}

impl Default for SecretFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretFilter {
    /// An empty, disabled filter.
    pub fn new() -> Self {
        Self {
            literals: Vec::new(),
            regex_patterns: Vec::new(),
            blocklist: Vec::new(),
            enabled: false,
        }
    }

    /// Start redacting.
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    /// Stop redacting without discarding what was loaded.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
    /// Whether `scrub_screen` will do anything.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Copy the loaded secrets into an independent filter.
    pub fn clone_secrets(&self) -> Self {
        Self {
            literals: self.literals.clone(),
            regex_patterns: self
                .regex_patterns
                .iter()
                .filter_map(|r| Regex::new(r.as_str()).ok())
                .collect(),
            blocklist: self.blocklist.clone(),
            enabled: self.enabled,
        }
    }

    /// Add every value in a dotenv-style file, plus its delimited parts.
    pub fn load_dotenv(&mut self, path: &Path) -> io::Result<()> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: cannot read filter file {:?}: {}", path, e);
                return Ok(());
            }
        };
        for value in parse_dotenv(&content) {
            if value.len() >= MIN_SECRET_LENGTH {
                for derived in extract_derived_secrets(&value) {
                    self.literals.push(derived);
                }
                self.literals.push(value);
            }
        }
        self.literals.sort_by_key(|s| std::cmp::Reverse(s.len()));
        self.literals.dedup();
        if !self.literals.is_empty() {
            self.enabled = true;
        }
        Ok(())
    }

    /// Add one literal, ignoring anything shorter than the minimum length.
    pub fn add_literal(&mut self, secret: &str) {
        if secret.len() >= MIN_SECRET_LENGTH {
            self.literals.push(secret.to_string());
            self.literals.sort_by_key(|s| std::cmp::Reverse(s.len()));
            self.enabled = true;
        }
    }

    /// Add the local user, host and git identity strings as literals.
    pub fn load_env_identity(&mut self) {
        use std::process::Command;
        let mut candidates = Vec::new();
        for var in ["USER", "LOGNAME", "HOSTNAME"] {
            if let Ok(val) = std::env::var(var) {
                candidates.push(val);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(home);
        }
        if let Ok(conn) = std::env::var("SSH_CONNECTION") {
            for part in conn.split_whitespace() {
                candidates.push(part.to_string());
            }
        }
        for cmd_args in [
            &["hostname"][..],
            &["git", "config", "user.name"],
            &["git", "config", "user.email"],
        ] {
            if let Ok(output) = Command::new(cmd_args[0]).args(&cmd_args[1..]).output() {
                let val = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !val.is_empty() {
                    candidates.push(val);
                }
            }
        }
        if let Ok(output) = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .output()
        {
            let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
            for part in parse_git_remote_identity(&url) {
                candidates.push(part);
            }
        }
        for val in candidates {
            if val.len() >= MIN_SECRET_LENGTH {
                self.literals.push(val);
            }
        }
        self.literals.sort_by_key(|s| std::cmp::Reverse(s.len()));
        self.literals.dedup();
    }

    /// Load the user's own word list from `literals_path`, if that file exists.
    pub fn load_user_literals(&mut self) -> io::Result<()> {
        self.load_optional_blocklist(&literals_path())
    }

    /// Load a word list, treating a missing file as an empty one.
    pub fn load_optional_blocklist(&mut self, path: &Path) -> io::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        self.load_blocklist(path)
    }

    /// Compile the built-in regex patterns for common secret shapes.
    pub fn load_bundled_patterns(&mut self) {
        for pattern in BUNDLED_PATTERNS {
            if let Ok(re) = Regex::new(pattern) {
                self.regex_patterns.push(re);
            }
        }
    }

    /// Load a word list: one entry per line, `#` starts a comment.
    pub fn load_blocklist(&mut self, path: &Path) -> io::Result<()> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("warning: cannot read blocklist {:?}: {}", path, e);
                return Ok(());
            }
        };
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.len() >= MIN_SECRET_LENGTH {
                self.blocklist.push(trimmed.to_string());
            }
        }
        self.blocklist.sort_by_key(|s| std::cmp::Reverse(s.len()));
        if !self.blocklist.is_empty() {
            self.enabled = true;
        }
        Ok(())
    }

    /// Redact every match in place, leaving the grid the same shape.
    pub fn scrub_screen(&self, screen: &mut Screen) {
        if !self.enabled {
            return;
        }
        let (flat_text, cell_map) = flatten_screen(screen);
        if flat_text.is_empty() {
            return;
        }

        self.scrub_literals(&flat_text, &cell_map, screen);
        self.scrub_regex(&flat_text, &cell_map, screen);
        self.scrub_blocklist(&flat_text, &cell_map, screen);
    }

    fn scrub_literals(&self, flat: &str, map: &[(usize, usize)], screen: &mut Screen) {
        for secret in &self.literals {
            let mut from = 0;
            while let Some(pos) = flat[from..].find(secret.as_str()) {
                let abs = from + pos;
                redact_range(screen, map, abs, secret.len(), flat);
                from = abs + secret.len();
            }
        }
    }

    fn scrub_regex(&self, flat: &str, map: &[(usize, usize)], screen: &mut Screen) {
        for re in &self.regex_patterns {
            for m in re.find_iter(flat) {
                redact_range(screen, map, m.start(), m.len(), flat);
            }
        }
    }

    fn scrub_blocklist(&self, flat: &str, map: &[(usize, usize)], screen: &mut Screen) {
        for entry in &self.blocklist {
            let mut from = 0;
            while let Some(pos) = flat[from..].find(entry.as_str()) {
                let abs = from + pos;
                redact_range(screen, map, abs, entry.len(), flat);
                from = abs + entry.len();
            }
        }
    }

    /// How many literals are loaded.
    pub fn literal_count(&self) -> usize {
        self.literals.len()
    }
    /// How many regex patterns are compiled.
    pub fn regex_count(&self) -> usize {
        self.regex_patterns.len()
    }
    /// How many word-list entries are loaded.
    pub fn blocklist_count(&self) -> usize {
        self.blocklist.len()
    }
}

/// The word-list file: `VIBE_TERM_FILTER_LITERALS`, else one in the config dir.
pub fn literals_path() -> PathBuf {
    if let Ok(path) = std::env::var(ENV_FILTER_LITERALS)
        && !path.is_empty()
    {
        return PathBuf::from(path);
    }
    config_dir().join("vibe-term").join(DEFAULT_LITERALS_FILE)
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".config");
    }
    PathBuf::from(".config")
}

fn redact_range(
    screen: &mut Screen,
    map: &[(usize, usize)],
    byte_start: usize,
    byte_len: usize,
    flat: &str,
) {
    let char_start = flat[..byte_start].chars().count();
    let char_len = flat[byte_start..byte_start + byte_len].chars().count();
    for i in char_start..(char_start + char_len) {
        if i < map.len() {
            let (r, c) = map[i];
            if r < screen.rows && c < screen.cols {
                screen.cells[r][c].ch = REDACT_CHAR;
                screen.dirty[r] = true;
            }
        }
    }
}

fn parse_git_remote_identity(url: &str) -> Vec<String> {
    let path = if let Some(rest) = url.strip_prefix("git@") {
        rest.split_once(':').map(|(_, p)| p.to_string())
    } else if url.starts_with("https://") || url.starts_with("http://") {
        let segs: Vec<&str> = url.splitn(4, '/').collect();
        segs.get(3).map(|s| s.to_string())
    } else {
        None
    };
    match path {
        Some(p) => p
            .trim_end_matches(".git")
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect(),
        None => Vec::new(),
    }
}

fn flatten_screen(screen: &Screen) -> (String, Vec<(usize, usize)>) {
    let mut text = String::new();
    let mut map = Vec::new();

    for r in 0..screen.rows.min(screen.cells.len()) {
        if r > 0 && !screen.wrapped[r.saturating_sub(1)] {
            text.push('\n');
            map.push((usize::MAX, 0));
        }
        let mut row_chars: Vec<(char, usize)> = Vec::new();
        for c in 0..screen.cols {
            if c < screen.cells[r].len() && screen.cells[r][c].width > 0 {
                row_chars.push((screen.cells[r][c].ch, c));
            }
        }
        while row_chars.last().map(|(ch, _)| *ch == ' ').unwrap_or(false) {
            row_chars.pop();
        }
        for (ch, c) in row_chars {
            text.push(ch);
            map.push((r, c));
        }
    }
    (text, map)
}

fn extract_derived_secrets(value: &str) -> Vec<String> {
    value
        .split(['/', ':', '@', ';', '?', '&', ',', '='])
        .map(str::trim)
        .filter(|s| s.len() >= MIN_SECRET_LENGTH)
        .map(str::to_string)
        .collect()
}

/// Collect the right-hand side of every assignment in dotenv-style text.
pub fn parse_dotenv(content: &str) -> Vec<String> {
    let mut values = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let work = if let Some(stripped) = trimmed.strip_prefix("export ") {
            stripped.trim()
        } else {
            trimmed
        };

        let Some(eq_pos) = work.find('=') else {
            continue;
        };
        let raw_value = work[eq_pos + 1..].trim();

        let value = strip_quotes(raw_value);
        if !value.is_empty() {
            values.push(value);
        }
    }
    values
}

fn strip_quotes(s: &str) -> String {
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return s[1..s.len() - 1].to_string();
    }
    s.to_string()
}

const BUNDLED_PATTERNS: &[&str] = &[
    r"(?i)sk-[a-z0-9]{20,}",
    r"(?i)sk-ant-api\d+-[A-Za-z0-9_-]{20,}",
    r"(?i)sk-proj-[A-Za-z0-9_-]{20,}",
    r"AKIA[0-9A-Z]{16}",
    r"(?i)ghp_[A-Za-z0-9]{36}",
    r"(?i)gho_[A-Za-z0-9]{36}",
    r"(?i)github_pat_[A-Za-z0-9_]{22,}",
    r"(?i)glpat-[A-Za-z0-9_-]{20,}",
    r"(?i)sk_live_[A-Za-z0-9]{24,}",
    r"(?i)rk_live_[A-Za-z0-9]{24,}",
    r"(?i)sq0atp-[A-Za-z0-9_-]{22,}",
    r"(?i)xox[bpas]-[A-Za-z0-9-]{10,}",
    r"(?i)hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[A-Za-z0-9]+",
    r"(?i)SG\.[A-Za-z0-9_-]{22,}\.[A-Za-z0-9_-]{22,}",
    r"(?i)eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}",
    r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
    r"(?i)bearer\s+[A-Za-z0-9_-]{20,}",
    r"(?i)password\s*[:=]\s*\S{6,}",
    r"(?i)secret\s*[:=]\s*\S{6,}",
    r"(?i)api[_-]?key\s*[:=]\s*\S{6,}",
];

#[cfg(test)]
#[path = "tests/filter.rs"]
mod tests;
