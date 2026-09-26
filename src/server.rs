// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The control socket: accepting viewers and one-shot requests, framing the
//! screen to viewers, lock-gated viewer input, and pending wait requests.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::capture::ApiAction;
use crate::lock::{ClientId, LockState};
use crate::runtime::StreamRegistry;

/// Largest control request accepted, newline included; larger ones get an error.
pub const MAX_REQUEST_BYTES: usize = 256 * 1024;

/// A client waiting for a pattern to appear after a baseline line count.
pub struct WaitRequest {
    pub stream: UnixStream,
    pub pane_id: u32,
    pub pattern: String,
    pub start: Instant,
    pub timeout_secs: u32,
    pub baseline_lines: usize,
}

/// One attached viewer and the stream frames are written to.
pub struct Observer {
    pub id: ClientId,
    pub stream: UnixStream,
    pub observe_only: bool,
    pub connected_at: Instant,
}

/// An exec that reports by event instead of by reply: pattern, exit or timeout.
pub struct NotifyRequest {
    pub task_id: String,
    pub pane_id: u32,
    pub notify_pattern: String,

    pub deadline: Option<std::time::Instant>,
    pub kill_on_timeout: bool,
    pub draining: bool,
    pub exit_status: Option<i32>,
}

/// The listening control socket, its viewers, and every request in flight.
pub struct Server {
    pub socket_path: PathBuf,
    listener: UnixListener,
    observers: BTreeMap<ClientId, Observer>,
    next_client_id: AtomicU32,
    pub lock: LockState,
    pub pending_actions: Vec<ApiAction>,
    pub waiting_clients: Vec<WaitRequest>,
    pub run_requests: Vec<crate::capture::RunRequest>,
    pub watch_requests: Vec<crate::capture::WatchRequest>,
    pub notify_requests: Vec<NotifyRequest>,
    pub streams: StreamRegistry,
    new_viewer: bool,
}

const PROTO_FRAME: u8 = b'F';
const PROTO_INPUT: u8 = b'I';
const PROTO_RESIZE: u8 = b'R';
const PROTO_ATTACH: u8 = b'A';

impl Server {
    /// Bind the named session's socket and build a server on it.
    pub fn start_named(name: &str) -> io::Result<Self> {
        let (socket_path, listener) = Self::bind_listener(name)?;
        Ok(Self::from_bound(socket_path, listener))
    }

    /// Create the session directory and bind its control socket, non-blocking.
    ///
    /// Fails when a live server already answers on that socket.
    pub fn bind_listener(name: &str) -> io::Result<(PathBuf, UnixListener)> {
        let dir = PathBuf::from(format!("/tmp/vibe-term-{name}"));
        std::fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let socket_path = dir.join("control.sock");
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err(io::Error::other(format!(
                    "session '{name}' already running (socket alive)"
                )));
            }
            std::fs::remove_file(&socket_path)?;
        }
        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        Ok((socket_path, listener))
    }

    /// Build a server around a listener that was bound earlier.
    pub fn from_bound(socket_path: PathBuf, listener: UnixListener) -> Self {
        Self {
            socket_path,
            listener,
            observers: BTreeMap::new(),
            next_client_id: AtomicU32::new(1),
            lock: LockState::new(),
            pending_actions: Vec::new(),
            waiting_clients: Vec::new(),
            run_requests: Vec::new(),
            watch_requests: Vec::new(),
            notify_requests: Vec::new(),
            streams: StreamRegistry::new(),
            new_viewer: false,
        }
    }

    /// Accept at most one connection: a viewer attach or a queued request.
    pub fn poll_connections(&mut self, session: &crate::session::Session) {
        match self.listener.accept() {
            Ok((mut stream, _)) => {
                stream.set_nonblocking(false).ok();
                stream
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .ok();
                let mut header = [0u8; 1];
                match stream.read_exact(&mut header) {
                    Ok(()) if header[0] == PROTO_ATTACH => {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(5)));
                        let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                        let id = self.next_client_id.fetch_add(1, Ordering::Relaxed);
                        // why: the first attacher takes the lock; later ones only observe.
                        self.lock.try_take(id);
                        self.observers.insert(
                            id,
                            Observer {
                                id,
                                stream,
                                observe_only: false,
                                connected_at: Instant::now(),
                            },
                        );
                        self.new_viewer = true;
                    }
                    _ => match read_request_bytes(&mut stream, header[0]) {
                        Ok(all) => {
                            let request = String::from_utf8_lossy(&all).trim().to_string();
                            let action =
                                crate::capture::parse_api_request(&request, session, stream);
                            self.pending_actions.push(action);
                        }
                        Err(ReadRequestError::TooLarge) => {
                            drain_remaining(&mut stream);
                            let body = format!(
                                "{{\"ok\":false,\"error\":\"body_too_large\",\"limit_bytes\":{MAX_REQUEST_BYTES}}}\n"
                            );
                            let _ = stream.write_all(body.as_bytes());
                            let _ = stream.flush();
                        }
                        Err(ReadRequestError::Io) => {}
                    },
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => {}
        }
    }

    /// Take every request decoded since the last call.
    pub fn drain_actions(&mut self) -> Vec<ApiAction> {
        std::mem::take(&mut self.pending_actions)
    }

    /// Answer each wait request whose pane exited, timed out or matched.
    pub fn check_wait_clients(&mut self, session: &crate::session::Session) {
        let mut i = 0;
        while i < self.waiting_clients.len() {
            let req = &self.waiting_clients[i];
            let elapsed = req.start.elapsed().as_secs() as u32;

            let pane = session.pane_by_id(req.pane_id);
            if pane.is_none() || !pane.unwrap().is_alive() {
                let mut req = self.waiting_clients.remove(i);
                let _ = req.stream.write_all(b"{\"exited\":true}\n");
                continue;
            }

            if elapsed >= req.timeout_secs {
                let mut req = self.waiting_clients.remove(i);
                let _ = req.stream.write_all(b"{\"timeout\":true}\n");
                continue;
            }

            let matched =
                session.check_pattern_after(req.pane_id, &req.pattern, req.baseline_lines);
            if let Some(line) = matched {
                let mut req = self.waiting_clients.remove(i);
                let escaped = line.replace('\\', "\\\\").replace('"', "'");
                let msg = format!("{{\"matched\":\"{escaped}\"}}\n");
                let _ = req.stream.write_all(msg.as_bytes());
                continue;
            }

            i += 1;
        }
    }

    /// Send one screen frame to every viewer, dropping any that fail.
    pub fn send_frame(&mut self, data: &[u8]) {
        let mut packet = Vec::with_capacity(1 + 4 + data.len());
        packet.push(PROTO_FRAME);
        packet.extend_from_slice(&(data.len() as u32).to_le_bytes());
        packet.extend_from_slice(data);
        let mut dead: Vec<ClientId> = Vec::new();
        for (id, ob) in self.observers.iter_mut() {
            if ob.stream.write_all(&packet).is_err() || ob.stream.flush().is_err() {
                dead.push(*id);
            }
        }
        for id in dead {
            self.drop_observer(id);
        }
    }

    /// Collect viewer input: the lock holder's keys, or any viewer's resize.
    pub fn recv_input(&mut self) -> Option<Vec<u8>> {
        let mut dead: Vec<ClientId> = Vec::new();
        let mut result: Option<Vec<u8>> = None;
        let holder = self.lock.holder();
        for (id, ob) in self.observers.iter_mut() {
            let mut header = [0u8; 1];
            match ob.stream.read_exact(&mut header) {
                Ok(()) if header[0] == PROTO_INPUT => {
                    let mut len_buf = [0u8; 4];
                    if ob.stream.read_exact(&mut len_buf).is_err() {
                        dead.push(*id);
                        continue;
                    }
                    let len = u32::from_le_bytes(len_buf) as usize;
                    let mut data = vec![0u8; len];
                    if ob.stream.read_exact(&mut data).is_err() {
                        dead.push(*id);
                        continue;
                    }
                    if *id == holder && !ob.observe_only {
                        self.lock.record_input(*id);
                        if result.is_none() {
                            result = Some(data);
                        }
                    }
                }
                Ok(()) if header[0] == PROTO_RESIZE => {
                    let mut size_buf = [0u8; 4];
                    if ob.stream.read_exact(&mut size_buf).is_err() {
                        dead.push(*id);
                        continue;
                    }
                    if result.is_none() {
                        result = Some(vec![
                            PROTO_RESIZE,
                            size_buf[0],
                            size_buf[1],
                            size_buf[2],
                            size_buf[3],
                        ]);
                    }
                }
                Err(ref e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut => {}
                _ => {
                    dead.push(*id);
                }
            }
        }
        for id in dead {
            self.drop_observer(id);
        }
        result
    }

    fn drop_observer(&mut self, id: ClientId) {
        self.observers.remove(&id);
        if self.lock.holder() == id {
            self.lock.try_release(id);
        }
    }

    /// True while at least one viewer is attached.
    pub fn has_display_client(&self) -> bool {
        !self.observers.is_empty()
    }

    /// Drop every viewer and release the lock if one of them held it.
    pub fn disconnect_viewers(&mut self) {
        self.observers.clear();
        let holder = self.lock.holder();
        if holder != 0 {
            self.lock.try_release(holder);
        }
    }

    /// True once after a viewer attaches; reading it clears the flag.
    pub fn has_new_viewer(&mut self) -> bool {
        let v = self.new_viewer;
        self.new_viewer = false;
        v
    }

    /// How many viewers are attached.
    pub fn observer_count(&self) -> usize {
        self.observers.len()
    }

    /// The listening socket's descriptor, for the poll loop.
    pub fn listener_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        use std::os::fd::AsFd;
        self.listener.as_fd()
    }

    /// The directory holding the control socket.
    pub fn session_dir(&self) -> &std::path::Path {
        self.socket_path
            .parent()
            .unwrap_or(std::path::Path::new("/tmp"))
    }

    /// The control socket path as a string, empty if it is not utf-8.
    pub fn socket_path_str(&self) -> &str {
        self.socket_path.to_str().unwrap_or("")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

#[cfg(target_os = "linux")]
fn get_peer_sid(stream: &UnixStream) -> Option<i32> {
    let fd = stream.as_raw_fd();
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 || cred.pid == 0 {
        return None;
    }
    let stat_path = format!("/proc/{}/stat", cred.pid);
    let content = std::fs::read_to_string(&stat_path).ok()?;
    let after_paren = content.rsplit(')').next()?;
    let fields: Vec<&str> = after_paren.split_whitespace().collect();
    fields.get(3).and_then(|s| s.parse::<i32>().ok())
}

#[cfg(target_os = "macos")]
fn get_peer_sid(stream: &UnixStream) -> Option<i32> {
    let fd = stream.as_raw_fd();
    let mut peer_pid: libc::pid_t = 0;
    let mut len = std::mem::size_of::<libc::pid_t>() as libc::socklen_t;
    // why: level 0 is SOL_LOCAL and option 5 is LOCAL_PEEREPID on macOS.
    let ret = unsafe {
        libc::getsockopt(
            fd,
            0,
            5,
            &mut peer_pid as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if ret != 0 || peer_pid == 0 {
        return None;
    }
    let sid = unsafe { libc::getsid(peer_pid) };
    if sid < 0 { None } else { Some(sid) }
}

/// True when the caller runs inside the pane it is addressing.
pub fn check_sid_protection(
    stream: &UnixStream,
    pane_id: u32,
    session: &crate::session::Session,
) -> bool {
    let sid = match get_peer_sid(stream) {
        Some(s) => s,
        None => return false,
    };
    if let Some(pane) = session.pane_by_id(pane_id) {
        pane.pty_opt()
            .map(|pty| sid == pty.child_pid.as_raw())
            .unwrap_or(false)
    } else {
        false
    }
}

/// Connect to a session's socket as a viewer.
pub fn connect_as_display(socket_path: &str) -> io::Result<UnixStream> {
    let mut stream = UnixStream::connect(socket_path)?;
    stream.write_all(&[PROTO_ATTACH])?;
    stream.flush()?;
    Ok(stream)
}

/// Send typed bytes from a viewer to the server.
pub fn send_input(stream: &mut UnixStream, data: &[u8]) -> io::Result<()> {
    let header = [PROTO_INPUT];
    let len = (data.len() as u32).to_le_bytes();
    stream.write_all(&header)?;
    stream.write_all(&len)?;
    stream.write_all(data)?;
    stream.flush()
}

/// Send a viewer's terminal size to the server.
pub fn send_resize(stream: &mut UnixStream, cols: u16, rows: u16) -> io::Result<()> {
    let header = [PROTO_RESIZE];
    let size = [
        (cols & 0xFF) as u8,
        (cols >> 8) as u8,
        (rows & 0xFF) as u8,
        (rows >> 8) as u8,
    ];
    stream.write_all(&header)?;
    stream.write_all(&size)?;
    stream.flush()
}

fn drain_remaining(stream: &mut UnixStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
    let mut sink = [0u8; 4096];
    let mut drained = 0usize;
    while drained < 4 * MAX_REQUEST_BYTES {
        match stream.read(&mut sink) {
            Ok(0) => break,
            Ok(n) => drained += n,
            Err(_) => break,
        }
    }
}

/// Why a request body could not be read.
pub enum ReadRequestError {
    TooLarge,
    Io,
}

/// Read one request line, given its already consumed first byte.
///
/// Stops at a newline, end of stream, a read timeout, or the size ceiling.
pub fn read_request_bytes(stream: &mut UnixStream, first: u8) -> Result<Vec<u8>, ReadRequestError> {
    let mut all = Vec::with_capacity(4096);
    all.push(first);
    let mut chunk = [0u8; 4096];
    loop {
        if all.contains(&b'\n') {
            break;
        }
        if all.len() >= MAX_REQUEST_BYTES {
            return Err(ReadRequestError::TooLarge);
        }
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if all.len() + n > MAX_REQUEST_BYTES {
                    return Err(ReadRequestError::TooLarge);
                }
                all.extend_from_slice(&chunk[..n]);
            }
            Err(ref e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(_) => return Err(ReadRequestError::Io),
        }
    }
    Ok(all)
}

/// Read one screen frame sent by the server to a viewer.
pub fn recv_frame(stream: &mut UnixStream) -> io::Result<Vec<u8>> {
    let mut header = [0u8; 1];
    stream.read_exact(&mut header)?;
    if header[0] != PROTO_FRAME {
        return Err(io::Error::other("unexpected protocol byte"));
    }
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut data = vec![0u8; len];
    stream.read_exact(&mut data)?;
    Ok(data)
}

#[cfg(test)]
#[path = "tests/server.rs"]
mod tests;
