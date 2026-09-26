// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Starting a session as a detached daemon: session lock, socket bind, double
//! fork, then building the session and running the headless loop.

use crate::cli::Cli;
use crate::daemon_loop::headless_loop;
use crate::filter::SecretFilter;
use crate::grid::SidebarSide;
use crate::lifecycle::daemonize::{self, DaemonHandle, DaemonStatus, Either, GrandchildOutcome};
use crate::lifecycle::session_lock::{self, AcquireErr};
use crate::lifecycle::signals::install_daemon_signal_handlers;
use crate::lifecycle::time_fmt;
use crate::server;
use crate::session::Session;
use crate::terminal::CHILD_PID;
use clap::Parser;
use crossterm::terminal;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;

enum DaemonOutcome {
    Started,
    AlreadyRunning,
}

/// Start a detached daemon from `[NAME] [OPTIONS] [-- CMD...]`; returns once it is ready.
pub fn run_start(args: &[String]) -> io::Result<()> {
    let (name, rest) = if args
        .first()
        .map(|s| s.starts_with('-') || s == "--")
        .unwrap_or(true)
    {
        (None, args)
    } else {
        (Some(args[0].clone()), &args[1..])
    };
    let mut cli_args = vec!["vibe-term".to_string(), "--daemon".to_string()];
    if let Some(ref n) = name {
        cli_args.push("--name".to_string());
        cli_args.push(n.clone());
    }
    cli_args.extend_from_slice(rest);
    let cli = Cli::parse_from(&cli_args);
    let captured_name = cli.name.clone();
    match run_daemon(cli)? {
        DaemonOutcome::Started => Ok(()),
        DaemonOutcome::AlreadyRunning => {
            let label = captured_name.unwrap_or_else(|| "<unknown>".to_string());
            Err(io::Error::other(format!(
                "session '{label}' already exists"
            )))
        }
    }
}

fn run_daemon(mut cli: Cli) -> io::Result<DaemonOutcome> {
    if cli.name.is_none() {
        cli.name = Some(crate::capture::generate_session_name());
    }
    let name = cli.name.clone().expect("name set above");
    let session_dir = PathBuf::from(format!("/tmp/vibe-term-{name}"));
    let invocation_cwd = std::env::current_dir().ok();

    std::fs::create_dir_all(&session_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&session_dir, std::fs::Permissions::from_mode(0o700));
    }
    let lock = match session_lock::try_acquire_session_lock(&session_dir) {
        Ok(l) => l,
        Err(AcquireErr::AlreadyHeld) => return Ok(DaemonOutcome::AlreadyRunning),
        Err(AcquireErr::Io(e)) => {
            eprintln!("vibe-term: session lock open failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = session_lock::scrub_stale_dir(&lock, &session_dir) {
        eprintln!("vibe-term: scrub_stale_dir: {e}");
    }

    let (socket_path, ctrl_listener) = match server::Server::bind_listener(&name) {
        Ok(v) => v,
        Err(e) => {
            drop(lock);
            let _ = std::fs::remove_dir_all(&session_dir);
            eprintln!("vibe-term: bind failed: control socket: {e}");
            std::process::exit(1);
        }
    };
    // why: panes' children reach their daemon through this; it survives the fork.
    unsafe {
        std::env::set_var("VIBE_TERM_SOCKET", socket_path.to_str().unwrap_or_default());
    }

    match daemonize::double_fork(&session_dir, invocation_cwd.as_deref())? {
        Either::Left(DaemonStatus::Ready(_)) => Ok(DaemonOutcome::Started),
        Either::Left(DaemonStatus::BindFailed) => Err(io::Error::other(
            "daemon failed to bind control socket (FAIL beacon)",
        )),
        Either::Right(GrandchildOutcome::BecomeDaemon { ready_writer, .. }) => {
            // why: the flock is released only when this grandchild drops it on exit.
            let _lock = lock;
            install_daemon_signal_handlers();
            let srv = server::Server::from_bound(socket_path, ctrl_listener);
            match init_session(&cli, srv) {
                Ok((mut session, mut srv)) => {
                    let handle = DaemonHandle {
                        pid: std::process::id(),
                        socket: PathBuf::from(srv.socket_path_str()),
                        start_time_unix_secs: time_fmt::now_unix_secs(),
                    };
                    if let Err(e) = daemonize::write_pid_file(&session_dir, &handle) {
                        eprintln!("vibe-term: daemon.pid write failed: {e}");
                    }
                    let _ = ready_writer.signal_ready();
                    let loop_result = headless_loop(&mut session, &mut srv);
                    session.kill_all();
                    session.cleanup_session_dir();
                    loop_result.map(|()| DaemonOutcome::Started)
                }
                Err(e) => {
                    let _ = ready_writer.signal_fail();
                    Err(e)
                }
            }
        }
    }
}

pub(crate) fn init_session(
    cli: &Cli,
    srv: server::Server,
) -> io::Result<(Session, server::Server)> {
    let name = cli
        .name
        .clone()
        .unwrap_or_else(crate::capture::generate_session_name);
    let mut session = build_session(cli)?;
    session.name = name;
    session.limits.mem_limit_mb = cli.mem_limit;
    session.limits.cpu_limit_pct = cli.cpu_limit;
    session.limits.restart_delay_secs = cli.restart_delay;
    session.clean_session = cli.clean_session;
    session.init_session_dir(srv.session_dir())?;
    let config_dir = session.session_dir.clone();
    if let Some(ref dir) = config_dir {
        let _ = session.load_config(dir);
    }
    Ok((session, srv))
}

/// The per-user session name for `key`: `$USER-key`, else `whoami`, else the uid.
pub fn default_session_name(key: &str) -> String {
    let user = std::env::var("USER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::process::Command::new("whoami")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| format!("uid-{}", unsafe { libc::getuid() }));
    format!("{user}-{key}")
}

/// The shell a session runs when given no command: zsh on macOS, else bash.
pub fn default_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "zsh"
    } else {
        "bash"
    }
}

pub(crate) fn build_session(cli: &Cli) -> io::Result<Session> {
    let (cols, rows) = terminal::size().unwrap_or((187, 34));
    let mut session = Session::new(cols as usize, rows as usize);
    let slot_cmds: Vec<&str> = [&cli.slot1, &cli.slot2, &cli.slot3, &cli.slot4]
        .iter()
        .filter_map(|s| s.as_deref())
        .collect();
    session.sidebar_count = slot_cmds.len();
    session.sidebar_side = match cli.side.as_str() {
        "right" => SidebarSide::Right,
        _ => SidebarSide::Left,
    };
    let mut filter = SecretFilter::new();
    filter.load_bundled_patterns();
    filter.load_user_literals()?;
    filter.load_env_identity();
    if let Some(ref path) = cli.filter {
        filter.load_dotenv(path)?;
    }
    if let Some(ref path) = cli.filter_blocklist {
        filter.load_blocklist(path)?;
    }
    if !cli.command.is_empty() {
        let cmd = &cli.command[0];
        let args: Vec<&str> = cli.command[1..].iter().map(|s| s.as_str()).collect();
        session.add_main(cmd, &args, filter)?;
    }
    for slot_cmd in &slot_cmds {
        let parts: Vec<&str> = slot_cmd.split_whitespace().collect();
        let shell = default_shell();
        let (scmd, sargs) = parts.split_first().unwrap_or((&shell, &[]));
        session.add_sidebar_pane(scmd, sargs)?;
    }
    session.relayout()?;
    if let Some(pane) = session.panes.first()
        && let Some(pty) = pane.pty_opt()
    {
        CHILD_PID.store(pty.child_pid.as_raw(), Ordering::Relaxed);
    }
    Ok(session)
}
