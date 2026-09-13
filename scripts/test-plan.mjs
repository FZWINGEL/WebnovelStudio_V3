import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const root = fileURLToPath(new URL('../', import.meta.url));

export const CRATE_MAPPINGS = {
  'crates/documents': {
    crate: 'wns-documents',
    integrationPrefixes: ['document_roles', 'scope', 'append_scope', 'text_replacement'],
  },
  'crates/conversation': {
    crate: 'wns-conversation',
    integrationPrefixes: ['project_chat', 'discussions', 'discussion_lookup'],
  },
  'crates/workshop': {
    crate: 'wns-workshop',
    integrationPrefixes: ['workshop'],
  },
  'crates/context': {
    crate: 'wns-context',
    integrationPrefixes: ['context_', 'navigation_'],
  },
  'crates/story': {
    crate: 'wns-story',
    integrationPrefixes: ['story_', 'reviewed_story', 'memory_', 'lookup_'],
  },
  'crates/providers': {
    crate: 'wns-providers',
    integrationPrefixes: ['codex_', 'claude_', 'openai_compatible', 'provider_endpoints', 'windows_process', 'model_settings'],
  },
  'crates/storage': {
    crate: 'wns-storage',
    integrationPrefixes: ['persistence', 'metadata', 'history', 'recovery_copy'],
  },
  'crates/kernel': {
    crate: 'wns-kernel',
    integrationPrefixes: ['persistence', 'metadata', 'history', 'source_pins'],
  },
  'crates/transfer': {
    crate: 'wns-transfer',
    integrationPrefixes: ['transfer', 'v2_import'],
  },
  'crates/library': {
    crate: 'wns-library',
    integrationPrefixes: ['library'],
  },
  'crates/bindings': {
    crate: 'wns-bindings',
    integrationPrefixes: ['structured_contract_golden'],
  },
  'contracts': {
    crate: 'contracts',
    integrationPrefixes: ['structured_contract_golden'],
  },
};

export const KNOWN_WORKSPACE_PACKAGES = Object.freeze(new Set([
  'wns-kernel', 'wns-storage', 'wns-providers', 'wns-documents',
  'wns-context', 'wns-story', 'wns-conversation', 'wns-workshop',
  'wns-transfer', 'wns-library', 'wns-architecture', 'contracts',
  'wns-bindings', 'webnovel-core', 'webnovel-desktop',
]));

const workspacePackagesCache = new Map();
let defaultWorkspacePackages = null;

export function setDefaultWorkspacePackages(pkgs) {
  defaultWorkspacePackages = pkgs;
}

export function getWorkspacePackages(baseDir = root) {
  if (defaultWorkspacePackages) return defaultWorkspacePackages;
  if (workspacePackagesCache.has(baseDir)) {
    const cached = workspacePackagesCache.get(baseDir);
    if (cached instanceof Error) throw cached;
    return cached;
  }
  try {
    const stdout = execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], { cwd: baseDir, encoding: 'utf8', windowsHide: true });
    const meta = JSON.parse(stdout);
    const pkgs = new Set(meta.packages.map(p => p.name));
    workspacePackagesCache.set(baseDir, pkgs);
    return pkgs;
  } catch (err) {
    const error = new Error(`Failed to acquire Cargo workspace packages from ${baseDir}: ${err.message}`);
    workspacePackagesCache.set(baseDir, error);
    throw error;
  }
}

export function deduplicateFilters(filters) {
  const unique = Array.from(new Set(filters)).filter(Boolean);
  unique.sort((a, b) => a.length - b.length);
  const result = [];
  for (const filter of unique) {
    if (!result.some(shorter => filter.includes(shorter))) {
      result.push(filter);
    }
  }
  return result;
}

export function parseGitStatusPorcelainZ(output) {
  if (!output) return [];
  const parts = output.split('\0');
  const files = [];
  for (let i = 0; i < parts.length; i++) {
    const entry = parts[i];
    if (!entry) continue;
    const status = entry.slice(0, 2);
    const filePath = entry.slice(3).replaceAll('\\', '/');
    if (filePath) files.push(filePath);
    if (status.includes('R') || status.includes('C')) {
      i++;
      if (i < parts.length && parts[i]) {
        files.push(parts[i].replaceAll('\\', '/'));
      }
    }
  }
  return files;
}

export function getChangedFiles(options = {}) {
  const baseDir = options.baseDir || root;
  const base = options.base;
  const head = options.head;
  if (head && !base) {
    throw new Error('Cannot specify head commit without base commit.');
  }
  try {
    if (base) {
      const targetHead = head || 'HEAD';
      const stdout = execFileSync('git', ['diff', '--no-renames', '--name-only', '-z', `${base}..${targetHead}`], { cwd: baseDir, encoding: 'utf8', windowsHide: true });
      return stdout.split('\0').filter(Boolean).map(s => s.replaceAll('\\', '/'));
    }
    const stdout = execFileSync('git', ['status', '--porcelain', '-z', '-uall'], { cwd: baseDir, encoding: 'utf8', windowsHide: true });
    return parseGitStatusPorcelainZ(stdout);
  } catch (err) {
    return { error: err?.message || String(err) };
  }
}



export function isBroadPath(f) {
  return (
    f.startsWith('.github/') ||
    f.startsWith('.cargo/') ||
    f === 'Cargo.toml' ||
    f === 'Cargo.lock' ||
    f === 'package.json' ||
    f === 'package-lock.json' ||
    f === '.node-version' ||
    f === 'apps/desktop/package.json' ||
    f === 'apps/desktop/package-lock.json' ||
    f === 'apps/desktop/src-tauri/tauri.conf.json' ||
    f === 'apps/desktop/src-tauri/Cargo.toml' ||
    f.startsWith('crates/core/') ||
    f.startsWith('crates/architecture/') ||
    f.endsWith('/Cargo.toml') ||
    f.endsWith('.sql') ||
    f.includes('/migrations/')
  );
}

export function isDocPath(f) {
  return (
    f.startsWith('docs/') ||
    f.endsWith('.md') ||
    f.startsWith('LICENSE') ||
    ['.gitignore'].includes(f)
  );
}

export class PlanRequirements {
  constructor() {
    this.isClean = false;
    this.broadFallback = null;
    this.rustFmt = false;
    this.rustPackages = new Set();
    this.rustIntegrationFilters = new Set();
    this.frontendTypecheck = false;
    this.frontendFullVitest = false;
    this.frontendDirs = new Set();
    this.frontendFiles = new Set();
    this.toolingProfiles = new Set();
    this.nativeSuites = new Set();
    this.nativeOutstanding = false;
    this.files = [];
  }

  merge(other) {
    if (other.broadFallback) {
      this.broadFallback = this.broadFallback ? `${this.broadFallback}; ${other.broadFallback}` : other.broadFallback;
    }
    if (other.rustFmt) this.rustFmt = true;
    for (const p of other.rustPackages) this.rustPackages.add(p);
    for (const f of other.rustIntegrationFilters) this.rustIntegrationFilters.add(f);
    if (other.frontendTypecheck) this.frontendTypecheck = true;
    if (other.frontendFullVitest) this.frontendFullVitest = true;
    for (const d of other.frontendDirs) this.frontendDirs.add(d);
    for (const f of other.frontendFiles) this.frontendFiles.add(f);
    for (const tp of other.toolingProfiles) this.toolingProfiles.add(tp);
    for (const s of other.nativeSuites) this.nativeSuites.add(s);
    if (other.nativeOutstanding) this.nativeOutstanding = true;
    if (other.files?.length) this.files.push(...other.files);
  }
}

export function classifyPath(f) {
  const req = new PlanRequirements();

  if (isBroadPath(f)) {
    req.broadFallback = `Manifest, config, migration, or core changed: ${f}`;
    return req;
  }

  if (isDocPath(f)) {
    req.toolingProfiles.add('core');
    return req;
  }

  // Native runner & harness paths check BEFORE generic scripts/
  if (
    f.startsWith('apps/desktop/scripts/native-') ||
    f.startsWith('apps/desktop/scripts/owned-process') ||
    f.startsWith('scripts/native-') ||
    f.startsWith('scripts/owned-process') ||
    f.startsWith('tests/native/')
  ) {
    if (f.endsWith('.test.mjs')) {
      if (f.startsWith('tests/native/')) {
        req.toolingProfiles.add('native-preflight');
      } else {
        req.toolingProfiles.add('core');
      }
      return req;
    }

    req.toolingProfiles.add('core');
    req.toolingProfiles.add('native-preflight');
    req.nativeOutstanding = true;

    if (f === 'scripts/native-consumer.mjs' || f === 'scripts/native-artifact.mjs') {
      req.nativeSuites.add('all');
    } else if (f.includes('native-smoke.mjs') || f.includes('native-save-dialog.ps1')) {
      req.nativeSuites.add('main');
    } else if (f.includes('native-chat-smoke.mjs') || f.includes('native-chat-failures.mjs') || f.includes('native-chat-live.mjs') || f.includes('native-chat-window.ps1')) {
      req.nativeSuites.add('chat');
    } else if (f.includes('native-workshop-smoke.mjs')) {
      req.nativeSuites.add('workshop');
    } else if (f.includes('native-http-smoke.mjs')) {
      req.nativeSuites.add('http');
    } else if (f.includes('native-app-close.mjs') || f.includes('native-app-close.ps1')) {
      req.nativeSuites.add('close');
    } else if (f.includes('native-interruption.mjs') || f.includes('native-context-menu') || f.includes('native-refresh-shortcut')) {
      req.nativeSuites.add('interruption');
    } else if (f.includes('native-project-recovery.mjs') || f.includes('native-backup-dialog.ps1')) {
      req.nativeSuites.add('recovery');
    } else if (f.includes('native-memory-lookup')) {
      req.nativeSuites.add('memory');
    } else if (f.includes('native-app-server-smoke.mjs')) {
      req.nativeSuites.add('app-server-transport');
    } else {
      req.nativeSuites.add('all');
    }
    return req;
  }

  if (f.startsWith('scripts/')) {
    req.toolingProfiles.add('core');
    return req;
  }

  if (f.startsWith('contracts/') || f.startsWith('crates/bindings/')) {
    req.rustFmt = true;
    req.rustPackages.add('wns-bindings');
    req.rustPackages.add('contracts');
    req.rustIntegrationFilters.add('structured_contract_golden');
    req.frontendTypecheck = true;
    req.frontendDirs.add('kernel');
    req.frontendFiles.add('src/featureBoundary.test.ts');
    req.nativeOutstanding = true;
    req.nativeSuites.add('main');
    req.nativeSuites.add('app-server-transport');
    return req;
  }

  for (const [prefix, config] of Object.entries(CRATE_MAPPINGS)) {
    if (f.startsWith(prefix + '/')) {
      req.rustFmt = true;
      req.rustPackages.add(config.crate);
      config.integrationPrefixes.forEach(p => req.rustIntegrationFilters.add(p));
      req.nativeOutstanding = true;
      if (config.crate === 'wns-workshop') {
        req.nativeSuites.add('workshop');
      } else if (config.crate === 'wns-conversation' || config.crate === 'wns-providers') {
        req.nativeSuites.add('chat');
        req.nativeSuites.add('app-server-transport');
      } else if (config.crate === 'wns-context') {
        req.nativeSuites.add('memory');
        req.nativeSuites.add('chat');
      } else {
        req.nativeSuites.add('main');
        req.nativeSuites.add('recovery');
      }
      return req;
    }
  }

  if (f.startsWith('apps/desktop/src/') || f.startsWith('apps/desktop/public/')) {
    req.frontendTypecheck = true;

    const isTestFile = f.startsWith('apps/desktop/src/') && (
      f.endsWith('.test.ts') || f.endsWith('.test.tsx') || f.endsWith('.test.js') || f.endsWith('.test.jsx')
    );

    if (isTestFile) {
      const rel = f.replace('apps/desktop/', '');
      req.frontendFiles.add(rel);
      req.frontendFiles.add('src/featureBoundary.test.ts');
      return req;
    }

    req.nativeOutstanding = true;
    if (f.startsWith('apps/desktop/public/') || f === 'apps/desktop/src/index.html' || f.match(/^apps\/desktop\/src\/[^/]+$/)) {
      req.frontendFullVitest = true;
      req.nativeSuites.add('main');
    } else {
      const rel = f.replace('apps/desktop/src/', '');
      const dir = rel.split('/')[0];
      if (dir && !dir.includes('.')) {
        req.frontendDirs.add(dir);
        if (dir === 'chat' || dir === 'assistant') {
          req.nativeSuites.add('chat');
          req.nativeSuites.add('memory');
        } else if (dir === 'workshop') {
          req.nativeSuites.add('workshop');
        } else if (dir === 'editor' || dir === 'kernel') {
          req.nativeSuites.add('main');
          req.nativeSuites.add('recovery');
          req.nativeSuites.add('close');
        } else if (dir === 'shell') {
          req.nativeSuites.add('main');
          req.nativeSuites.add('workshop');
          req.nativeSuites.add('chat');
        } else if (dir === 'ipc') {
          req.nativeSuites.add('main');
          req.nativeSuites.add('app-server-transport');
          req.nativeSuites.add('http');
        } else {
          req.nativeSuites.add('main');
        }
      } else {
        req.frontendFullVitest = true;
        req.nativeSuites.add('main');
      }
      req.frontendFiles.add('src/featureBoundary.test.ts');
    }
    return req;
  }

  req.broadFallback = `Unrecognized file path: ${f}`;
  return req;
}

export function classifyChanges(files) {
  if (files && typeof files === 'object' && !Array.isArray(files) && files.error) {
    const req = new PlanRequirements();
    req.broadFallback = `git status error: ${files.error}`;
    return req;
  }

  const req = new PlanRequirements();
  if (!files || files.length === 0) {
    req.isClean = true;
    return req;
  }

  const normalized = files.map(f => String(f).replaceAll('\\', '/').replace(/^\.\//, ''));
  req.files = normalized;

  for (const f of normalized) {
    req.merge(classifyPath(f));
  }

  return req;
}

export function validateCommand(cmd, baseDir = root, options = {}) {
  if (cmd.executable === 'npm' && cmd.cwd === 'apps/desktop') {
    if (cmd.args[0] === 'run') {
      const scriptName = cmd.args[1];
      const pkgPath = path.resolve(baseDir, 'apps/desktop/package.json');
      const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
      if (!pkg.scripts?.[scriptName]) {
        throw new Error(`Emitted invalid npm script: 'npm run ${scriptName}'. Script does not exist in apps/desktop/package.json.`);
      }
    }
  } else if (cmd.executable === 'cargo') {
    const known = options.workspacePackages || (defaultWorkspacePackages || getWorkspacePackages(baseDir));
    for (let i = 0; i < cmd.args.length; i++) {
      if (cmd.args[i] === '-p' && i + 1 < cmd.args.length) {
        const pkg = cmd.args[i + 1];
        if (!known.has(pkg)) {
          throw new Error(`Emitted invalid Cargo package: '${pkg}'. Package not in workspace Cargo.toml.`);
        }
      }
    }
  }
}

export function planFromClassification(classification, baseDir = root, options = {}) {
  // Support either PlanRequirements instance or object
  const req = classification instanceof PlanRequirements
    ? classification
    : (() => {
        const r = new PlanRequirements();
        if (classification.category === 'none' || classification.isClean) r.isClean = true;
        if (classification.category === 'broad' || classification.broadFallback) {
          r.broadFallback = classification.reason || classification.broadFallback || 'Broad qualification required';
        }
        if (classification.category === 'docs') r.toolingProfiles.add('core');
        if (classification.crates) classification.crates.forEach(c => r.rustPackages.add(typeof c === 'string' ? c : c.crate));
        if (classification.integrationPrefixes) classification.integrationPrefixes.forEach(p => r.rustIntegrationFilters.add(p));
        if (classification.needsFrontend) r.frontendTypecheck = true;
        if (classification.fullFrontend) r.frontendFullVitest = true;
        if (classification.frontendDirs) classification.frontendDirs.forEach(d => r.frontendDirs.add(d));
        if (classification.needsTooling) r.toolingProfiles.add('core');
        if (classification.files) r.files = classification.files;
        return r;
      })();

  let nativeObligation;
  if (req.isClean) {
    nativeObligation = {
      required: false,
      status: 'none',
      suites: [],
      reason: 'Working tree is clean.',
    };
  } else if (req.broadFallback) {
    nativeObligation = {
      required: true,
      status: 'all',
      suites: ['all'],
      reason: req.broadFallback,
    };
  } else if (req.nativeOutstanding || req.nativeSuites.size > 0) {
    const suites = Array.from(req.nativeSuites).sort();
    nativeObligation = {
      required: true,
      status: suites.includes('all') ? 'all' : 'scoped',
      suites: suites.includes('all') ? ['all'] : suites,
      reason: 'Application source or native orchestration changed: native qualification required before acceptance.',
    };
  } else {
    nativeObligation = {
      required: false,
      status: 'none',
      suites: [],
      reason: 'Documentation, tooling, or test-only change: native qualification not required.',
    };
  }

  if (req.isClean) {
    return {
      category: 'clean',
      description: 'Working tree is clean.',
      commands: [
        { executable: 'cargo', args: ['fmt', '--all', '--check'] },
        { executable: 'node', args: ['scripts/run-tooling-tests.mjs', '--profile=core'] },
      ],
      exclusions: ['Skipped full workspace compilation and frontend tests'],
      nativeObligation,
      outstandingNative: false,
      requiredNativeSuites: [],
    };
  }

  if (req.broadFallback) {
    return {
      category: 'broad',
      description: req.broadFallback,
      commands: [
        { executable: 'desktop', args: ['check'] },
      ],
      exclusions: ['No exclusions (fails closed to full check)'],
      nativeObligation,
      outstandingNative: true,
      requiredNativeSuites: ['all'],
    };
  }

  const commands = [];
  const exclusions = [];

  if (req.rustFmt) {
    commands.push({ executable: 'cargo', args: ['fmt', '--all', '--check'] });
  }

  if (req.rustPackages.size > 0) {
    const pkgs = Array.from(req.rustPackages).sort();
    const pkgArgs = pkgs.flatMap(p => ['-p', p]);
    commands.push({ executable: 'cargo', args: ['clippy', ...pkgArgs, '--all-targets', '--locked', '--', '-D', 'warnings'] });
    commands.push({ executable: 'cargo', args: ['test', ...pkgArgs, '--lib', '--bins', '--locked'] });
  } else {
    exclusions.push('Skipped Rust workspace test and compilation');
  }

  if (req.rustIntegrationFilters.size > 0) {
    const filters = deduplicateFilters(Array.from(req.rustIntegrationFilters));
    commands.push({ executable: 'cargo', args: ['test', '-p', 'webnovel-core', '--test', 'integration', '--locked', '--', ...filters] });
  }

  if (req.frontendTypecheck) {
    commands.push({ executable: 'npm', args: ['run', 'typecheck'], cwd: 'apps/desktop' });
    if (req.frontendFullVitest || (!req.frontendDirs.size && !req.frontendFiles.size) || req.frontendDirs.size > 2) {
      commands.push({ executable: 'npm', args: ['test'], cwd: 'apps/desktop' });
    } else {
      const targets = Array.from(req.frontendDirs).sort().map(d => `src/${d}/`);
      targets.push(...Array.from(req.frontendFiles).sort());
      commands.push({ executable: 'npm', args: ['test', '--', ...targets], cwd: 'apps/desktop' });
    }
  } else {
    exclusions.push('Skipped frontend Vitest suite');
  }

  if (req.toolingProfiles.size > 0) {
    const profiles = Array.from(req.toolingProfiles);
    if (profiles.includes('native-preflight') && profiles.includes('core')) {
      commands.push({ executable: 'node', args: ['scripts/run-tooling-tests.mjs', '--profile=all'] });
    } else if (profiles.includes('native-preflight')) {
      commands.push({ executable: 'node', args: ['scripts/run-tooling-tests.mjs', '--profile=native-preflight'] });
    } else {
      commands.push({ executable: 'node', args: ['scripts/run-tooling-tests.mjs', '--profile=core'] });
    }
  }

  let category = 'scoped';
  if (req.rustPackages.size > 0 && req.frontendTypecheck) category = 'cross-cutting';
  else if (req.rustPackages.size > 0) category = 'isolated-rust';
  else if (req.frontendTypecheck) category = 'frontend';
  else if (req.nativeSuites.size > 0) category = 'native-harness';
  else if (req.toolingProfiles.size > 0) category = req.files.every(isDocPath) ? 'docs' : 'tooling';

  const outstandingNative = nativeObligation.required;
  const requiredNativeSuites = nativeObligation.suites;
  if (!outstandingNative) {
    exclusions.push('Skipped native WebView2 suite');
  } else {
    exclusions.push('Skipped native WebView2 suite (omitted from fast local iteration; required before merge)');
  }

  const workspacePackages = options.workspacePackages || defaultWorkspacePackages;
  for (const cmd of commands) {
    validateCommand(cmd, baseDir, { workspacePackages });
  }

  return {
    category,
    description: `Targeted plan based on accumulated requirements for ${req.files.length} changed file(s).`,
    commands,
    exclusions,
    nativeObligation,
    outstandingNative,
    requiredNativeSuites,
  };
}

export function formatPlan(plan) {
  let output = `\n=== WebnovelStudio Change-Aware Test Plan ===\n`;
  output += `Category:    ${plan.category}\n`;
  output += `Description: ${plan.description}\n\n`;
  output += `Selected checks (${plan.commands.length}):\n`;
  plan.commands.forEach((cmd, idx) => {
    output += `  ${idx + 1}. ${cmd.executable} ${cmd.args.join(' ')}${cmd.cwd ? ` (cwd: ${cmd.cwd})` : ''}\n`;
  });
  output += `\nExclusions:\n`;
  plan.exclusions.forEach(exc => {
    output += `  - ${exc}\n`;
  });
  output += `\nNative Qualification:\n`;
  if (!plan.nativeObligation?.required) {
    output += `  - Required:  No\n`;
    output += `  - Status:    NONE\n`;
    output += `  - Reason:    ${plan.nativeObligation?.reason || 'Not required for scoped changes.'}\n`;
  } else {
    output += `  - Required:  Yes\n`;
    output += `  - Status:    ${plan.nativeObligation.status.toUpperCase()} (${plan.nativeObligation.suites.join(', ')})\n`;
    output += `  - Reason:    ${plan.nativeObligation.reason}\n`;
  }
  return output;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  let base = null;
  let head = null;
  let jsonMode = false;
  let porcelainMode = false;

  for (let i = 2; i < process.argv.length; i++) {
    const arg = process.argv[i];
    if (arg === '--json') {
      jsonMode = true;
    } else if (arg === '--porcelain') {
      porcelainMode = true;
    } else if (arg === '--base') {
      if (i + 1 >= process.argv.length || process.argv[i + 1].startsWith('--')) {
        throw new Error('Missing argument value for --base.');
      }
      base = process.argv[++i];
    } else if (arg === '--head') {
      if (i + 1 >= process.argv.length || process.argv[i + 1].startsWith('--')) {
        throw new Error('Missing argument value for --head.');
      }
      head = process.argv[++i];
    } else {
      throw new Error(`Unsupported option: ${arg}`);
    }
  }

  if (head && !base) {
    throw new Error('Cannot specify --head without --base.');
  }

  let scope;
  if (base) {
    const targetHead = head || 'HEAD';
    const resolvedBase = execFileSync('git', ['rev-parse', `${base}^{commit}`], { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
    const resolvedHead = execFileSync('git', ['rev-parse', `${targetHead}^{commit}`], { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
    scope = { type: 'revision-range', base: resolvedBase, head: resolvedHead };
  } else {
    const headCommit = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8', windowsHide: true }).trim();
    scope = { type: 'worktree', head: headCommit };
  }

  const files = getChangedFiles({ base, head });
  const classification = classifyChanges(files);
  const plan = planFromClassification(classification);
  if (jsonMode) {
    console.log(JSON.stringify({
      scope,
      snapshot: {
        timestamp: new Date().toISOString(),
        changedFiles: Array.isArray(files) ? files : [],
      },
      ...plan,
    }, null, 2));
  } else {
    console.log(formatPlan(plan));
  }
}

