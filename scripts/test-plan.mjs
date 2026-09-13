import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

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

export function getChangedFiles(baseDir = root) {
  try {
    const diff = execFileSync('git', ['status', '--porcelain'], { cwd: baseDir, encoding: 'utf8' });
    const lines = diff.split('\n').filter(Boolean);
    return lines.map(line => {
      const match = line.slice(3).trim();
      return match.split(' -> ').pop().replaceAll('\\', '/');
    });
  } catch {
    return [];
  }
}

export function classifyChanges(files) {
  if (!files || files.length === 0) {
    return { category: 'none', files: [] };
  }

  const normalized = files.map(f => String(f).replaceAll('\\', '/').replace(/^\.\//, ''));

  // If any root configuration, workflow, build or cargo file is touched -> broad full check
  const isBroad = normalized.some(f =>
    f.startsWith('.github/') ||
    f.startsWith('.cargo/') ||
    f === 'Cargo.toml' ||
    f === 'Cargo.lock' ||
    f === 'apps/desktop/package.json' ||
    f === 'apps/desktop/package-lock.json' ||
    f === 'apps/desktop/src-tauri/tauri.conf.json' ||
    f === 'apps/desktop/src-tauri/Cargo.toml' ||
    f.startsWith('crates/core/') ||
    f.startsWith('crates/architecture/')
  );
  if (isBroad) {
    return { category: 'broad', files: normalized };
  }

  // Check docs only
  const isDocOnly = normalized.every(f =>
    f.startsWith('docs/') ||
    ['AGENTS.md', 'CLAUDE.md', 'GEMINI.md', 'PRODUCT.md', 'README.md', 'CHANGELOG.md', 'DESIGN.md'].includes(f)
  );
  if (isDocOnly) {
    return { category: 'docs', files: normalized };
  }

  // Check contract / bindings
  const touchesContracts = normalized.some(f => f.startsWith('contracts/') || f.startsWith('crates/bindings/'));
  if (touchesContracts) {
    return { category: 'contracts', files: normalized };
  }

  // Check crates
  const crateMatches = new Set();
  const nonCrateFiles = [];
  for (const f of normalized) {
    let matched = false;
    for (const [prefix, config] of Object.entries(CRATE_MAPPINGS)) {
      if (f.startsWith(prefix + '/')) {
        crateMatches.add(config);
        matched = true;
        break;
      }
    }
    if (!matched) nonCrateFiles.push(f);
  }

  if (crateMatches.size > 0 && nonCrateFiles.length === 0) {
    return { category: 'isolated-rust', crates: Array.from(crateMatches), files: normalized };
  }

  // Check frontend only
  const isFrontend = normalized.every(f => f.startsWith('apps/desktop/src/') || f.startsWith('apps/desktop/public/'));
  if (isFrontend) {
    return { category: 'frontend', files: normalized };
  }

  // Mixed or unknown -> broad
  return { category: 'broad', files: normalized };
}

export function planFromClassification(classification) {
  switch (classification.category) {
    case 'none':
      return {
        category: 'clean',
        description: 'Working tree is clean.',
        commands: [
          { executable: 'cargo', args: ['fmt', '--all', '--check'] },
          { executable: 'node', args: ['--test', 'scripts/*.test.mjs'] },
        ],
        exclusions: ['Skipped full workspace compilation and frontend tests'],
      };

    case 'docs':
      return {
        category: 'docs',
        description: 'Documentation changes only.',
        commands: [
          { executable: 'node', args: ['--test', 'scripts/*.test.mjs'] },
        ],
        exclusions: ['Skipped Rust compilation and frontend Vitest suite'],
      };

    case 'isolated-rust': {
      const commands = [{ executable: 'cargo', args: ['fmt', '--all', '--check'] }];
      const prefixes = new Set();
      for (const c of classification.crates) {
        commands.push({ executable: 'cargo', args: ['clippy', '-p', c.crate, '--all-targets', '--locked', '--', '-D', 'warnings'] });
        commands.push({ executable: 'cargo', args: ['test', '-p', c.crate, '--lib', '--bins', '--locked'] });
        c.integrationPrefixes.forEach(p => prefixes.add(p));
      }
      for (const prefix of prefixes) {
        commands.push({ executable: 'cargo', args: ['test', '-p', 'webnovel-core', '--test', 'integration', '--', prefix] });
      }
      return {
        category: 'isolated-rust',
        description: `Scoped Rust changes in: ${classification.crates.map(c => c.crate).join(', ')}`,
        commands,
        exclusions: ['Skipped unrelated Rust crates', 'Skipped frontend Vitest suite', 'Skipped native WebView2 suite'],
      };
    }

    case 'frontend': {
      const subdirs = new Set();
      for (const f of classification.files) {
        const rel = f.replace('apps/desktop/src/', '');
        const part = rel.split('/')[0];
        if (part && !part.includes('.')) subdirs.add(part);
      }
      const commands = [
        { executable: 'npm', args: ['run', 'test:types'], cwd: 'apps/desktop' },
      ];
      if (subdirs.size > 0 && subdirs.size <= 2) {
        for (const dir of subdirs) {
          commands.push({ executable: 'npm', args: ['test', '--', `src/${dir}/`], cwd: 'apps/desktop' });
        }
      } else {
        commands.push({ executable: 'npm', args: ['test'], cwd: 'apps/desktop' });
      }
      return {
        category: 'frontend',
        description: 'Frontend component changes only.',
        commands,
        exclusions: ['Skipped Rust workspace test and compilation', 'Skipped native WebView2 suite'],
      };
    }

    case 'contracts':
      return {
        category: 'contracts',
        description: 'Shared contract or bindings changes.',
        commands: [
          { executable: 'cargo', args: ['clippy', '-p', 'wns-bindings', '-p', 'contracts', '--all-targets', '--locked', '--', '-D', 'warnings'] },
          { executable: 'cargo', args: ['test', '-p', 'wns-bindings', '-p', 'contracts', '--locked'] },
          { executable: 'cargo', args: ['test', '-p', 'webnovel-core', '--test', 'integration', '--', 'structured_contract_golden'] },
          { executable: 'npm', args: ['run', 'test:types'], cwd: 'apps/desktop' },
          { executable: 'npm', args: ['test', '--', 'src/kernel/'], cwd: 'apps/desktop' },
        ],
        exclusions: ['Skipped unrelated concern crates', 'Skipped full frontend Vitest suite'],
      };

    case 'broad':
    default:
      return {
        category: 'broad',
        description: 'Broad, architectural, persistence, or root changes: full qualification required.',
        commands: [
          { executable: 'desktop', args: ['check'] },
        ],
        exclusions: ['No exclusions (fails closed to full check)'],
      };
  }
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
  return output;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(fileURLToPath(import.meta.url))) {
  const files = getChangedFiles();
  const classification = classifyChanges(files);
  const plan = planFromClassification(classification);
  console.log(formatPlan(plan));
}
