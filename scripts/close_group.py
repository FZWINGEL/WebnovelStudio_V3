"""Close a bindings group's name list against the frontend compiler."""
import io, os, re, shutil, subprocess

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CARGO = shutil.which('cargo') or os.path.expanduser('~/.cargo/bin/cargo.exe')
ENV = dict(os.environ, CARGO_TARGET_DIR='/d/WebnovelStudio_V3/target',
           PATH=os.path.expanduser('~/.cargo/bin') + ':' + os.environ.get('PATH', ''))
LIB = os.path.join(ROOT, 'crates/bindings/src/lib.rs')


def rust_path(name):
    for root, _, files in os.walk(os.path.join(ROOT, 'crates')):
        for f in files:
            if not f.endswith('.rs'):
                continue
            path = os.path.join(root, f)
            if re.search(r'pub (struct|enum) %s\b' % re.escape(name), io.open(path, encoding='utf-8').read()):
                crate = os.path.relpath(root, os.path.join(ROOT, 'crates')).replace(os.sep, '/').split('/')[0]
                rel = os.path.relpath(path, os.path.join(ROOT, 'crates', crate, 'src')).replace(os.sep, '/')[:-3]
                parts = [x for x in rel.split('/') if x and x not in ('lib', 'mod')]
                return 'wns_%s::%s' % (crate.replace('-', '_'), '::'.join(parts + [name]))
    return None


def tsc_missing():
    p = subprocess.run('npx tsc --noEmit', cwd=os.path.join(ROOT, 'apps/desktop'),
                       capture_output=True, text=True, shell=True)
    return sorted({m.group(1) for m in re.finditer(
        r"generated/workshop\.ts.*Cannot find name '(\w+)'", p.stdout + p.stderr)})


for round_number in range(1, 20):
    names = tsc_missing()
    if not names:
        print('converged after %d rounds' % (round_number - 1))
        break
    s = io.open(LIB, encoding='utf-8').read()
    declared = set(re.findall(r'\("(\w+)", one::<', s))
    todo = [(n, rust_path(n)) for n in names if n not in declared]
    todo = [(n, p) for n, p in todo if p]
    if not todo:
        print('nothing addable: %s' % ', '.join(names))
        break
    start = s.index('pub fn workshop()')
    open_vec = s.index('            vec![\n', start) + len('            vec![\n')
    close_vec = s.index('            ],\n', open_vec)
    s = s[:close_vec] + ''.join('                ("%s", one::<%s>()),\n' % e for e in todo) + s[close_vec:]
    io.open(LIB, 'w', encoding='utf-8', newline='\n').write(s)
    print('round %d: +%s' % (round_number, ', '.join(n for n, _ in todo)))
    subprocess.run([CARGO, 'run', '-p', 'wns-bindings'], cwd=ROOT, env=ENV,
                   capture_output=True, text=True)
