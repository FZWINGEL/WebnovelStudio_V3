import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { isAbsolute, join, relative, resolve } from 'node:path';
import { parseSync } from 'rolldown/utils';
import { describe, expect, it } from 'vitest';

type AstNode = { type?: string; [key: string]: unknown };

interface ManifestSource {
  path: string;
  sha256: string;
}

interface ManifestCommand {
  name: string;
  rustPath: string;
  required: string[];
  optional: string[];
}

interface CommandContract {
  version: 1;
  sources: ManifestSource[];
  commands: ManifestCommand[];
}

interface CallSite {
  file: string;
  command: string;
  arguments: string[];
  undefinedArguments: string[];
}

type ImportedBinding = 'invoke' | 'namespace' | 'root-core' | 'root-namespace';
type ScopeKind = 'program' | 'function' | 'block';

interface Scope {
  parent: Scope | null;
  kind: ScopeKind;
  bindings: Set<string>;
  functionScope: Scope;
}

interface ScanContext {
  file: string;
  source: string;
  rootScope: Scope;
  scopes: WeakMap<object, Scope>;
  imported: Map<string, ImportedBinding>;
  sites: CallSite[];
  contract: Map<string, ManifestCommand>;
}

const CORE_MODULE = '@tauri-apps/api/core';
const REQUIRED_SOURCE = 'apps/desktop/src-tauri/src/main.rs';
const KNOWN_BROWSER_GLOBALS = new Set(['window', 'globalThis', 'self']);
const DESKTOP_ROOT = existsSync(join(process.cwd(), 'src'))
  ? resolve(process.cwd())
  : resolve(process.cwd(), 'apps/desktop');
const REPO_ROOT = resolve(DESKTOP_ROOT, '..', '..');
const SOURCE_ROOT = join(DESKTOP_ROOT, 'src');
const MANIFEST_PATH = join(SOURCE_ROOT, 'ipc', 'tauriCommands.generated.json');

function isNode(value: unknown): value is AstNode {
  return value !== null && typeof value === 'object' && !Array.isArray(value) && 'type' in value;
}

function childNodes(node: AstNode): AstNode[] {
  const children: AstNode[] = [];
  for (const [key, value] of Object.entries(node)) {
    if (key === 'type' || key === 'start' || key === 'end' || key === 'range') continue;
    if (isNode(value)) children.push(value);
    else if (Array.isArray(value)) {
      for (const item of value) if (isNode(item)) children.push(item);
    }
  }
  return children;
}

function identifierName(node: unknown): string | null {
  return isNode(node) && node.type === 'Identifier' && typeof node.name === 'string' ? node.name : null;
}

function importName(node: unknown): string | null {
  if (isNode(node) && node.type === 'Literal' && typeof node.value === 'string') return node.value;
  return identifierName(node);
}

function literalString(node: unknown): string | null {
  if (isNode(node) && node.type === 'Literal' && typeof node.value === 'string') return node.value;
  if (isNode(node) && node.type === 'TemplateLiteral') {
    const expressions = Array.isArray(node.expressions) ? node.expressions : [];
    const quasis = Array.isArray(node.quasis) ? node.quasis : [];
    if (expressions.length === 0 && quasis.length === 1 && isNode(quasis[0])) {
      const value = quasis[0].value;
      if (value !== null && typeof value === 'object' && !Array.isArray(value) && typeof (value as { cooked?: unknown }).cooked === 'string') {
        return (value as { cooked: string }).cooked;
      }
    }
  }
  return null;
}

function unwrapExpression(node: AstNode): AstNode {
  let current = node;
  while (
    current.type === 'ParenthesizedExpression' ||
    current.type === 'TSAsExpression' ||
    current.type === 'TSSatisfiesExpression' ||
    current.type === 'TSNonNullExpression' ||
    current.type === 'TypeCastExpression'
  ) {
    if (!isNode(current.expression)) break;
    current = current.expression;
  }
  return current;
}

function patternNames(node: unknown, names: Set<string>): void {
  if (!isNode(node)) return;
  const name = identifierName(node);
  if (name) {
    names.add(name);
    return;
  }
  if (node.type === 'RestElement' || node.type === 'AssignmentPattern' || node.type === 'TSParameterProperty') {
    if (isNode(node.argument)) patternNames(node.argument, names);
    else if (isNode(node.left)) patternNames(node.left, names);
    else if (isNode(node.parameter)) patternNames(node.parameter, names);
    return;
  }
  if (node.type === 'Property') {
    patternNames(node.value, names);
    return;
  }
  if (node.type === 'ObjectPattern') {
    if (Array.isArray(node.properties)) for (const property of node.properties) patternNames(property, names);
    return;
  }
  if (node.type === 'ArrayPattern') {
    if (Array.isArray(node.elements)) for (const element of node.elements) patternNames(element, names);
  }
}

function addDeclaration(scope: Scope, node: AstNode): void {
  if (node.type === 'VariableDeclaration') {
    const target = node.kind === 'var' ? scope.functionScope : scope;
    if (Array.isArray(node.declarations)) {
      for (const declaration of node.declarations) if (isNode(declaration)) patternNames(declaration.id, target.bindings);
    }
  } else if (node.type === 'FunctionDeclaration' || node.type === 'ClassDeclaration') {
    const name = identifierName(node.id);
    if (name) scope.bindings.add(name);
  }
}

function addDirectDeclarations(scope: Scope, body: unknown): void {
  if (!Array.isArray(body)) return;
  for (const statement of body) {
    if (!isNode(statement)) continue;
    addDeclaration(scope, statement);
    if (statement.type === 'ImportDeclaration' && Array.isArray(statement.specifiers)) {
      for (const specifier of statement.specifiers) {
        if (isNode(specifier) && isNode(specifier.local)) {
          const name = identifierName(specifier.local);
          if (name) scope.bindings.add(name);
        }
      }
    }
  }
}

function collectHoistedVars(node: AstNode, functionScope: Scope): void {
  if (node.type === 'FunctionDeclaration' || node.type === 'FunctionExpression' || node.type === 'ArrowFunctionExpression' || node.type === 'ClassDeclaration' || node.type === 'ClassExpression') return;
  if (node.type === 'VariableDeclaration' && node.kind === 'var' && Array.isArray(node.declarations)) {
    for (const declaration of node.declarations) if (isNode(declaration)) patternNames(declaration.id, functionScope.bindings);
  }
  for (const child of childNodes(node)) collectHoistedVars(child, functionScope);
}

function makeScope(node: AstNode, parent: Scope | null, kind: ScopeKind, scopes: WeakMap<object, Scope>): Scope {
  const scope: Scope = {
    parent,
    kind,
    bindings: new Set<string>(),
    functionScope: undefined as unknown as Scope,
  };
  scope.functionScope = kind === 'function' || kind === 'program' ? scope : parent!.functionScope;
  scopes.set(node, scope);
  return scope;
}

function buildScopes(program: AstNode): { root: Scope; scopes: WeakMap<object, Scope> } {
  const scopes = new WeakMap<object, Scope>();
  const root = makeScope(program, null, 'program', scopes);

  function visit(node: AstNode, parentScope: Scope): void {
    if (node.type === 'Program') {
      addDirectDeclarations(root, node.body);
      collectHoistedVars(node, root);
      for (const child of childNodes(node)) visit(child, root);
      return;
    }
    if (node.type === 'BlockStatement' || node.type === 'StaticBlock' || node.type === 'TSModuleBlock') {
      const scope = makeScope(node, parentScope, 'block', scopes);
      addDirectDeclarations(scope, node.body);
      for (const child of childNodes(node)) visit(child, scope);
      return;
    }
    if (node.type === 'FunctionDeclaration' || node.type === 'FunctionExpression' || node.type === 'ArrowFunctionExpression') {
      const scope = makeScope(node, parentScope, 'function', scopes);
      const name = identifierName(node.id);
      if (name) scope.bindings.add(name);
      if (Array.isArray(node.params)) {
        for (const parameter of node.params) patternNames(parameter, scope.bindings);
      }
      collectHoistedVars(node.body && isNode(node.body) ? node.body : node, scope);
      for (const child of childNodes(node)) visit(child, scope);
      return;
    }
    if (node.type === 'ClassDeclaration' || node.type === 'ClassExpression') {
      const scope = makeScope(node, parentScope, 'block', scopes);
      const name = identifierName(node.id);
      if (name) scope.bindings.add(name);
      for (const child of childNodes(node)) visit(child, scope);
      return;
    }
    if (node.type === 'CatchClause') {
      const scope = makeScope(node, parentScope, 'block', scopes);
      patternNames(node.param, scope.bindings);
      for (const child of childNodes(node)) visit(child, scope);
      return;
    }
    if (node.type === 'ForStatement' || node.type === 'ForInStatement' || node.type === 'ForOfStatement') {
      const scope = makeScope(node, parentScope, 'block', scopes);
      const declaration = node.type === 'ForStatement' ? node.init : node.left;
      if (isNode(declaration) && declaration.type === 'VariableDeclaration') addDeclaration(scope, declaration);
      for (const child of childNodes(node)) visit(child, scope);
      return;
    }
    if (node.type === 'SwitchStatement') {
      const scope = makeScope(node, parentScope, 'block', scopes);
      if (Array.isArray(node.cases)) {
        for (const branch of node.cases) if (isNode(branch)) addDirectDeclarations(scope, branch.consequent);
      }
      for (const child of childNodes(node)) visit(child, scope);
      return;
    }
    for (const child of childNodes(node)) visit(child, parentScope);
  }

  visit(program, root);
  return { root, scopes };
}

function resolvedBinding(name: string, scope: Scope, root: Scope, imported: Map<string, ImportedBinding>): ImportedBinding | null {
  for (let current: Scope | null = scope; current; current = current.parent) {
    if (!current.bindings.has(name)) continue;
    return current === root ? imported.get(name) ?? null : null;
  }
  return null;
}

function isUnbound(name: string, scope: Scope): boolean {
  for (let current: Scope | null = scope; current; current = current.parent) {
    if (current.bindings.has(name)) return false;
  }
  return true;
}

function sourceLine(source: string, node: AstNode): number {
  const start = typeof node.start === 'number' ? node.start : 0;
  let line = 1;
  for (let index = 0; index < start && index < source.length; index += 1) if (source[index] === '\n') line += 1;
  return line;
}

function fail(context: ScanContext, node: AstNode, message: string): never {
  throw new Error(`${context.file}:${sourceLine(context.source, node)}: ${message}`);
}

function staticMemberName(node: unknown): string | null {
  if (isNode(node) && node.type === 'Identifier') return identifierName(node);
  return literalString(node);
}

function memberPath(node: unknown): string[] | null {
  const expression = isNode(node) ? unwrapExpression(node) : null;
  if (!expression) return null;
  const identifier = identifierName(expression);
  if (identifier) return [identifier];
  if (expression.type !== 'MemberExpression' && expression.type !== 'OptionalMemberExpression') return null;
  const property = staticMemberName(expression.property);
  if (expression.computed && property === null) return null;
  const object = memberPath(expression.object);
  return object && property ? [...object, property] : null;
}

function isTypeOnlyContext(node: AstNode, parent: AstNode | null): boolean {
  if (!parent) return false;
  if (parent.type === 'TSTypeAssertion') return parent.typeAnnotation === node;
  if (parent.type === 'TSAsExpression' || parent.type === 'TSSatisfiesExpression' || parent.type === 'TSNonNullExpression' || parent.type === 'TypeCastExpression' || parent.type === 'TSInstantiationExpression') {
    return parent.expression !== node;
  }
  return Boolean(parent.type?.startsWith('TS'));
}

function isDeclarationIdentifier(node: AstNode, parent: AstNode | null): boolean {
  if (!parent) return false;
  if (parent.type === 'VariableDeclarator' && parent.id === node) return true;
  if ((parent.type === 'FunctionDeclaration' || parent.type === 'FunctionExpression' || parent.type === 'ClassDeclaration') && parent.id === node) return true;
  if (parent.type === 'ImportSpecifier' || parent.type === 'ImportDefaultSpecifier' || parent.type === 'ImportNamespaceSpecifier') return true;
  if (parent.type === 'LabeledStatement' || parent.type === 'BreakStatement' || parent.type === 'ContinueStatement') return true;
  if (parent.type === 'Property' && parent.key === node && !parent.shorthand && !parent.computed) return true;
  if ((parent.type === 'MethodDefinition' || parent.type === 'PropertyDefinition') && parent.key === node && !parent.computed) return true;
  return false;
}

function isAllowedDirectBindingReference(
  node: AstNode,
  parent: AstNode | null,
  grandparent: AstNode | null,
  binding: ImportedBinding,
): boolean {
  if (binding === 'invoke' && parent?.type === 'CallExpression' && parent.callee === node) return true;
  if (
    binding === 'namespace' &&
    parent?.type === 'MemberExpression' &&
    parent.object === node &&
    !parent.computed &&
    (identifierName(parent.property) !== 'invoke' || (grandparent?.type === 'CallExpression' && grandparent.callee === parent))
  ) return true;
  if (
    binding === 'root-namespace' &&
    parent?.type === 'MemberExpression' &&
    parent.object === node &&
    !parent.computed &&
    identifierName(parent.property) !== 'core'
  ) return true;
  return false;
}

function isDefinitelyUndefined(node: unknown, scope: Scope): boolean {
  const expression = isNode(node) ? unwrapExpression(node) : null;
  if (!expression) return false;
  if (expression.type === 'Identifier' && identifierName(expression) === 'undefined') {
    return isUnbound('undefined', scope);
  }
  return expression.type === 'UnaryExpression' && expression.operator === 'void';
}

function argumentKeys(context: ScanContext, node: AstNode, scope: Scope): { keys: string[]; undefinedKeys: string[] } {
  const expression = unwrapExpression(node);
  if (expression.type !== 'ObjectExpression') fail(context, node, 'invoke argument payload must be an object literal; resolve indirect values before invoking');
  const keys: string[] = [];
  const undefinedKeys: string[] = [];
  if (!Array.isArray(expression.properties)) fail(context, expression, 'invoke argument payload is malformed');
  for (const property of expression.properties) {
    if (!isNode(property)) fail(context, expression, 'invoke argument payload contains an unsupported property');
    if (property.type === 'SpreadElement' || property.type === 'SpreadProperty') {
      fail(context, property, 'invoke argument spreads are unresolved; expand the keys explicitly');
    }
    if (property.type !== 'Property' || property.kind !== 'init' || property.method) {
      fail(context, property, 'invoke argument getters, setters and methods are unresolved');
    }
    if (property.computed) fail(context, property, 'invoke argument computed keys are unresolved; use a literal key');
    const key = staticMemberName(property.key);
    if (!key) fail(context, property, 'invoke argument key must be an identifier or string literal');
    if (keys.includes(key)) fail(context, property, `duplicate invoke argument key '${key}'`);
    keys.push(key);
    if (isDefinitelyUndefined(property.value, scope)) undefinedKeys.push(key);
  }
  return { keys, undefinedKeys };
}

function invokeCall(context: ScanContext, node: AstNode, scope: Scope, direct: boolean): void {
  if (!direct) return;
  if (node.optional) fail(context, node, 'optional invoke calls are unsupported; call the imported binding directly');
  if (!Array.isArray(node.arguments) || node.arguments.length === 0) fail(context, node, 'invoke command name must be a literal string');
  const command = literalString(node.arguments[0]);
  if (command === null) fail(context, node.arguments[0] as AstNode, 'invoke command name must be statically resolved to a literal string');
  if (node.arguments.length > 2) fail(context, node, 'invoke options are unsupported by the static contract checker; resolve command and wire arguments directly');
  let argumentsFound: string[] = [];
  let undefinedArguments: string[] = [];
  if (node.arguments.length === 2) {
    const payload = node.arguments[1];
    if (!isNode(payload)) fail(context, node, 'invoke argument payload is unresolved');
    ({ keys: argumentsFound, undefinedKeys: undefinedArguments } = argumentKeys(context, payload, scope));
  }
  const registered = context.contract.get(command);
  if (!registered) fail(context, node, `invoke command '${command}' is not registered in tauriCommands.generated.json`);
  const expected = new Set([...registered.required, ...registered.optional]);
  const missing = registered.required.filter(key => !argumentsFound.includes(key));
  if (missing.length) fail(context, node, `invoke command '${command}' is missing required argument key(s): ${missing.join(', ')}`);
  const extra = argumentsFound.filter(key => !expected.has(key));
  if (extra.length) fail(context, node, `invoke command '${command}' has unknown argument key(s): ${extra.join(', ')}`);
  const explicitUndefined = undefinedArguments.filter(key => registered.required.includes(key));
  if (explicitUndefined.length) fail(context, node, `invoke command '${command}' has explicitly undefined required key(s): ${explicitUndefined.join(', ')}`);
  context.sites.push({ file: context.file, command, arguments: argumentsFound, undefinedArguments });
}

function inspectImport(context: ScanContext, node: AstNode): void {
  const source = literalString(node.source);
  if (!source || !Array.isArray(node.specifiers)) return;
  if (source === CORE_MODULE) {
    for (const specifier of node.specifiers) {
      if (!isNode(specifier)) continue;
      if (specifier.type === 'ImportSpecifier') {
        if (importName(specifier.imported) === 'invoke') {
          const local = identifierName(specifier.local) ?? 'invoke';
          context.imported.set(local, 'invoke');
        }
      } else if (specifier.type === 'ImportNamespaceSpecifier') {
        const local = identifierName(specifier.local);
        if (local) context.imported.set(local, 'namespace');
      } else {
        fail(context, specifier, 'default imports from the Tauri core bridge are unsupported');
      }
    }
    return;
  }
  if (source === '@tauri-apps/api') {
    for (const specifier of node.specifiers) {
      if (!isNode(specifier)) continue;
      const imported = specifier.type === 'ImportSpecifier' ? importName(specifier.imported) : null;
      const local = identifierName(specifier.local);
      if (imported === 'core' && local) context.imported.set(local, 'root-core');
      else if (specifier.type === 'ImportNamespaceSpecifier' || specifier.type === 'ImportDefaultSpecifier') {
        if (local) context.imported.set(local, 'root-namespace');
      }
    }
  }
  for (const specifier of node.specifiers) {
    if (isNode(specifier) && specifier.type === 'ImportSpecifier' && importName(specifier.imported) === 'invoke') {
      fail(context, specifier, `invoke imported through '${source}' is an unresolved re-export; import it from ${CORE_MODULE}`);
    }
  }
}

function scanSource(file: string, source: string, contract: CommandContract): CallSite[] {
  let parsed: { program?: unknown; errors?: unknown[] };
  try {
    const extension = file.toLowerCase().split('.').pop();
    const lang = extension === 'tsx' ? 'tsx' : extension === 'jsx' ? 'jsx' : extension === 'js' ? 'js' : 'ts';
    parsed = parseSync(file, source, { lang, sourceType: 'module', astType: 'ts' });
  } catch (error) {
    throw new Error(`${file}: unable to parse production source: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (parsed.errors?.length) throw new Error(`${file}: unable to parse production source: ${JSON.stringify(parsed.errors)}`);
  if (!isNode(parsed.program)) throw new Error(`${file}: parser returned no program`);
  const { root, scopes } = buildScopes(parsed.program);
  const commands = new Map(contract.commands.map(command => [command.name, command]));
  const context: ScanContext = { file, source, rootScope: root, scopes, imported: new Map(), sites: [], contract: commands };
  if (Array.isArray(parsed.program.body)) {
    for (const statement of parsed.program.body) if (isNode(statement) && statement.type === 'ImportDeclaration') inspectImport(context, statement);
  }

  function visit(node: AstNode, currentScope: Scope, parent: AstNode | null, grandparent: AstNode | null): void {
    const scope = context.scopes.get(node) ?? currentScope;
    if (node.type === 'ImportDeclaration') {
      inspectImport(context, node);
      return;
    }
    if (node.type === 'ExportNamedDeclaration' || node.type === 'ExportAllDeclaration') {
      const sourceName = literalString(node.source);
      if (sourceName === CORE_MODULE || (sourceName === '@tauri-apps/api' && node.type === 'ExportAllDeclaration')) {
        fail(context, node, 'Tauri core re-exports are unsupported; invoke calls must remain in a parsed production module');
      }
      if (Array.isArray(node.specifiers)) {
        for (const specifier of node.specifiers) {
          if (!isNode(specifier)) continue;
          const local = importName(specifier.local);
          const exported = importName(specifier.exported);
          if (literalString(node.source) !== null && (local === 'invoke' || exported === 'invoke')) {
            fail(context, specifier, 'an invoke binding escapes through a re-export');
          }
          if (local && resolvedBinding(local, scope, root, context.imported)) {
            fail(context, specifier, 'an imported invoke binding escapes through a re-export');
          }
        }
      }
    }
    if (node.type === 'ImportExpression' && [CORE_MODULE, '@tauri-apps/api'].includes(literalString(node.source) ?? '')) {
      fail(context, node, 'dynamic imports of the Tauri core bridge are unsupported');
    }
    if (node.type === 'TSImportEqualsDeclaration' && isNode(node.moduleReference) && node.moduleReference.type === 'TSExternalModuleReference' && [CORE_MODULE, '@tauri-apps/api'].includes(literalString(node.moduleReference.expression) ?? '')) {
      fail(context, node, 'require() of the Tauri core bridge bypasses the generated invoke contract');
    }
    if (node.type === 'MemberExpression' || node.type === 'OptionalMemberExpression') {
      const path = memberPath(node);
      if (node.computed && literalString(node.property) === null && isNode(node.object)) {
        const object = unwrapExpression(node.object);
        const global = identifierName(object);
        if (global && KNOWN_BROWSER_GLOBALS.has(global) && isUnbound(global, scope)) {
          fail(context, node, `raw Tauri bridge access uses an unresolved computed property on ${global}; use a literal path or the generated invoke binding`);
        }
      }
      if (path?.includes('__TAURI_INTERNALS__') || path?.includes('__TAURI__')) {
        fail(context, node, 'raw Tauri bridge access bypasses the generated invoke contract');
      }
      const first = path?.[0];
      const firstBinding = first ? resolvedBinding(first, scope, root, context.imported) : null;
      if ((firstBinding === 'root-core' && path && path.length > 1) || (firstBinding === 'root-namespace' && path?.[1] === 'core')) {
        fail(context, node, 'the Tauri core bridge must be imported directly from @tauri-apps/api/core');
      }
    }
    if (node.type === 'CallExpression') {
      const path = memberPath(node.callee);
      if (path?.includes('__TAURI_INTERNALS__') || path?.includes('__TAURI__')) {
        fail(context, node, 'raw Tauri bridge calls bypass the generated invoke contract');
      }
      if (identifierName(node.callee) === 'require' && Array.isArray(node.arguments) && [CORE_MODULE, '@tauri-apps/api'].includes(literalString(node.arguments[0]) ?? '')) {
        fail(context, node, 'require() of the Tauri core bridge bypasses the generated invoke contract');
      }
      const callee = unwrapExpression(node.callee as AstNode);
      let direct = false;
      if (callee.type === 'Identifier') {
        direct = resolvedBinding(identifierName(callee)!, scope, root, context.imported) === 'invoke';
      } else if (callee.type === 'MemberExpression' && !callee.computed && identifierName(callee.property) === 'invoke') {
        const object = unwrapExpression(callee.object as AstNode);
        direct = object.type === 'Identifier' && resolvedBinding(identifierName(object)!, scope, root, context.imported) === 'namespace';
      }
      invokeCall(context, node, scope, direct);
    }
    if (node.type === 'Identifier') {
      const name = identifierName(node)!;
      const binding = resolvedBinding(name, scope, root, context.imported);
      if (binding && !isTypeOnlyContext(node, parent) && !isDeclarationIdentifier(node, parent) && !isAllowedDirectBindingReference(node, parent, grandparent, binding)) {
        fail(context, node, `imported ${binding === 'invoke' ? 'invoke' : 'Tauri namespace'} binding escapes its statically checked call shape`);
      }
    }
    for (const child of childNodes(node)) visit(child, scope, node, parent);
  }

  visit(parsed.program, root, null, null);
  return context.sites;
}

function productionFiles(directory: string, out: string[] = []): string[] {
  for (const entry of readdirSync(directory)) {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) {
      productionFiles(path, out);
      continue;
    }
    const lower = entry.toLowerCase();
    if (!/\.(ts|tsx|js|jsx)$/.test(lower) || /(?:\.test|\.spec)\.(?:ts|tsx|js|jsx)$/.test(lower) || lower.endsWith('.d.ts')) continue;
    out.push(path);
  }
  return out;
}

function normalizeSource(source: string): string {
  return source.replace(/\r\n/g, '\n');
}

function sourceHash(source: string): string {
  return createHash('sha256').update(normalizeSource(source), 'utf8').digest('hex');
}

function plainObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

function stringArray(value: unknown, label: string): string[] {
  if (!Array.isArray(value) || value.some(item => typeof item !== 'string')) throw new Error(`${label} must be an array of strings`);
  const result = value as string[];
  if (new Set(result).size !== result.length) throw new Error(`${label} contains duplicate entries`);
  return result;
}

function validateManifest(value: unknown, verifyHashes: boolean): CommandContract {
  if (!plainObject(value) || value.version !== 1) throw new Error('tauri command manifest must have version 1');
  if (!Array.isArray(value.sources) || !Array.isArray(value.commands)) throw new Error('tauri command manifest must contain sources and commands arrays');
  const sources: ManifestSource[] = [];
  const sourcePaths = new Set<string>();
  for (const item of value.sources) {
    if (!plainObject(item) || typeof item.path !== 'string' || typeof item.sha256 !== 'string' || !/^[a-f0-9]{64}$/.test(item.sha256)) {
      throw new Error('tauri command manifest source entries require a relative path and lowercase SHA-256 hash');
    }
    const path = item.path.replace(/\\/g, '/');
    if (isAbsolute(path) || path.split('/').includes('..')) throw new Error(`tauri command manifest source path escapes the repository: ${path}`);
    if (sourcePaths.has(path)) throw new Error(`tauri command manifest contains duplicate source path: ${path}`);
    sourcePaths.add(path);
    sources.push({ path, sha256: item.sha256 });
  }
  if (sources.length === 0) throw new Error('tauri command manifest must contain at least one source hash');
  if (!sourcePaths.has(REQUIRED_SOURCE)) throw new Error(`tauri command manifest must hash ${REQUIRED_SOURCE}`);
  const commands: ManifestCommand[] = [];
  const commandNames = new Set<string>();
  const rustPaths = new Set<string>();
  for (const item of value.commands) {
    if (!plainObject(item) || typeof item.name !== 'string' || !item.name || typeof item.rustPath !== 'string') throw new Error('tauri command manifest command entries require nonempty name and rustPath');
    if (!item.rustPath) throw new Error(`tauri command manifest command '${item.name}' has an empty rustPath`);
    if (commandNames.has(item.name)) throw new Error(`tauri command manifest contains duplicate command: ${item.name}`);
    if (rustPaths.has(item.rustPath)) throw new Error(`tauri command manifest contains duplicate rustPath: ${item.rustPath}`);
    commandNames.add(item.name);
    rustPaths.add(item.rustPath);
    const required = stringArray(item.required, `required arguments for ${item.name}`);
    const optional = stringArray(item.optional, `optional arguments for ${item.name}`);
    if (required.some(key => optional.includes(key))) throw new Error(`tauri command manifest overlaps required and optional arguments for ${item.name}`);
    commands.push({ name: item.name, rustPath: item.rustPath, required, optional });
  }
  if (commands.length === 0) throw new Error('tauri command manifest must contain at least one command');
  if (verifyHashes) {
    for (const source of sources) {
      const path = resolve(REPO_ROOT, source.path);
      const actualPath = relative(REPO_ROOT, path);
      if (actualPath.startsWith('..') || isAbsolute(actualPath)) throw new Error(`tauri command manifest source path escapes the repository: ${source.path}`);
      let current: string;
      try {
        current = readFileSync(path, 'utf8');
      } catch {
        throw new Error(`tauri command manifest source is missing: ${source.path}. Run cargo run -p wns-bindings.`);
      }
      if (sourceHash(current) !== source.sha256) {
        throw new Error(`tauri command manifest is stale for ${source.path}. Run cargo run -p wns-bindings.`);
      }
    }
  }
  return { version: 1, sources, commands };
}

function currentContract(): CommandContract {
  let value: unknown;
  try {
    value = JSON.parse(readFileSync(MANIFEST_PATH, 'utf8'));
  } catch {
    throw new Error(`unable to read ${relative(DESKTOP_ROOT, MANIFEST_PATH)}; run cargo run -p wns-bindings`);
  }
  return validateManifest(value, true);
}

function fixtureContract(): CommandContract {
  return validateManifest({
    version: 1,
    sources: [{ path: REQUIRED_SOURCE, sha256: '0'.repeat(64) }],
    commands: [
      { name: 'foo', rustPath: 'fixture::foo', required: ['required'], optional: ['optional'] },
      { name: 'renamedCommand', rustPath: 'fixture::renamed', required: [], optional: [] },
    ],
  }, false);
}

function fixtureScan(source: string): CallSite[] {
  return scanSource('fixture.ts', source, fixtureContract());
}

describe('the parsed IPC call contract', () => {
  it('rejects malformed or duplicate command manifests', () => {
    const main = { path: REQUIRED_SOURCE, sha256: '0'.repeat(64) };
    expect(() => validateManifest({ version: 1, sources: [main], commands: [{ name: 'foo', rustPath: 'a', required: [], optional: [] }, { name: 'foo', rustPath: 'b', required: [], optional: [] }] }, false)).toThrow(/duplicate command/);
    expect(() => validateManifest({ version: 1, sources: [main, { path: 'x', sha256: '0'.repeat(64) }, { path: 'x', sha256: '0'.repeat(64) }], commands: [{ name: 'foo', rustPath: 'a', required: [], optional: [] }] }, false)).toThrow(/duplicate source/);
  });

  it('requires current source hashes before using the generated contract', () => {
    expect(() => validateManifest({ version: 1, sources: [{ path: REQUIRED_SOURCE }], commands: [{ name: 'foo', rustPath: 'a', required: [], optional: [] }] }, false)).toThrow(/source entries/);
    expect(() => validateManifest({ version: 1, sources: [{ path: REQUIRED_SOURCE, sha256: '0'.repeat(64) }], commands: [{ name: 'foo', rustPath: 'a', required: [], optional: [] }] }, true)).toThrow(/stale/);
  });

  it('resolves named aliases and namespaces with lexical shadowing, comments and strings ignored', () => {
    const sites = fixtureScan(`
      import { invoke as call } from '@tauri-apps/api/core';
      import * as tauri from '@tauri-apps/api/core';
      // call('fake', { required: undefined });
      const example = "tauri.invoke('fake', { required: undefined })";
      call<string>('foo', { required, optional: value });
      tauri.invoke('foo', { required });
      {
        call('foo', { required });
        const call = () => undefined;
        call('foo', { required });
      }
      function shadow(call: unknown) {
        call('foo', { required });
      }
    `);
    expect(sites.map(site => site.command)).toEqual(['foo', 'foo']);
    expect(sites[1]?.arguments).toEqual(['required']);
  });

  it('supports multiline generic calls and omission of optional keys', () => {
    const sites = fixtureScan(`
      import { invoke as run } from '@tauri-apps/api/core';
      run<unknown>(
        \`foo\`,
        { required },
      );
    `);
    expect(sites).toHaveLength(1);
    expect(sites[0]?.arguments).toEqual(['required']);
  });

  it('resolves imports before their source order and allows unrelated Tauri modules', () => {
    const sites = fixtureScan(`
      call('foo', { required });
      import { invoke as call } from '@tauri-apps/api/core';
      import { listen } from '@tauri-apps/api/event';
      import { getCurrentWindow } from '@tauri-apps/api/window';
      void listen;
      void getCurrentWindow;
    `);
    expect(sites).toHaveLength(1);
  });

  it('keeps function var hoisting and named class-expression scopes lexical', () => {
    const sites = fixtureScan(`
      import { invoke as call } from '@tauri-apps/api/core';
      function shadowed() {
        if (true) var call = local;
        call('foo', { required });
      }
      const localClass = class call {
        method() { call('foo', { required }); }
      };
      call('foo', { required });
    `);
    expect(sites).toHaveLength(1);
  });

  it('does not treat shadowed browser-global names as raw bridges', () => {
    const sites = fixtureScan(`
      import { invoke as call } from '@tauri-apps/api/core';
      function shadowed(window) {
        const raw = '__TAURI_INTERNALS__';
        window[raw].invoke('missing', {});
      }
      call('foo', { required });
    `);
    expect(sites).toHaveLength(1);
  });

  it('accepts an externally renamed command without applying the old snake_case rule', () => {
    const sites = fixtureScan(`import { invoke as call } from '@tauri-apps/api/core'; call('renamedCommand');`);
    expect(sites[0]?.command).toBe('renamedCommand');
  });

  it('resolves string-literal ESM import names before joining the command', () => {
    expect(() => fixtureScan(`import { 'invoke' as call } from '@tauri-apps/api/core'; call('missing');`)).toThrow(/not registered/);
  });

  it.each([
    ['dynamic command names', `const command = 'foo'; call(command, { required });`, /command name must be statically resolved/],
    ['indirect payloads', `const payload = { required }; call('foo', payload);`, /payload must be an object literal/],
    ['object spreads', `call('foo', { ...payload, required });`, /argument spreads are unresolved/],
    ['computed keys', `call('foo', { [key]: required });`, /computed keys are unresolved/],
    ['binding escapes', `const escaped = call; escaped('foo', { required });`, /binding escapes/],
    ['runtime cast escapes', `const escaped = call as typeof call; escaped('foo', { required });`, /binding escapes/],
    ['runtime type assertion escapes', `const escaped = <typeof call>call; escaped('foo', { required });`, /binding escapes/],
    ['raw bridges', `window.__TAURI_INTERNALS__.invoke('foo', { required });`, /raw Tauri bridge/],
    ['template raw bridges', "window[`__TAURI_INTERNALS__`].invoke('missing', {});", /raw Tauri bridge/],
    ['raw bridge aliases', `const bridge = window.__TAURI_INTERNALS__; bridge.invoke('foo', { required });`, /raw Tauri bridge/],
    ['computed raw bridges', `const raw = '__TAURI_INTERNALS__'; window[raw].invoke('missing', {});`, /computed property on window/],
    ['computed raw bridge aliases', `const raw = '__TAURI_INTERNALS__'; const bridge = window[raw]; bridge.invoke('missing', {});`, /computed property on window/],
    ['dynamic core imports', `import('@tauri-apps/api/core').then(module => module.invoke('foo'));`, /dynamic imports/],
    ['root core imports', `import { core } from '@tauri-apps/api'; core.invoke('foo', { required });`, /core bridge must be imported directly/],
    ['root namespace imports', `import * as tauri from '@tauri-apps/api'; tauri.core.invoke('foo', { required });`, /core bridge must be imported directly/],
    ['require core imports', `const core = require('@tauri-apps/api/core'); core.invoke('foo', { required });`, /require\(\) of the Tauri core bridge/],
    ['TypeScript import-equals', `import core = require('@tauri-apps/api/core'); core.invoke('foo', { required });`, /require\(\) of the Tauri core bridge/],
    ['unsupported invoke options', `call('foo', { required }, { timeout: 1 });`, /invoke options are unsupported/],
  ])('rejects %s with an actionable diagnostic', (_label, body, error) => {
    expect(() => fixtureScan(`import { invoke as call } from '@tauri-apps/api/core'; ${body}`)).toThrow(error);
  });

  it('rejects re-exports and imported invoke bindings from another module', () => {
    expect(() => fixtureScan(`export { invoke } from '@tauri-apps/api/core';`)).toThrow(/re-exports/);
    expect(() => fixtureScan(`export { invoke } from './bridge';`)).toThrow(/re-export/);
    expect(() => fixtureScan(`import { invoke as call } from './bridge'; call('foo', { required });`)).toThrow(/re-export/);
  });

  it.each([
    ['unregistered commands', `call('missing', { required });`, /not registered/],
    ['missing required keys', `call('foo', { optional });`, /missing required/],
    ['extra keys', `call('foo', { required, extra });`, /unknown argument/],
    ['explicit undefined required keys', `call('foo', { required: undefined });`, /explicitly undefined/],
  ])('joins calls to the manifest and rejects %s', (_label, body, error) => {
    expect(() => fixtureScan(`import { invoke as call } from '@tauri-apps/api/core'; ${body}`)).toThrow(error);
  });

  it('scans every production source file and joins every real call to current Rust hashes', () => {
    const contract = currentContract();
    const sites = productionFiles(SOURCE_ROOT).flatMap(path => {
      const file = relative(SOURCE_ROOT, path).replace(/\\/g, '/');
      return scanSource(file, readFileSync(path, 'utf8'), contract);
    });
    expect(sites.length).toBeGreaterThan(100);
    expect(new Set(sites.map(site => site.file)).size).toBeGreaterThan(10);
    expect(sites.flatMap(site => site.arguments).length).toBeGreaterThan(100);
    expect(sites.every(site => contract.commands.some(command => command.name === site.command))).toBe(true);
  });
});
