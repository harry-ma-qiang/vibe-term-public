# vibe-term

A terminal multiplexer built to be driven by a program, not only by a person.

The thing it is for: start a long job, detach, let it run unattended, and come back
later and read what it printed. `screen` for something that is not sitting there
watching.

## State: a partial port, and honest about it

This is being lifted out of a larger private tool one layer at a time. It is now a
binary with nine subcommands: `start`, `stop`, and the seven verbs `exec`, `serve`,
`read`, `send`, `wait`, `status` and `close`. `vibe-term --help` lists them.

```
cargo build                                # zero warnings
cargo test                                 # 415 tests
cargo run --example smoke                  # what the emulator renders
cargo run --example run_a_command          # a real command on a real pty
```

## Try it

```
cargo build --release
vt() { env -u VIBE_TERM_SOCKET ./target/release/vibe-term "$@"; }

vt start --name demo                        # returns at once; the session runs on
vt exec --name demo 'echo hi; exit 3'       # output and exit code, as JSON
vt serve --name demo 'echo ready; while read l; do echo got:$l; done' ready
vt status --name demo                       # every pane; the serve pane is id 2
vt wait --name demo 2 got:x &               # matches output that arrives later
vt send --name demo 2 command x
vt close --name demo 2
vt read --name demo 2                       # still answers after close
vt stop --name demo                         # daemon gone, directory removed
```

`env -u VIBE_TERM_SOCKET` matters when you run this from inside another vibe-term
pane: without `--name`, a command goes to the session in that variable. The whole
run, with its output, is in `docs/test-a-started-session-detaches-and-keeps-working.md`.

## What is here

```
pty          a child process on a real pty
emulator     a terminal that understands what the child writes
pane         one child, its screen, its frame, its stats
session      panes, layout, focus, lifecycle
frame grid   rectangles, splits, the sidebar
renderer     drawing a screen, whole or by difference
filter       redacting secrets out of output before anyone sees it
lock         who holds the session, and who is only watching
recorder     writing a session out as a cast file
service      long-lived panes supervised rather than run once
selection    mouse selection and copy
monitor      cpu and memory per pane
```

Roughly 11,000 lines, ported module by module, tests carried across with it.

## What is not here yet

`attach`: watching a running session on your own terminal and typing into it. The
daemon still accepts a viewer and paints for one, but there is no client that
connects as a viewer. Everything else is driven through the verbs above.

## Why the code looks the way it does

Much of this was debugged by a person watching a real terminal misbehave, so it was
copied here rather than rewritten, character for character wherever that was possible.
Several places look wrong and are not. `emulator.rs`'s `resize` only ever grows a row
and never shrinks one; that is a fix, and the test named for it says so.

If you are tempted to tidy something here, read the test names first.

## Redaction

`filter.rs` blanks secret-shaped text out of a pane before it is displayed, recorded
or read back. The shapes are built in. The literal words are not: put them in a file
and point `VIBE_TERM_FILTER_LITERALS` at it, or leave it and nothing extra is matched.
Nothing about any vendor, product or model is compiled in.

It catches shapes. It cannot catch a secret that has no shape.
