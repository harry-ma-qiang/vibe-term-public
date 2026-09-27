# bug: a finished command is never marked finished — resolved

This record was open while the library was being ported, and it was wrong once
before it was right. It is kept because the history is the useful part.

## What it first claimed

That nothing set a pane's `completed` flag to `true`, and that
`Session::reap_dead_panes` was the missing writer. Both halves were false:
`Session::mark_pane_completed` in `src/session_api.rs` was the writer all along,
and `reap_dead_panes` only reads the flag.

## What was actually missing

A caller. Nothing drove `mark_pane_completed` when a managed child exited,
because the code that runs `exec` and `serve` had not been ported.

## What resolved it

Porting the command runners. `src/commands.rs` now marks a pane finished when
its child exits (line 194, with the real exit code) and when a run times out
(line 235, with `-1`). An `exec` through the binary returns the command's exit
code:

    vibe-term start --name demo
    vibe-term exec --name demo 'echo done; exit 7'
    # {"exit_code":7,"output":"done", ...}

## What is still true

`cargo run --example run_a_command` drives a bare `Pane` with no `Session`, so it
still prints `completed=false`. That is correct: marking a pane finished is a
`Session` concern, and the example deliberately goes underneath it.

Settled by: `close_pane_if_idle_closes_a_completed_pane` (`src/tests/session.rs`)
for the flag, and the `exec` run in
`docs/test-a-started-session-detaches-and-keeps-working.md` for the driver.
