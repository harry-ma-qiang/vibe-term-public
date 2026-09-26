// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for heredoc commands through exec and through the request parser.

use super::{poll_until_done, read_response, run_exec, setup_session};
use crate::session::Session;
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[test]
fn exec_heredoc_single_line_cat() {
    let mut session = setup_session();
    let cmd = "cat <<'EOF'\nhello_heredoc_body\nEOF";
    let (mut reqs, mut reader) = run_exec(&mut session, cmd, 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty(), "heredoc request should have completed");
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0, "heredoc cat should exit 0: {json}");
    assert!(
        json["output"]
            .as_str()
            .unwrap()
            .contains("hello_heredoc_body"),
        "output should contain heredoc body: {json}"
    );
}

#[test]
fn exec_heredoc_multi_line_python() {
    let mut session = setup_session();
    let cmd = "python3 - <<'PY'\nimport json\nprint(json.dumps({'k':'v'}))\nPY";
    let (mut reqs, mut reader) = run_exec(&mut session, cmd, 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty(), "python heredoc should have completed");
    let json = read_response(&mut reader);
    assert_eq!(json["exit_code"], 0, "python heredoc should exit 0: {json}");
    let out = json["output"].as_str().unwrap();
    assert!(
        out.contains("\"k\"") && out.contains("\"v\""),
        "python heredoc stdout should contain json with k/v: {json}"
    );
}

#[test]
fn exec_heredoc_nested_quotes_preserved() {
    let mut session = setup_session();
    // why: nested single + double quotes inside heredoc body must not be mangled.
    let cmd = "cat <<'EOF'\nmix 'single' and \"double\" quotes\nEOF";
    let (mut reqs, mut reader) = run_exec(&mut session, cmd, 10, false);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(
        reqs.is_empty(),
        "nested quote heredoc should have completed"
    );
    let json = read_response(&mut reader);
    assert_eq!(
        json["exit_code"], 0,
        "nested quote heredoc should exit 0: {json}"
    );
    let out = json["output"].as_str().unwrap();
    assert!(
        out.contains("'single'"),
        "single quotes should survive: {json}"
    );
    assert!(
        out.contains("\"double\""),
        "double quotes should survive: {json}"
    );
}

#[test]
fn parse_api_request_preserves_newlines_in_tab_encoded_arg() {
    // why: tab-joined args keep a heredoc's newlines through request tokenizing.
    let session = Session::new(160, 45);
    let (s1, _s2) = UnixStream::pair().unwrap();
    let heredoc_arg = "cat <<'EOF'\nhello\nEOF";
    let request = format!("exec\t{heredoc_arg}");
    let action = crate::capture::parse_api_request(&request, &session, s1);
    match action {
        crate::capture::ApiAction::Exec { cmd, .. } => {
            assert!(cmd.contains('\n'), "newlines must be preserved: {cmd:?}");
            assert!(cmd.contains("hello"), "body must survive: {cmd:?}");
            assert!(cmd.contains("EOF"), "delimiter must survive: {cmd:?}");
        }
        _ => panic!("expected Exec action for heredoc exec request"),
    }
}
