"""Migrate one crate's IPC types onto generated bindings.

    python scripts/add_bindings_group.py <group> <crate-dir-name>

Runs the whole Rust side in one pass: seed the derives from the crate's types
the frontend names, converge them against the compiler, apply the per-variant
`rename_all` attributes specta needs, generate, then close the group's name
list against `tsc`. Both lists come from the compiler rather than from a
person, which is what makes the pass repeatable.

What it does NOT do is the frontend tail. Replacing the mirrors is safe to
script; making the frontend compile against the result is judgement, because
every error so far has been the *mirror* being wrong rather than the
generation — a renamed enum, a field the wire carries as `null`, a fixture
built from fields Rust never sends. Do that part by hand, reading each error.

THE THING TO KNOW BEFORE STARTING, learned twice the hard way: the frontend's
`ipc/*.ts` types mirror the *Tauri command surface*, not the core crate types.
The shell declares its own DTOs and augments core's:

    DesktopMemoryRead { #[serde(flatten)] read: MemoryRead, pending_save, pending_job_ids }
    StartMemoryRequest { ..., model_selection, maintenance_revision }
    WorkerIssue / DiscussionView's worker_issues  (discussion_recovery.rs)

So a frontend type is generated from core only where the command passes a core
type through unchanged. Where the shell wraps, flattens or adds fields, the
frontend type belongs to the shell and replacing it with the generated core
type silently drops what the shell adds.

`SHELL_OWNED` below is that list. The mirror-replacement step must skip these
names: the generated file still declares core's version of each — it is a
faithful export of the crate — but the frontend keeps its own, because its own
is the one that matches the wire.

Two traps this script already avoids, both found by having them:

* `ensure_dep` must be given a *file* path, not a Rust path. Handing it
  `wns_story::memory::MemoryView` writes `wns-.. = { path = "../.." }` into
  `crates/bindings/Cargo.toml` and breaks the whole workspace.
* `defining` must scan `pub type` as well as `pub struct` and `pub enum`:
  twelve type aliases in `wns-context` came back "nothing addable" without it.
"""
import glob
import io
import os
import re
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CARGO = shutil.which('cargo') or os.path.expanduser('~/.cargo/bin/cargo.exe')
ENV = dict(os.environ, CARGO_TARGET_DIR='/d/WebnovelStudio_V3/target',
           PATH=os.path.expanduser('~/.cargo/bin') + ':' + os.environ.get('PATH', ''))
LIB = os.path.join(ROOT, 'crates/bindings/src/lib.rs')

GROUP = sys.argv[1]
CRATE = sys.argv[2].replace('-', '_')

# Frontend types that mirror a command DTO rather than a core type. Skipped by
# the mirror-replacement step; see the module docstring.
SHELL_OWNED = {
    # `DesktopMemoryRead` flattens core's `MemoryRead` and adds the envelope.
    'MemoryRead',
    # `StartMemoryRequest` carries the model choice and maintenance revision.
    'StartMemory',
    # `discussion_recovery::WorkerIssue`, folded into the view the command
    # returns.
    'DiscussionView',
}


def run(args):
    p = subprocess.run(args, cwd=ROOT, env=ENV, capture_output=True, text=True)
    return p.stdout + p.stderr


def defining(name):
    """The file declaring `name` — as a struct, enum or alias."""
    for root, _, files in os.walk(os.path.join(ROOT, 'crates')):
        for f in files:
            if f.endswith('.rs'):
                p = os.path.join(root, f)
                if re.search(r'pub (struct|enum|type) %s\b' % re.escape(name),
                             io.open(p, encoding='utf-8').read()):
                    return p
    return None


def crate_of(path):
    return os.path.relpath(path, os.path.join(ROOT, 'crates')).replace(os.sep, '/').split('/')[0]


def rust_path(name):
    """A path to `name` that is reachable from outside the crate.

    The file a type is declared in is not always the path it is exported at:
    `project_chat/save_recap.rs` holds `ChatDocumentSave`, but `save_recap` is
    a private module and the type is reachable as
    `project_chat::ChatDocumentSave`. Walking the file path blindly produces
    `project_chat::save_recap::ChatDocumentSave`, which does not compile.

    So the module segments are kept only while each is declared `pub mod` by
    its parent.
    """
    path = defining(name)
    if not path:
        return None
    crate = crate_of(path)
    src = os.path.join(ROOT, 'crates', crate, 'src')
    rel = os.path.relpath(path, src).replace(os.sep, '/')[:-3]
    parts = [x for x in rel.split('/') if x and x not in ('lib', 'mod')]

    visible = []
    parent = os.path.join(src, 'lib.rs')
    for part in parts:
        text = io.open(parent, encoding='utf-8').read() if os.path.exists(parent) else ''
        if re.search(r'^\s*pub mod %s\b' % re.escape(part), text, re.M):
            visible.append(part)
            parent = os.path.join(os.path.dirname(parent), part + '.rs')
        else:
            break
    return 'wns_%s::%s' % (crate.replace('-', '_'), '::'.join(visible + [name]))


def ensure_specta(path):
    """Under `[dependencies]`, not appended.

    Appending lands past the `[dev-dependencies]` header, where the derive
    fails as an unresolved crate and the failure reads like a missing
    dependency rather than a misplaced one.
    """
    manifest = os.path.join(ROOT, 'crates', crate_of(path), 'Cargo.toml')
    s = io.open(manifest, encoding='utf-8').read()
    if 'specta' in s.split('[dev-dependencies]')[0]:
        return
    s = s.replace('[dependencies]', '[dependencies]\nspecta = "1.0.5"', 1)
    io.open(manifest, 'w', encoding='utf-8', newline='\n').write(s)


def ensure_dep(crate):
    """The bindings crate must depend on every crate it exports from."""
    manifest = os.path.join(ROOT, 'crates/bindings/Cargo.toml')
    t = io.open(manifest, encoding='utf-8').read()
    dep = 'wns-%s' % crate.replace('_', '-')
    if dep not in t.split('[dev-dependencies]')[0]:
        t = t.replace('wns-kernel = { path = "../kernel" }',
                      'wns-kernel = { path = "../kernel" }\n%s = { path = "../%s" }' % (dep, crate), 1)
        io.open(manifest, 'w', encoding='utf-8', newline='\n').write(t)


def add_derive(path):
    lines = io.open(path, encoding='utf-8').read().split('\n')
    out, added = [], 0
    for i, line in enumerate(lines):
        m = re.match(r'^#\[derive\(([^)]*)\)\]\s*$', line)
        if m and 'specta::Type' not in m.group(1):
            j, depth = i + 1, 0
            # Attributes may be blank-separated: `Lens` is written that way.
            while j < len(lines) and (depth > 0 or lines[j].lstrip().startswith('#[') or not lines[j].strip()):
                depth += lines[j].count('(') - lines[j].count(')')
                j += 1
            if j < len(lines) and re.match(r'pub (struct|enum) ', lines[j]):
                out.append('#[derive(%s, specta::Type)]' % m.group(1))
                added += 1
                continue
        out.append(line)
    if added:
        io.open(path, 'w', encoding='utf-8', newline='\n').write('\n'.join(out))
    return added


def fix_variants():
    """specta honours `rename_all` on a variant, not `rename_all_fields`."""
    total = 0
    for path in glob.glob(os.path.join(ROOT, 'crates/*/src/**/*.rs'), recursive=True):
        lines = io.open(path, encoding='utf-8').read().split('\n')
        out, added, last_has, in_enum = [], 0, False, False
        for i, line in enumerate(lines):
            m = re.match(r'^#\[derive\(([^)]*)\)\]\s*$', line)
            if m:
                last_has = 'specta::Type' in m.group(1)
            if re.match(r'^pub enum \w+', line):
                in_enum = last_has
            elif re.match(r'^pub (struct|union|fn|const|static|type|mod) ', line) or line == '}':
                in_enum = False
            if in_enum and re.match(r'^    \w+ \{', line) and not lines[i - 1].strip().startswith('#[specta('):
                out.append('    #[specta(rename_all = "camelCase")]')
                added += 1
            out.append(line)
        if added:
            io.open(path, 'w', encoding='utf-8', newline='\n').write('\n'.join(out))
            total += added
    return total


def bounds(s):
    start = s.index('    pub fn %s()' % GROUP)
    open_vec = s.index('            vec![\n', start) + len('            vec![\n')
    return open_vec, s.index('            ],\n', open_vec)


def set_entries(entries):
    s = io.open(LIB, encoding='utf-8').read()
    anchor = '    pub fn %s() -> Result<Group, specta::ts::TsExportError> {' % GROUP
    if anchor not in s:
        s = s.rstrip('\n')
        s = s[:-1].rstrip('\n') + '''

    /// `%s`'s IPC closure: every name its generated file mentions, closed
    /// against the frontend compiler rather than typed by hand.
    pub fn %s() -> Result<Group, specta::ts::TsExportError> {
        group(
            "%s",
            "%s",
            vec![
            ],
        )
    }
}
''' % (GROUP, GROUP, GROUP, GROUP)
        for old in ['Ok(vec![wns_groups::kernel()?, wns_groups::workshop()?, wns_groups::context()?])',
                    'Ok(vec![wns_groups::kernel()?, wns_groups::workshop()?])',
                    'Ok(vec![wns_groups::kernel()?])']:
            if old in s:
                s = s.replace(old, old[:-2] + ', wns_groups::%s()?])' % GROUP)
                break
    open_vec, close_vec = bounds(s)
    s = s[:open_vec] + ''.join('                ("%s", one::<%s>()),\n' % e for e in entries) + s[close_vec:]
    io.open(LIB, 'w', encoding='utf-8', newline='\n').write(s)


named = set()
for path in glob.glob(os.path.join(ROOT, 'apps/desktop/src/ipc/*.ts')):
    named |= set(re.findall(r'^export (?:type|interface|enum) (\w+)',
                            io.open(path, encoding='utf-8').read(), re.M))

seeded = []
for name in sorted(named):
    path = defining(name)
    if path and crate_of(path) == CRATE:
        ensure_specta(path)
        ensure_dep(CRATE)
        seeded.append(name)
print('seeded %d of the frontend\'s %d names from %s' % (len(seeded), len(named), CRATE))

set_entries([(n, p) for n, p in ((n, rust_path(n)) for n in sorted(seeded)) if p])

for round_number in range(1, 30):
    text = run([CARGO, 'check', '-p', 'wns-bindings'])
    names = {m.group(1).split('::')[-1] for m in re.finditer(
        r'trait bound `([^`]+): (?:specta::)?(?:r#type::Type|NamedType)` is not satisfied', text)}
    if not names:
        print('derives converged after %d rounds' % (round_number - 1))
        break
    progressed = False
    for name in sorted(names):
        path = defining(name)
        if path:
            ensure_specta(path)
            ensure_dep(crate_of(path))
            if add_derive(path):
                progressed = True
    if not progressed:
        print('stuck: %s' % ', '.join(sorted(names)))
        break

print('variant attributes:', fix_variants())
out = run([CARGO, 'run', '-p', 'wns-bindings'])
print('generated' if 'wrote' in out else out[:300])

for round_number in range(1, 25):
    p2 = subprocess.run('npx tsc --noEmit', cwd=os.path.join(ROOT, 'apps/desktop'),
                        capture_output=True, text=True, shell=True)
    names = sorted({m.group(1) for m in re.finditer(
        r"generated/%s\.ts.*Cannot find name '(\w+)'" % re.escape(GROUP), p2.stdout + p2.stderr)})
    if not names:
        print('name list converged after %d rounds' % (round_number - 1))
        break
    s = io.open(LIB, encoding='utf-8').read()
    declared = set(re.findall(r'\("(\w+)", one::<', s))
    todo = [(n, rust_path(n)) for n in names if n not in declared]
    todo = [(n, x) for n, x in todo if x]
    if not todo:
        print('nothing addable: %s' % ', '.join(names))
        break
    for name, _ in todo:
        f = defining(name)
        if f:
            ensure_specta(f)
            ensure_dep(crate_of(f))
    open_vec, close_vec = bounds(s)
    s = s[:close_vec] + ''.join('                ("%s", one::<%s>()),\n' % e for e in todo) + s[close_vec:]
    io.open(LIB, 'w', encoding='utf-8', newline='\n').write(s)
    print('  round %d: +%s' % (round_number, ', '.join(n for n, _ in todo)))
    run([CARGO, 'run', '-p', 'wns-bindings'])
