"""Seed a bindings group, converge its Rust derives, then generate.

The group's `one::<T>()` calls are what trigger the missing-derive errors, so
the group is seeded first and the derives are converged against *its* build.
"""
import io, os, re, shutil, subprocess, glob

ROOT = os.path.dirname(os.path.abspath(__file__))
CARGO = shutil.which('cargo') or os.path.expanduser('~/.cargo/bin/cargo.exe')
ENV = dict(os.environ, CARGO_TARGET_DIR='/d/WebnovelStudio_V3/target',
           PATH=os.path.expanduser('~/.cargo/bin') + ':' + os.environ.get('PATH', ''))
LIB = os.path.join(ROOT, 'crates/bindings/src/lib.rs')


def run(args):
    p = subprocess.run(args, cwd=ROOT, env=ENV, capture_output=True, text=True)
    return p.stdout + p.stderr


def missing_derives(text):
    return {m.group(1).split('::')[-1]
            for m in re.finditer(r'trait bound `([^`]+): specta::r#type::Type` is not satisfied', text)}


def defining(name):
    for root, _, files in os.walk(os.path.join(ROOT, 'crates')):
        for f in files:
            if f.endswith('.rs'):
                p = os.path.join(root, f)
                if re.search(r'pub (struct|enum) %s\b' % re.escape(name), io.open(p, encoding='utf-8').read()):
                    return p
    return None


def add_derive(path):
    lines = io.open(path, encoding='utf-8').read().split('\n')
    out, added = [], 0
    for i, line in enumerate(lines):
        m = re.match(r'^#\[derive\(([^)]*)\)\]\s*$', line)
        if m and 'specta::Type' not in m.group(1):
            j, depth = i + 1, 0
            # Attributes may be blank-separated: `#[derive(..)]`, a blank line,
            # then `#[serde(..)]` — which is how `Lens` is written.
            while j < len(lines) and (depth > 0 or lines[j].lstrip().startswith('#[')
                                      or not lines[j].strip()):
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


def ensure_specta(path):
    top = os.path.relpath(path, os.path.join(ROOT, 'crates')).replace(os.sep, '/').split('/')[0]
    manifest = os.path.join(ROOT, 'crates', top, 'Cargo.toml')
    s = io.open(manifest, encoding='utf-8').read()
    if 'specta' not in s:
        io.open(manifest, 'w', encoding='utf-8', newline='\n').write(s.rstrip('\n') + '\nspecta = "1.0.5"\n')


# --- seed the group from the workshop's derived types -------------------
def derived(path):
    s = io.open(os.path.join(ROOT, path), encoding='utf-8').read()
    return [m.group(2) for m in re.finditer(
        r'#\[derive\(([^)]*)\)\]\n(?:#\[[^\n]*\]\n)*\s*pub (?:struct|enum) (\w+)', s)
        if 'specta::Type' in m.group(1)]


entries = []
for n in derived('crates/workshop/src/workshop.rs'):
    entries.append((n, 'wns_workshop::workshop::%s' % n))
for n in derived('crates/story/src/workshop_vocabulary.rs'):
    entries.append((n, 'wns_story::workshop_vocabulary::%s' % n))
for f, mod in [('crates/story/src/run_vocabulary.rs', 'run_vocabulary'),
               ('crates/story/src/discussion_vocabulary.rs', 'discussion_vocabulary')]:
    for n in derived(f):
        entries.append((n, 'wns_story::%s::%s' % (mod, n)))
entries = list(dict.fromkeys(entries))

s = io.open(LIB, encoding='utf-8').read()
if 'pub fn workshop()' not in s:
    s = s.rstrip('\n')
    s = s[:-1].rstrip('\n') + '''

    /// `wns-workshop`'s IPC closure: the record vocabulary lives in
    /// `wns-story` since the two crates stopped being able to reach sideways,
    /// and the snapshot reaches `DiscussionRun` and through it the provider and
    /// context vocabularies.
    pub fn workshop() -> Result<Group, specta::ts::TsExportError> {
        group(
            "workshop",
            "wns-workshop",
            vec![
            ],
        )
    }
}
'''
    s = s.replace('    Ok(vec![wns_groups::kernel()?])',
                  '    Ok(vec![wns_groups::kernel()?, wns_groups::workshop()?])')
start = s.index('pub fn workshop()')
open_vec = s.index('            vec![\n', start) + len('            vec![\n')
close_vec = s.index('            ],\n', open_vec)
s = s[:open_vec] + ''.join('                ("%s", one::<%s>()),\n' % e for e in entries) + s[close_vec:]
io.open(LIB, 'w', encoding='utf-8', newline='\n').write(s)

p = os.path.join(ROOT, 'crates/bindings/Cargo.toml')
t = io.open(p, encoding='utf-8').read()
for crate, path in [('wns-story', '../story'), ('wns-workshop', '../workshop'),
                    ('wns-context', '../context'), ('wns-providers', '../providers'),
                    ('wns-documents', '../documents')]:
    if crate not in t:
        t = t.rstrip('\n') + '\n%s = { path = "%s" }\n' % (crate, path)
io.open(p, 'w', encoding='utf-8', newline='\n').write(t)
print('seeded %d entries' % len(entries))

# --- converge the derives against the bindings crate's own build --------
for round_number in range(1, 25):
    names = missing_derives(run([CARGO, 'check', '-p', 'wns-bindings']))
    if not names:
        print('derives converged after %d rounds' % (round_number - 1))
        break
    progressed = False
    for name in sorted(names):
        path = defining(name)
        if path:
            ensure_specta(path)
            if add_derive(path):
                progressed = True
    print('  round %d: %d missing' % (round_number, len(names)))
    if not progressed:
        print('  stuck: %s' % ', '.join(sorted(names)))
        break

print('variant attributes:', fix_variants())
out = run([CARGO, 'run', '-p', 'wns-bindings'])
print('generated' if 'wrote' in out else out[:400])
