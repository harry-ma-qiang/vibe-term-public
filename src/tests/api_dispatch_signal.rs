// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for signal name handling: the safelist, the case rules around the
//! `SIG` prefix, the refusal of dangerous signals, and canonical names.

use super::{SIGNAL_SAFELIST, normalise_signal_name, parse_signal_name, strip_sig_prefix};

#[test]
fn safelist_seven_names() {
    assert_eq!(SIGNAL_SAFELIST.len(), 7);
    assert_eq!(
        SIGNAL_SAFELIST,
        &["HUP", "INT", "QUIT", "TERM", "USR1", "USR2", "KILL"]
    );
    for n in SIGNAL_SAFELIST {
        assert!(parse_signal_name(n).is_some(), "safelist entry {n} maps");
    }
}

#[test]
fn safelist_rejects_lowercase() {
    for bad in ["term", "Term", "TERM ", "sig_term"] {
        assert!(
            parse_signal_name(bad).is_none(),
            "name {bad:?} must be rejected"
        );
    }
}

#[test]
fn sig_prefixed_form_accepted() {
    use nix::sys::signal::Signal::{SIGINT, SIGKILL, SIGTERM};
    assert_eq!(parse_signal_name("SIGTERM"), Some(SIGTERM));
    assert_eq!(parse_signal_name("sigTERM"), Some(SIGTERM));
    assert_eq!(parse_signal_name("SigINT"), Some(SIGINT));
    assert_eq!(parse_signal_name("SIGKILL"), Some(SIGKILL));
    assert!(parse_signal_name("sigterm").is_none());
    assert_eq!(strip_sig_prefix("SIGTERM"), "TERM");
    assert_eq!(strip_sig_prefix("sigTERM"), "TERM");
    assert_eq!(strip_sig_prefix("TERM"), "TERM");
    assert_eq!(strip_sig_prefix("SIG"), "SIG");
}

#[test]
fn safelist_rejects_dangerous_signals() {
    for bad in [
        "SEGV", "ABRT", "FPE", "PIPE", "STOP", "CONT", "SIGSEGV", "SIGABRT", "WINCH",
    ] {
        assert!(
            parse_signal_name(bad).is_none(),
            "dangerous name {bad:?} must be rejected"
        );
    }
}

#[test]
fn canonical_normalisation_adds_sig_prefix() {
    assert_eq!(normalise_signal_name("TERM"), "SIGTERM");
    assert_eq!(normalise_signal_name("KILL"), "SIGKILL");
    assert_eq!(normalise_signal_name("HUP"), "SIGHUP");
    assert_eq!(normalise_signal_name("INT"), "SIGINT");
    assert_eq!(normalise_signal_name("QUIT"), "SIGQUIT");
    assert_eq!(normalise_signal_name("USR1"), "SIGUSR1");
    assert_eq!(normalise_signal_name("USR2"), "SIGUSR2");
}
