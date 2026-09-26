# bug: a finished command is never marked finished

This record was wrong. It is kept, corrected, because the correction is the
interesting part: two separate things were run together under one claim.

## What the record used to claim

That nothing in the ported code ever sets `completed` to `true`, that both
constructors initialise it to `false` and no other write exists, and that
`Session::reap_dead_panes` is the code that would set it.

## What is actually true

`completed` does have a writer, and it arrived with the session layer:
`Session::mark_pane_completed` (`src/session_api.rs`) sets both `completed`
and `completed_exit`. A test already depends on it
(`close_pane_if_idle_closes_a_completed_pane` in `src/tests/session.rs`),
so the claim was already false before `reap_dead_panes` was ported.

`reap_dead_panes` does not set the flag either. It removes panes whose child
has exited, and it *reads* `completed` to skip the ones already accounted
for. Porting it did not, and could not, change this.

What is genuinely absent is the caller: nothing in the library calls
`mark_pane_completed` on its own. In the full program the exec and serve
driver does it when it reaps a child it is managing, and that driver is not
ported. So a program using this library has to mark a pane itself.

## Why the example still prints `completed=false`

`cargo run --example run_a_command` drives a bare `Pane` and never builds a
`Session`. Neither `mark_pane_completed` nor `reap_dead_panes` is a `Pane`
method, so no amount of session porting can change that output. It reports
`completed=false` for the same reason a value nobody assigns keeps its
initial value.

## What should happen

A caller that wants a finished command reported as finished holds a
`Session`, and calls `mark_pane_completed` with the exit status when the
child exits. The library offers the write; it does not perform it.

## How to see it

    cargo run --example run_a_command

Every block prints `completed=false`, and that is correct for a bare pane.

Settled by: `close_pane_if_idle_closes_a_completed_pane`
(`src/tests/session.rs`) settles that `completed` can be set and is read.
Nothing settles the absent driver, because there is nothing there to test.
