// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Tests for the control socket: socket cleanup on drop, viewer attach,
//! the lock the first viewer takes, and releasing it on disconnect.

use super::*;
use std::io::Write;
use std::os::unix::net::UnixStream;

#[test]
fn server_drop_removes_socket() {
    let name = format!("test-drop-{}", std::process::id());
    let srv = Server::start_named(&name).unwrap();
    let path = srv.socket_path.clone();
    assert!(path.exists());
    drop(srv);
    assert!(!path.exists(), "socket should be removed on Drop");
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn no_display_client_initially() {
    let name = format!("test-viewer-{}", std::process::id());
    let srv = Server::start_named(&name).unwrap();
    assert!(!srv.has_display_client());
    assert_eq!(srv.observer_count(), 0);
    assert_eq!(srv.lock.holder(), 0, "no holder when no observers");
    let _ = std::fs::remove_dir_all(srv.socket_path.parent().unwrap());
}

fn make_session() -> crate::session::Session {
    crate::session::Session::new(80, 24)
}

fn rand_suffix() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}", std::process::id(), nanos)
}

fn drive_accept(srv: &mut Server, session: &crate::session::Session) {
    // why: the listener is non-blocking; give the kernel time to queue the accept.
    std::thread::sleep(std::time::Duration::from_millis(50));
    srv.poll_connections(session);
}

#[test]
fn first_attach_auto_takes_lock_and_mints_client_id_1() {
    let name = format!("test-attach-{}", rand_suffix());
    let mut srv = Server::start_named(&name).unwrap();
    let socket = srv.socket_path.clone();
    let session = make_session();

    let mut client = UnixStream::connect(&socket).unwrap();
    client.write_all(b"A").unwrap();
    client.flush().unwrap();
    drive_accept(&mut srv, &session);

    assert_eq!(srv.observer_count(), 1, "first attach registered");
    assert_eq!(
        srv.lock.holder(),
        1,
        "first attach auto-takes lock as client_id 1"
    );
    assert!(srv.has_display_client());

    drop(client);
    let _ = std::fs::remove_dir_all(socket.parent().unwrap());
}

#[test]
fn second_attach_does_not_displace_first_observer() {
    let name = format!("test-multi-{}", rand_suffix());
    let mut srv = Server::start_named(&name).unwrap();
    let socket = srv.socket_path.clone();
    let session = make_session();

    let mut a = UnixStream::connect(&socket).unwrap();
    a.write_all(b"A").unwrap();
    a.flush().unwrap();
    drive_accept(&mut srv, &session);
    assert_eq!(srv.observer_count(), 1);

    let mut b = UnixStream::connect(&socket).unwrap();
    b.write_all(b"A").unwrap();
    b.flush().unwrap();
    drive_accept(&mut srv, &session);

    assert_eq!(
        srv.observer_count(),
        2,
        "second attach does NOT clear first (multi-attach)"
    );
    assert_eq!(
        srv.lock.holder(),
        1,
        "lock stays with first attacher (client_id 1)"
    );

    drop(a);
    drop(b);
    let _ = std::fs::remove_dir_all(socket.parent().unwrap());
}

#[test]
fn disconnect_viewers_releases_lock() {
    let name = format!("test-disconn-{}", rand_suffix());
    let mut srv = Server::start_named(&name).unwrap();
    let socket = srv.socket_path.clone();
    let session = make_session();

    let mut client = UnixStream::connect(&socket).unwrap();
    client.write_all(b"A").unwrap();
    client.flush().unwrap();
    drive_accept(&mut srv, &session);
    assert_eq!(srv.lock.holder(), 1);

    srv.disconnect_viewers();
    assert_eq!(srv.observer_count(), 0);
    assert_eq!(
        srv.lock.holder(),
        0,
        "disconnect_viewers releases lock so reattach starts clean"
    );

    drop(client);
    let _ = std::fs::remove_dir_all(socket.parent().unwrap());
}

#[test]
fn three_attaches_advance_client_id_but_holder_stays_first() {
    let name = format!("test-monotonic-{}", rand_suffix());
    let mut srv = Server::start_named(&name).unwrap();
    let socket = srv.socket_path.clone();
    let session = make_session();

    for _ in 0..3 {
        let mut c = UnixStream::connect(&socket).unwrap();
        c.write_all(b"A").unwrap();
        c.flush().unwrap();
        drive_accept(&mut srv, &session);
    }

    assert_eq!(srv.observer_count(), 3);
    // why: later attachers fail try_take with Conflict, so the holder stays 1.
    assert_eq!(srv.lock.holder(), 1);

    let _ = std::fs::remove_dir_all(socket.parent().unwrap());
}
