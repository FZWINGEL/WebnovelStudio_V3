# Bug: `add-many` silently discards the `links` key of a batch entry

**pm version:** 0.41.0
**Engine:** `scripts/conductor.mjs`
**Severity:** silent data loss — the batch reports success, no error is raised, and `integrity` reports 0 findings
**Date:** 2026-09-11

## Summary

`add-many --help` advertises `links` as a supported key of a batch entry:

> Each entry in the `--from` document may carry: id, title, lane, priority, status, parent,
> externalId, externalUrl, planPath, specPath, **links**, description, externalUpdatedAt, stories.

It does not state the required shape. Both spellings a user would naturally write are accepted
without complaint, and neither produces a working link:

| Form passed in the batch entry | Value stored in `state.json` | Effect |
|---|---|---|
| `"links": ["depends-on:<epic>"]` | `[]` | **silently dropped** — no error, no warning |
| `"links": "depends-on:<epic>"` | `[]` | **silently dropped** |
| `"links": [{"type":"depends-on","target":"<epic>"}]` | `[{"type":"depends-on","target":"<epic>"}]` | **stored but inert** — see below |
| *(not a batch form)* `update-epic <id> --link "depends-on:<epic>"` | `[{"type":"depends-on","epic":"<epic>"}]` | works |

## The two distinct defects

**1. String forms are dropped without a word.** The documented CLI syntax for `--link` is
`<type>:<epic>[:<reason>]`. A user batch-creating epics by mirroring that syntax into the JSON —
the obvious thing to do — loses every relationship. Nothing in the output says so.

**2. The object form writes a key no reader uses.** `add-many` persists the target under
`target`; `update-epic --link` persists it under `epic`. Every engine reader keys on `epic`, so
an `add-many`-written link is structurally invisible:

- it renders as `-` in `PROJECT.md`'s **Links** column
- it does **not** trigger the `## Dependency warnings` block for `depends-on`
- `integrity` reports **0 findings** — `link-of-unknown-type` passes (`depends-on` is a known
  type) and `dangling-epic-reference` passes (the epic exists; the target is simply never read)

So the record claims a relationship that the conductor cannot see, and the one command whose job
is to report records that cannot be true reports nothing.

## Reproduction

```bash
E=scripts/conductor.mjs   # pm 0.41.0

# 1. A target epic to link to
node "$E" add-epic --id zz-target --title "target" --lane claude-code --priority P3 --status later

# 2. Batch-create a dependent, passing links in the natural string form
node "$E" add-many --from - <<'JSON'
{"epics":[{"id":"zz-dep","title":"dep","lane":"claude-code","priority":"P3","status":"queued",
           "links":["depends-on:zz-target"]}]}
JSON

# -> "add-many added 1 epic(s)"   (reports success)
node -e "console.log(require('./.conductor/state.json').epics.find(e=>e.id==='zz-dep').links)"
# -> []            <-- link gone, no error

# 3. Now the object form
node "$E" add-many --from - <<'JSON'
{"epics":[{"id":"zz-dep2","title":"dep2","lane":"claude-code","priority":"P3","status":"queued",
           "links":[{"type":"depends-on","target":"zz-target"}]}]}
JSON
node -e "console.log(require('./.conductor/state.json').epics.find(e=>e.id==='zz-dep2').links)"
# -> [{"type":"depends-on","target":"zz-target"}]   <-- stored, key name 'target'

node "$E" integrity | grep -i "link\|dangling"
# -> both report 0 finding(s)

grep -n zz-dep2 PROJECT.md
# -> the Links column reads '-'   <-- invisible
```

## How it was hit

Registering a five-epic backlog in one `add-many` batch. One epic carried
`"links": ["depends-on:<epic>"]`. The batch returned `added 5 epic(s)`. The link did not exist.
It was caught only by eyeballing the rendered `PROJECT.md` **Links** column and noticing `-` on
an epic that was supposed to have a dependency — not by any error, and not by `integrity`.

Recording the same link correctly afterwards required knowing that `update-epic --link` writes
`epic` while `add-many` writes `target`, which is discoverable only by running both and diffing
`state.json` by hand.

## Suggested fixes

At minimum, one of these; ideally the first two:

1. **Reject the string forms loudly.** `add-many` already validates entry keys and reports
   `unsupported key(s) <k> (supported: ...)` — a helpful precedent. A `links` value that is not
   an array of `{type, epic|target}` objects should be refused the same way rather than dropped.
2. **Accept the documented CLI syntax.** Parse `"<type>:<epic>[:<reason>]"` exactly as
   `--link` does — including when passed as a bare string. This is what a user will write.
3. **Make the two key names agree.** Normalise `target` → `epic` on write (or teach the readers
   both), so an `add-many` link is not inert.
4. **Blast radius worth auditing.** A malformed link is a class of record that cannot be true.
   If `integrity` can detect a link whose target key is not the one readers use, this becomes
   self-reporting rather than something a user must notice visually.

## Note

This report was written to `.conductor/feedback/` per step 2 of `commands/feedback.md` and was
**not** filed upstream — no `gh issue create` was run, and no near-duplicate search was
performed. A maintainer should check open issues on `cfdude/pm` before treating this as new.
