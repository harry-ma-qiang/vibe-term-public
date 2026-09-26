# bug: a finished command is never marked finished

## What happens

`Pane::spawn` runs a command, `feed_output` collects what it writes, and
`capture_full_text` gives the text back. All of that works. But `is_completed`
returns `false` forever, even after the child has exited and been reaped by the
operating system.

Nothing in the ported code ever sets `completed` to `true`. Both constructors
initialise it to `false` (`src/pane.rs:187`, `src/pane.rs:225`) and no other
write exists. `is_idle` is affected too, because it reads `completed` first
(`src/pane.rs:86`).

Reproduce it with `cargo run --example run_a_command`: four commands run to
completion and every one of them reports `completed=false`.

## What should happen

A pane whose child has exited reports that it finished, and carries the exit
status in `completed_exit`.

The code that does this is `Session::reap_dead_panes`, which is not ported. It
walks the panes, finds the ones that are no longer alive, and marks them. It is
not here because its tail repaints the screen through a rendering module that is
also not ported, and a version with that tail removed would look correct while
silently leaving the display stale.

So this is not a defect in what was ported. It is the visible edge of what has
not been ported yet, and it is recorded because someone reading `is_completed`
would otherwise assume it is broken rather than absent.

## How to see it

    cargo run --example run_a_command

Every block prints `completed=false`. The captured text is correct in every one,
which is what makes the gap easy to miss.

Settled by: nothing. No test asserts it, because a test that did would have to
assert the absence of a module rather than a behaviour.
