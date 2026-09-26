// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests that a request line goes through parse and dispatch and its answer
//! reaches the client: exec output, an immediate status, and errors.

use super::{TmpServerDir, poll_until_done, read_response, setup_session, unique_server_name};
use crate::api_dispatch::handle_api_actions;
use crate::capture::parse_api_request;
use crate::server::Server;
use crate::session::Session;
use std::os::unix::net::UnixStream;
use std::time::Duration;

fn dispatch_line(session: &mut Session, srv: &mut Server, line: &str) -> UnixStream {
    let (daemon_end, client) = UnixStream::pair().unwrap();
    daemon_end.set_nonblocking(true).unwrap();
    let action = parse_api_request(line, session, daemon_end);
    srv.pending_actions.push(action);
    handle_api_actions(srv, session).unwrap();
    client
}

#[test]
fn exec_line_runs_through_parse_and_dispatch_and_returns_output() {
    let mut session = setup_session();
    let name = unique_server_name();
    let _dir = TmpServerDir(name.clone());
    let mut srv = Server::start_named(&name).unwrap();
    let mut client = dispatch_line(&mut session, &mut srv, "exec echo dispatch_marker_42");
    assert_eq!(srv.run_requests.len(), 1, "exec must park one run request");
    let mut reqs = std::mem::take(&mut srv.run_requests);
    poll_until_done(&mut reqs, &mut session, Duration::from_secs(10));
    assert!(reqs.is_empty(), "exec should have completed");
    let json = read_response(&mut client);
    assert_eq!(json["exit_code"], 0);
    assert!(
        json["output"]
            .as_str()
            .unwrap()
            .contains("dispatch_marker_42")
    );
    assert_eq!(json["timeout"], false);
}

#[test]
fn status_line_is_answered_during_dispatch() {
    let mut session = setup_session();
    let name = unique_server_name();
    let _dir = TmpServerDir(name.clone());
    let mut srv = Server::start_named(&name).unwrap();
    let mut client = dispatch_line(&mut session, &mut srv, "status");
    let json = read_response(&mut client);
    assert!(json.is_object(), "status must answer with a json object");
    assert!(json.get("error").is_none(), "status must not error: {json}");
}

#[test]
fn close_and_wait_on_unknown_pane_answer_with_an_error() {
    let mut session = setup_session();
    let name = unique_server_name();
    let _dir = TmpServerDir(name.clone());
    let mut srv = Server::start_named(&name).unwrap();
    let mut closed = dispatch_line(&mut session, &mut srv, "close 9999");
    assert!(read_response(&mut closed)["error"].is_string());
    let mut waited = dispatch_line(&mut session, &mut srv, "wait 9999 ready 1");
    assert_eq!(read_response(&mut waited)["error"], "pane not found: 9999");
    assert!(srv.waiting_clients.is_empty());
}
