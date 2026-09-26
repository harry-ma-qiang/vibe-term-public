# vibe-term

A terminal multiplexer built to be driven by a program, not only by a person.

The thing it is for: start a long job, detach, let it run unattended, and come back
later and read what it printed. `screen` for something that is not sitting there
watching.

## State: a partial port, and honest about it

This is being lifted out of a larger private tool one layer at a time. What is here
is a **library**, not yet a binary. It builds, its tests pass, and two examples run
real work through it, but you cannot yet type `vibe-term` at a shell.

```
cargo build                                # zero warnings
cargo test                                 # 269 tests
cargo run --example smoke                  # what the emulator renders
cargo run --example run_a_command          # a real command on a real pty
```

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

Roughly 6,500 lines, ported module by module, tests carried across with it.

## What is not here yet

The command line, the socket server, the request dispatcher, and the loop that ties
them together. Until those land there is no `exec`, no `serve`, no `attach`, and no
daemon.

One consequence is visible in the examples and worth knowing before you read the
code: **a command that has finished is never marked finished.** Capturing its output
works; noticing that it ended does not, because the code that notices also repaints
the screen and the rendering module is not ported. See
`docs/bug-a-finished-command-is-never-marked-finished.md`.

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
