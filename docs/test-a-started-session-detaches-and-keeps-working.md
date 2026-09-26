# test: a started session detaches, keeps working, and stops clean

## What this case claims

`vibe-term start` returns at once and leaves a daemon running with no
controlling terminal. That daemon answers all seven verbs from a different
process. `stop` ends it and leaves nothing behind. Nothing is written to a log
file at any point.

## How to run it

Build first, then run each command from the repository root. Pick a session
name of your own; `pubtest-l8` is the one used below. `env -u VIBE_TERM_SOCKET`
makes sure a command reaches only the session it names, never one it inherited
from the shell it was typed in.

    cargo build --release
    vt() { env -u VIBE_TERM_SOCKET ./target/release/vibe-term "$@"; }

The main pane runs a bare bash with a plain `$ ` prompt, so the record shows no
host or user name. `vibe-term start --name pubtest-l8` with no command runs bash
(zsh on macOS) with your usual prompt instead, and behaves the same.

## 1. start returns promptly: it really detached

    t0=$(date +%s%N)
    vt start --name pubtest-l8 -- env 'PS1=$ ' bash --norc --noprofile
    echo "[exit $?] returned after $(( ($(date +%s%N) - t0) / 1000000 )) ms"

    started session:pubtest-l8 pid:2473505 socket:/tmp/vibe-term-pubtest-l8/control.sock
    [exit 0] returned after 19 ms

Pass: exit 0, and the prompt came back in well under a second.

## 2. the daemon is alive and has no controlling terminal

    ps -o pid,ppid,sid,tty,stat,cmd -p 2473505
    ps -o pid,ppid,tty,stat,cmd --ppid 2473505

        PID    PPID     SID TT       STAT CMD
    2473505       1 2473504 ?        Sl   ./target/release/vibe-term start --name pubtest-l8 -- env PS1=$  bash --norc --noprofile
        PID    PPID TT       STAT CMD
    2473512 2473505 pts/2    Ss+  bash --norc --noprofile

Pass: TTY is `?`, the parent is pid 1, and the session id differs from the pid,
so the daemon is not a session leader and cannot acquire a terminal. Its pane's
shell owns a pty of its own.

## 3. exec runs a real command and returns its output and exit code

    vt exec --name pubtest-l8 'echo hello from exec; exit 3'

    {"exit_code":3,"hint":null,"output":"hello from exec","pane_id":1,"tail_shown":1,"timeout":false,"total_lines":1}

Pass. The client exits 0: it reports a reply, and the command's own code is
`exit_code`. The command is one shell string. Separate words are joined with
spaces before the shell sees them, so `exec sh -c 'echo hi'` loses its quoting.

## 4. serve, status, read, send, wait, close, and read after close

    vt serve --name pubtest-l8 --vt-timeout 20 'echo server ready; while read l; do echo got:$l; done' 'server ready'

    {"alive":true,"matched":"server ready","pane_id":2,"pid":2473521,"ready":true,"started":true,"timeout":false}

    vt status --name pubtest-l8

    {"browse":"disabled","filter":false,"focused":0,"panes":[{"alive":true,"cmd":"env PS1=$  bash --norc --noprofile","cols":48,"cpu":0.0,"kind":"pty","lines":["$"],"mem_mb":0.0,"pane":0,"pid":2473512,"rows":20,"state":"idle"},{"alive":false,"cmd":"echo hello from exec; exit 3","cols":28,"cpu":0.0,"kind":"pty","lines":["hello from exec"],"mem_mb":0.0,"pane":1,"pid":2473519,"rows":9,"state":"completed"},{"alive":true,"cmd":"echo server ready; while read l; do echo got:$l; done","cols":28,"cpu":0.0,"kind":"pty","lines":["server ready"],"mem_mb":0.0,"pane":2,"pid":2473521,"rows":9,"state":"idle"}],"ratio":"1:φ","session":"pubtest-l8","session_dir":"/tmp/vibe-term-pubtest-l8","sidebar_count":2,"total_panes":3}

    vt read --name pubtest-l8 2

    {"lines":1,"output":"server ready","pane":2,"total":1}

`wait` matches output that arrives after it starts, so it is started first and
the input is sent while it waits:

    ( vt wait --name pubtest-l8 2 got:ping; echo "[exit $?]" ) > wait.out &
    sleep 0.5
    vt send --name pubtest-l8 2 command ping
    wait; cat wait.out

    {"ok":true}
    {"matched":"got:ping"}
    [exit 0]

    vt close --name pubtest-l8 2

    {"ok":true}

    vt read --name pubtest-l8 2

    {"closed":true,"lines":3,"output":"server ready\nping\ngot:ping","pane":2,"total":3}

Pass: the pane that was closed still answers `read` with everything it printed.

## 6. nothing is written to a log file

Checked while the daemon is still running, since `stop` removes the directory:

    ls -A /tmp/vibe-term-pubtest-l8
    find /tmp/vibe-term-pubtest-l8 -name '*.log' | wc -l

    config.json
    control.sock
    daemon.lock
    daemon.pid
    0

Pass: four files, none of them a log.

## 5. stop ends the daemon and leaves nothing behind

    vt stop --name pubtest-l8
    ps -o pid,cmd -p 2473505; echo "[exit $?]"
    pgrep -f 'vibe-term start --name pubtest-l8' | wc -l
    ls -d /tmp/vibe-term-pubtest-l8; echo "[exit $?]"

    stopped pid:2473505
        PID CMD
    [exit 1]
    0
    ls: cannot access '/tmp/vibe-term-pubtest-l8': No such file or directory
    [exit 2]

Pass: the daemon and its panes are gone, and so is the session directory.
`start` asks for the directory to be removed on exit; a session started by the
library with `clean_session` left false keeps it.

## What would settle it

Settled by: nothing automatic yet. This record is run by hand against the
built binary, and every step above passed when it was written. The pieces it
exercises have their own tests: `a_reaped_pane_leaves_no_file_in_the_session_directory`
in `src/tests/session_api.rs`, and `a_closed_pane_stays_readable` in
`src/tests/capture.rs`.
