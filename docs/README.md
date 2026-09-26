# docs

Three kinds of thing are written here and no others. Each is one file, named by what it
is about, and holds one claim per heading.

| Kind | What it is | Settled by |
|---|---|---|
| `req` | A requirement, written as behaviour: given, when, then | a test that runs it |
| `test` | A case a test plan carries, in the shape a person would run it | the test passing |
| `bug` | A defect: what happens, what should, and how to see it | a test that would have caught it |

A `Settled by:` line names a test that exists in this repository, or says plainly that
nothing settles it yet. It never names a test that was not written.

There is no tool here for reading these. They are text, and a person opens them.

A file says what it claims and what would settle it. It does not say what was decided in
private, who decided it, or what else the author is building.
