// Copyright (C) 2026 harry-ma-qiang
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The `vibe-term` command: start and stop a detached session, and send it
//! the seven verbs.

use std::io;
use std::path::PathBuf;
use vibe_term::{capture, daemon, subcmd};

const VERBS: [&str; 7] = ["exec", "serve", "read", "send", "wait", "status", "close"];

const HELP: &str = "\
vibe-term: a terminal multiplexer driven by a program

usage: vibe-term <command> [--name NAME] [args...]

  start [--name N] [-- CMD...]   start a detached session (default CMD: a shell)
  stop  [--name N]               stop a session and remove its files
  exec  [--vt-timeout S] 'CMD'   run shell CMD in a pane, return output and exit code
  serve [--vt-timeout S] 'CMD' PATTERN
                                 start CMD, return once PATTERN is printed
  read  PANE                     print a pane's text, live or ended
  send  PANE command|text|key V  type V (command adds Enter; key: ctrl-c, tab...)
  wait  PANE PATTERN             block until PATTERN appears in new output
  status                         list every pane
  close PANE                     stop a pane; read still returns its text

Without --name the session is taken from VIBE_TERM_SOCKET, which a pane's
programs inherit, else the first running session found. Replies are JSON.
";

fn main() {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("vibe-term: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn run(args: &[String]) -> io::Result<i32> {
    let Some(cmd) = args.first() else {
        eprint!("{HELP}");
        return Ok(2);
    };
    if matches!(cmd.as_str(), "-h" | "--help" | "help") {
        print!("{HELP}");
        return Ok(0);
    }
    let (name, rest) = take_name(&args[1..])?;
    match cmd.as_str() {
        "start" => start(name, rest),
        "stop" => stop(name, rest),
        verb if VERBS.contains(&verb) => send(name, verb, rest),
        other => Err(io::Error::other(format!(
            "unknown command '{other}' (try --help)"
        ))),
    }
}

fn take_name(args: &[String]) -> io::Result<(Option<String>, &[String])> {
    let (name, rest) = match args.first().map(String::as_str) {
        Some("--name") => match args.get(1) {
            Some(n) => (Some(n.clone()), &args[2..]),
            None => return Err(io::Error::other("--name needs a value")),
        },
        Some(a) if a.starts_with("--name=") => (Some(a["--name=".len()..].to_string()), &args[1..]),
        _ => (None, args),
    };
    if let Some(ref n) = name {
        check_name(n)?;
    }
    Ok((name, rest))
}

fn check_name(name: &str) -> io::Result<()> {
    let ok = !name.is_empty()
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if ok {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "bad session name '{name}': use letters, digits, '-', '_' or '.'"
        )))
    }
}

fn start(name: Option<String>, rest: &[String]) -> io::Result<i32> {
    let command = match rest.split_first() {
        None => vec![daemon::default_shell().to_string()],
        Some((dd, cmd)) if dd == "--" && !cmd.is_empty() => cmd.to_vec(),
        _ => {
            return Err(io::Error::other(
                "usage: vibe-term start [--name N] [-- CMD...]",
            ));
        }
    };
    let name = name.unwrap_or_else(capture::generate_session_name);
    let mut start_args = vec![
        name.clone(),
        "--clean-session".to_string(),
        "--".to_string(),
    ];
    start_args.extend(command);
    daemon::run_start(&start_args)?;
    let dir = PathBuf::from(format!("/tmp/vibe-term-{name}"));
    match vibe_term::lifecycle::daemonize::read_pid_file(&dir) {
        Ok((pid, socket, _)) => println!(
            "started session:{name} pid:{pid} socket:{}",
            socket.display()
        ),
        Err(_) => println!("started session:{name}"),
    }
    Ok(0)
}

fn stop(name: Option<String>, rest: &[String]) -> io::Result<i32> {
    let target = match (name, rest) {
        (Some(n), []) => n,
        (None, [n]) => {
            check_name(n)?;
            n.clone()
        }
        _ => return Err(io::Error::other("usage: vibe-term stop --name N")),
    };
    subcmd::cmd_stop(&target)?;
    Ok(0)
}

fn send(name: Option<String>, verb: &str, rest: &[String]) -> io::Result<i32> {
    if let Some(n) = name {
        let sock = subcmd::resolve_socket(Some(&n))?;
        // why: run_api_client_capture reads the target from this variable.
        unsafe {
            std::env::set_var("VIBE_TERM_SOCKET", sock);
        }
    }
    let mut request = vec![verb.to_string()];
    request.extend_from_slice(rest);
    let reply = subcmd::run_api_client_capture(&request)?;
    print!("{reply}");
    Ok(if is_error_reply(&reply) { 1 } else { 0 })
}

fn is_error_reply(reply: &str) -> bool {
    reply
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| serde_json::from_str::<serde_json::Value>(l.trim()).ok())
        .is_some_and(|v| v.get("error").is_some())
}

#[cfg(test)]
#[path = "tests/main.rs"]
mod tests;
