# req: a finished pane's output is kept in memory

## The requirement

Given a pane whose child has exited, when the session reaps the pane and a
caller then reads that pane id, the output the pane had captured is returned,
with the same line count and the same `max_lines` trimming as a read of a live
pane. Nothing is written to disk to make that possible, and nothing is read
back from disk to answer it.

## Why this is written down

The library used to answer such a read from a file: one log per pane under
`<session_dir>/panes/<pane_id>.log`, written while the pane ran. That file
existed only because reaping a pane destroyed its screen and its scrollback
before anybody could ask for the text, so the text had to be somewhere else
first. A `BufWriter` per pane, a cursor into the scrollback, a directory and a
path were all downstream of that one decision.

Keeping the text instead removes the reason for the file. A reader who finds
no persistence here should not read it as an oversight, or add a log back: the
output is not thrown away, so there is nothing for a file to rescue.

## What is kept, and how much

`Session::remember_finished` captures `Pane::capture_full_text()` before the
pane is dropped and pushes `(id, text)` onto `Session::finished`. The list is
bounded by `MAX_FINISHED_PANES` in `src/session.rs`, which is 32: at the cap
the oldest entry is dropped to make room. A pane read after its entry has been
evicted fails exactly as an unknown pane id does, with `pane not found: <id>`.

Only reaping remembers a pane. A pane closed on request, by `close_pane` or by
`kill_service_pane`, is not kept, because the caller asked for it to go.

## What the library writes

No file, of its own accord, ever. Two writes remain and both are a caller
naming a path: the session config, written when a program calls
`init_session_dir` and stored as `config.json` in that directory, and a
recording, written when a program calls `setup_recorder` with a cast path. A
program that calls neither leaves the filesystem untouched.

## How to see it

    cargo test session_api::tests::a_reaped_pane_leaves_no_file_in_the_session_directory

The test gives a session a directory, lets a sidebar command finish, reaps it,
and asserts the directory holds `config.json` and nothing else.

Settled by: `reading_a_reaped_pane_returns_the_output_it_had`,
`read_pane_honours_max_lines_on_a_reaped_pane`,
`the_finished_list_evicts_the_oldest_pane_at_the_cap`,
`reading_an_unknown_pane_id_is_an_error` and
`a_reaped_pane_leaves_no_file_in_the_session_directory`
(all in `src/tests/session_api.rs`).
