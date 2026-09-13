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

export const KNOWN_WORKSPACE_PACKAGES = new Set([
  'wns-kernel',
  'wns-storage',
  'wns-providers',
  'wns-documents',
  'wns-context',
  'wns-story',
  'wns-conversation',
  'wns-workshop',
  'wns-transfer',
  'wns-library',
  'wns-architecture',
  'contracts',
  'wns-bindings',
  'webnovel-core',
  'webnovel-desktop',
]);

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

export function getChangedFiles(baseDir = root) {
  try {
    const stdout = execFileSync('git', ['status', '--porcelain', '-z', '-uall'], { cwd: baseDir, encoding: 'utf8' });
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
    f.endsWith('/Cargo.toml')
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

export function classifyChanges(files) {
  if (files && typeof files === 'object' && !Array.isArray(files) && files.error) {
    return { category: 'broad', reason: `git status error: ${files.error}`, files: [] };
  }

  if (!files || files.length === 0) {
    return { category: 'none', files: [] };
  }

  const normalized = files.map(f => String(f).replaceAll('\\', '/').replace(/^\.\//, ''));

  // If any changed path is a broad/manifest path -> broad
  const broadFile = normalized.find(isBroadPath);
  if (broadFile) {
    return { category: 'broad', reason: `Manifest, config, or core changed: ${broadFile}`, files: normalized };
  }

  // Union model: check every file against known areas
  const crates = new Set();
  const integrationPrefixes = new Set();
  let needsFrontend = false;
  let fullFrontend = false;
  const frontendDirs = new Set();
  let needsTooling = false;
  let allDocs = true;

  for (const f of normalized) {
    let matched = false;

    if (isDocPath(f)) {
      matched = true;
      continue;
    }

    allDocs = false;

    if (f.startsWith('scripts/') || f.startsWith('tests/native/')) {
      needsTooling = true;
      matched = true;
      continue;
    }

    if (f.startsWith('contracts/') || f.startsWith('crates/bindings/')) {
      crates.add('wns-bindings');
      crates.add('contracts');
      integrationPrefixes.add('structured_contract_golden');
      needsFrontend = true;
      frontendDirs.add('kernel');
      matched = true;
      continue;
    }

    for (const [prefix, config] of Object.entries(CRATE_MAPPINGS)) {
      if (f.startsWith(prefix + '/')) {
        crates.add(config.crate);
        config.integrationPrefixes.forEach(p => integrationPrefixes.add(p));
        matched = true;
        break;
      }
    }
    if (matched) continue;

    if (f.startsWith('apps/desktop/src/') || f.startsWith('apps/desktop/public/')) {
      needsFrontend = true;
      if (f.startsWith('apps/desktop/public/') || f === 'apps/desktop/src/index.html' || f.match(/^apps\/desktop\/src\/[^/]+$/)) {
        fullFrontend = true;
      } else {
        const rel = f.replace('apps/desktop/src/', '');
        const dir = rel.split('/')[0];
        if (dir && !dir.includes('.')) {
          frontendDirs.add(dir);
        } else {
          fullFrontend = true;
        }
      }
      matched = true;
      continue;
    }

    // Any unmatched file fails closed to broad
    return { category: 'broad', reason: `Unrecognized file path: ${f}`, files: normalized };
  }

  if (allDocs) {
    return { category: 'docs', files: normalized };
  }

  const isContractOnly = crates.size === 2 && crates.has('wns-bindings') && crates.has('contracts') && !fullFrontend && frontendDirs.size === 1 && frontendDirs.has('kernel');
  if (isContractOnly) {
    return { category: 'contracts', files: normalized, crates: Array.from(crates), integrationPrefixes: Array.from(integrationPrefixes) };
  }

  if (crates.size > 0 && !needsFrontend) {
    return { category: 'isolated-rust', files: normalized, crates: Array.from(crates), integrationPrefixes: Array.from(integrationPrefixes), needsTooling };
  }

  if (needsFrontend && crates.size === 0) {
    return { category: 'frontend', files: normalized, fullFrontend, frontendDirs: Array.from(frontendDirs), needsTooling };
  }

  return {
    category: 'cross-cutting',
    files: normalized,
    crates: Array.from(crates),
    integrationPrefixes: Array.from(integrationPrefixes),
    needsFrontend,
    fullFrontend,
    frontendDirs: Array.from(frontendDirs),
    needsTooling,
  };
}

export function validateCommand(cmd, baseDir = root) {
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
    for (let i = 0; i < cmd.args.length; i++) {
      if (cmd.args[i] === '-p' && i + 1 < cmd.args.length) {
        const pkg = cmd.args[i + 1];
        if (!KNOWN_WORKSPACE_PACKAGES.has(pkg)) {
          throw new Error(`Emitted invalid Cargo package: '${pkg}'. Package not in workspace Cargo.toml.`);
        }
      }
    }
  }
}

export function planFromClassification(classification, baseDir = root) {
  let plan;
  switch (classification.category) {
    case 'none':
      plan = {
        category: 'clean',
        description: 'Working tree is clean.',
        commands: [
          { executable: 'cargo', args: ['fmt', '--all', '--check'] },
          { executable: 'node', args: ['scripts/run-tooling-tests.mjs'] },
        ],
        exclusions: ['Skipped full workspace compilation and frontend tests'],
        outstandingNative: false,
      };
      break;

    case 'docs':
      plan = {
        category: 'docs',
        description: 'Documentation changes only.',
        commands: [
          { executable: 'node', args: ['scripts/run-tooling-tests.mjs'] },
        ],
        exclusions: ['Skipped Rust compilation and frontend Vitest suite'],
        outstandingNative: false,
      };
      break;

    case 'isolated-rust': {
      const commands = [{ executable: 'cargo', args: ['fmt', '--all', '--check'] }];
      for (const crate of classification.crates) {
        commands.push({ executable: 'cargo', args: ['clippy', '-p', crate, '--all-targets', '--locked', '--', '-D', 'warnings'] });
        commands.push({ executable: 'cargo', args: ['test', '-p', crate, '--lib', '--bins', '--locked'] });
      }
      if (classification.integrationPrefixes?.length > 0) {
        const filters = deduplicateFilters(classification.integrationPrefixes);
        commands.push({ executable: 'cargo', args: ['test', '-p', 'webnovel-core', '--test', 'integration', '--locked', '--', ...filters] });
      }
      if (classification.needsTooling) {
        commands.push({ executable: 'node', args: ['scripts/run-tooling-tests.mjs'] });
      }
      plan = {
        category: 'isolated-rust',
        description: `Scoped Rust changes in: ${classification.crates.join(', ')}`,
        commands,
        exclusions: ['Skipped unrelated Rust crates', 'Skipped frontend Vitest suite', 'Skipped native WebView2 suite'],
        outstandingNative: false,
      };
      break;
    }

    case 'contracts': {
      const commands = [
        { executable: 'cargo', args: ['fmt', '--all', '--check'] },
        { executable: 'cargo', args: ['clippy', '-p', 'wns-bindings', '-p', 'contracts', '--all-targets', '--locked', '--', '-D', 'warnings'] },
        { executable: 'cargo', args: ['test', '-p', 'wns-bindings', '-p', 'contracts', '--locked'] },
        { executable: 'cargo', args: ['test', '-p', 'webnovel-core', '--test', 'integration', '--locked', '--', 'structured_contract_golden'] },
        { executable: 'npm', args: ['run', 'typecheck'], cwd: 'apps/desktop' },
        { executable: 'npm', args: ['test', '--', 'src/kernel/', 'src/featureBoundary.test.ts'], cwd: 'apps/desktop' },
      ];
      plan = {
        category: 'contracts',
        description: 'Shared contract and bindings changes.',
        commands,
        exclusions: ['Skipped unrelated concern crates', 'Skipped full frontend Vitest suite', 'Skipped native WebView2 suite'],
        outstandingNative: false,
      };
      break;
    }

    case 'frontend': {
      const commands = [
        { executable: 'npm', args: ['run', 'typecheck'], cwd: 'apps/desktop' },
      ];
      if (classification.fullFrontend || !classification.frontendDirs?.length || classification.frontendDirs.length > 2) {
        commands.push({ executable: 'npm', args: ['test'], cwd: 'apps/desktop' });
      } else {
        const targets = classification.frontendDirs.map(d => `src/${d}/`);
        targets.push('src/featureBoundary.test.ts');
        commands.push({ executable: 'npm', args: ['test', '--', ...targets], cwd: 'apps/desktop' });
      }
      if (classification.needsTooling) {
        commands.push({ executable: 'node', args: ['scripts/run-tooling-tests.mjs'] });
      }
      plan = {
        category: 'frontend',
        description: 'Frontend component changes only.',
        commands,
        exclusions: ['Skipped Rust workspace test and compilation', 'Skipped native WebView2 suite'],
        outstandingNative: false,
      };
      break;
    }

    case 'cross-cutting': {
      const commands = [{ executable: 'cargo', args: ['fmt', '--all', '--check'] }];
      for (const crate of classification.crates) {
        commands.push({ executable: 'cargo', args: ['clippy', '-p', crate, '--all-targets', '--locked', '--', '-D', 'warnings'] });
        commands.push({ executable: 'cargo', args: ['test', '-p', crate, '--lib', '--bins', '--locked'] });
      }
      if (classification.integrationPrefixes?.length > 0) {
        const filters = deduplicateFilters(classification.integrationPrefixes);
        commands.push({ executable: 'cargo', args: ['test', '-p', 'webnovel-core', '--test', 'integration', '--locked', '--', ...filters] });
      }
      commands.push({ executable: 'npm', args: ['run', 'typecheck'], cwd: 'apps/desktop' });
      if (classification.fullFrontend || !classification.frontendDirs?.length || classification.frontendDirs.length > 2) {
        commands.push({ executable: 'npm', args: ['test'], cwd: 'apps/desktop' });
      } else {
        const targets = classification.frontendDirs.map(d => `src/${d}/`);
        targets.push('src/featureBoundary.test.ts');
        commands.push({ executable: 'npm', args: ['test', '--', ...targets], cwd: 'apps/desktop' });
      }
      if (classification.needsTooling) {
        commands.push({ executable: 'node', args: ['scripts/run-tooling-tests.mjs'] });
      }
      plan = {
        category: 'cross-cutting',
        description: `Scoped cross-cutting changes across Rust crates (${classification.crates.join(', ')}) and frontend.`,
        commands,
        exclusions: ['Skipped unrelated Rust crates', 'Skipped native WebView2 suite'],
        outstandingNative: false,
      };
      break;
    }

    case 'broad':
    default:
      plan = {
        category: 'broad',
        description: classification.reason || 'Broad, architectural, persistence, or root changes: full qualification required.',
        commands: [
          { executable: 'desktop', args: ['check'] },
        ],
        exclusions: ['No exclusions (fails closed to full check)'],
        outstandingNative: true,
      };
      break;
  }

  for (const cmd of plan.commands) {
    validateCommand(cmd, baseDir);
  }

  return plan;
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
  output += `  - ${plan.outstandingNative ? 'Outstanding: full native suite required.' : 'Not required for scoped changes.'}\n`;
  return output;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  const jsonMode = process.argv.includes('--json');
  const files = getChangedFiles();
  const classification = classifyChanges(files);
  const plan = planFromClassification(classification);
  if (jsonMode) {
    console.log(JSON.stringify({
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
