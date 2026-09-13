// Opt-in live Codex Exec qualification for the native chat-first surface.
//
// This is deliberately outside the native CI consumer. The default and
// contracts modes use two explicitly submitted requests; grouped mode uses
// one explicitly submitted request that returns two review drafts. None of
// the modes retries or falls back, and each keeps packet/result evidence
// required to audit the run. These are synthetic contract trials, not prose-
// quality benchmarks.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { DatabaseSync } from 'node:sqlite';
import { createServer } from 'node:net';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { createReadStream } from 'node:fs';
import { mkdtemp, mkdir, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { isAbsolute, relative, resolve, sep, toNamespacedPath } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright-core';
import { spawnOwned, stopOwned, markOwnedReady } from './owned-process.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));
const groupedMode = process.env.WNS_V3_CHAT_LIVE_MODE === 'grouped';
const contractMode = process.env.WNS_V3_CHAT_LIVE_MODE === 'contracts';
const output = resolve(root, groupedMode
  ? '.local/native-results/chat-live-grouped'
  : contractMode ? '.local/native-results/chat-live-contracts' : '.local/native-results/chat-live');
const executable = resolve(process.env.WNS_V3_NATIVE_EXE ?? resolve(root, 'target/debug/webnovel-desktop.exe'));
const execFileAsync = promisify(execFile);
const requestedSelection = {
  providerId: 'codex',
  modelId: 'gpt-5.6-luna',
  reasoning: 'xhigh',
  serviceTier: 'priority',
};
const limitations = [
  'One synthetic live qualification trial only; this is not a prose-quality or latency benchmark.',
  'The trial qualifies the Codex Exec adapter and native persistence contract only; it does not qualify app-server parity.',
  'The trial does not qualify author usability, screen readers, IME behavior, installed packaging, provider billing limits, or long-form literary quality.',
];

await mkdir(output, { recursive: true });
const report = {
  status: 'disabled',
  enabled: process.env.WNS_V3_ALLOW_LIVE_CHAT === '1',
  mode: groupedMode ? 'grouped' : contractMode ? 'contracts' : 'default',
  executable,
  checks: [],
  limitations,
};
const saveReport = () => writeFile(resolve(output, 'report.json'), JSON.stringify(report, null, 2));

if (!report.enabled) {
  report.checks.push({ description: 'Live Codex chat qualification is disabled by default.', passed: true });
  await saveReport();
  const requestHint = groupedMode ? 'one grouped live request' : 'the two live requests';
  console.log(JSON.stringify({ status: report.status, output, hint: `Set WNS_V3_ALLOW_LIVE_CHAT=1 to explicitly authorize ${requestHint}.` }, null, 2));
  process.exit(0);
}

// The output directory is evidence. A passing run must not leave the previous
// run's failure artifacts beside its own report, where they read as a failure
// that did not happen.
await rm(resolve(output, 'failure.png'), { force: true });
await rm(resolve(output, 'failure.txt'), { force: true });

report.status = 'started';
await saveReport();

async function hashFile(path) {
  const hash = createHash('sha256');
  await new Promise((resolvePromise, reject) => {
    const stream = createReadStream(path);
    stream.on('data', chunk => hash.update(chunk));
    stream.on('error', reject);
    stream.on('end', resolvePromise);
  });
  return hash.digest('hex');
}

async function sourceIdentity() {
  const run = async args => {
    try {
      const result = await execFileAsync('git', args, { cwd: root, windowsHide: true, timeout: 10_000 });
      return result.stdout.trim();
    } catch (error) {
      return `unavailable: ${error.message}`;
    }
  };
  return {
    repository: root,
    commit: await run(['rev-parse', 'HEAD']),
    dirtyFiles: await run(['status', '--porcelain']),
    executablePath: executable,
    executableSha256: await hashFile(executable),
  };
}

const data = await realpath(await mkdtemp(resolve(tmpdir(), 'wns-v3-chat-live-')));
const identity = await sourceIdentity();
report.identity = { ...identity, isolatedDataDirectory: data };
await writeFile(resolve(output, 'source-identity.json'), JSON.stringify(report.identity, null, 2));

async function reservePort() {
  const server = createServer();
  await new Promise(done => server.listen(0, '127.0.0.1', done));
  const port = server.address().port;
  await new Promise(done => server.close(done));
  return port;
}

let port = await reservePort();

let app;
let browser;
let page;
let database;
let appLog = '';
let spawnError;
const pageErrors = [];

function launch() {
  const child = spawnOwned(executable, [], {
    cwd: data,
    windowsHide: true,
    stdio: 'pipe',
    env: {
      ...process.env,
      WNS_V3_NATIVE_CDP_PORT: String(port),
      WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'),
      WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
    },
  });
  child.stdout.on('data', chunk => { appLog += chunk; });
  child.stderr.on('data', chunk => { appLog += chunk; });
  child.on('error', error => { spawnError = error; appLog += `\n${error.stack ?? error}`; });
  return child;
}

app = launch();
const sleep = milliseconds => new Promise(resolvePromise => setTimeout(resolvePromise, milliseconds));
const invoke = (command, args) => page.evaluate(([name, value]) => window.__TAURI_INTERNALS__.invoke(name, value), [command, args]);
async function until(predicate, label, timeout = 90_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await sleep(100);
  }
  throw new Error(`Timed out waiting for ${label}`);
}
function count(sql, ...params) {
  return Number(database.prepare(sql).get(...params).n);
}
function rows(sql, ...params) {
  return database.prepare(sql).all(...params).map(row => ({ ...row }));
}
function ordinary() {
  return rows("SELECT id,body_hash,working_version FROM documents WHERE role='ordinary' AND trashed=0 ORDER BY id");
}
function workshopState() {
  const row = rows('SELECT version,state_json FROM workshop_state WHERE singleton=1')[0];
  assert(row, 'The grouped adoption must persist a Workshop state row for its relationship.');
  const state = JSON.parse(row.state_json);
  return { version: String(row.version), state };
}
function check(description, passed, details = undefined) {
  report.checks.push({ description, passed, ...(details === undefined ? {} : { details }) });
  assert(passed, description);
}
async function retainJson(name, value) {
  await writeFile(resolve(output, name), JSON.stringify(value, null, 2));
}
async function retainText(name, value) {
  await writeFile(resolve(output, name), String(value));
}

async function connectApp() {
  await until(async () => {
    if (spawnError || app.exitCode !== null) throw new Error(`Native launch failed: ${spawnError ?? appLog}`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(1500) });
      return response.ok && !!(await response.json()).webSocketDebuggerUrl;
    } catch {
      return false;
    }
  }, 'WebView2 CDP startup');
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  page = browser.contexts()[0].pages()[0];
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  markOwnedReady(app);
}

async function chooseCodexLuna() {
  // Connection qualification is explicit setup, not a generation retry or a
  // fallback. Use the same Settings action an author uses so ProviderContext
  // receives the refreshed catalog/state before the picker opens. Calling the
  // Tauri command directly updates Rust but can leave the mounted React picker
  // with the pre-check catalog. Source: apps/desktop/src/providers/ModelSettings.tsx
  // and apps/desktop/src/providers/ProviderContext.tsx.
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  const settings = page.getByRole('dialog', { name: 'Settings', exact: true });
  await settings.waitFor();
  await settings.getByRole('button', { name: 'Check Codex connection', exact: true }).click();
  await until(async () => {
    const status = settings.getByRole('region', { name: 'Codex connection', exact: true }).locator('.provider-connection-status');
    const current = await invoke('provider_state');
    return current.codexConnection?.ready === true && current.codexConnection?.checking !== true
      && await settings.getByRole('button', { name: 'Check Codex connection', exact: true }).isEnabled()
      && await status.count() === 1 && (await status.textContent())?.trim() === 'Connected';
  }, 'the Settings Codex connection check to complete');
  const checked = await invoke('provider_state');
  check('The Settings Codex connection check succeeds before live generation.',
    checked.codexConnection?.ready === true, checked.codexConnection);
  assert.equal(await settings.locator('#codex-transport').inputValue(), 'exec');
  await settings.getByRole('button', { name: 'Close settings', exact: true }).click();
  await until(async () => await page.getByRole('dialog', { name: 'Settings', exact: true }).count() === 0, 'Settings to close after Codex check');

  // Only now open the picker. The picker selection below is a real UI action;
  // this preserves the visible model, reasoning, and service-tier contract.
  await page.getByRole('button', { name: /^Choose model:/ }).click();
  const descriptor = checked.catalog.models.find(model => model.key.providerId === requestedSelection.providerId && model.key.modelId === requestedSelection.modelId);
  assert(descriptor, 'The checked Codex catalog must contain the explicitly requested model.');
  const luna = page.locator('.model-choice').filter({ hasText: descriptor.label }).filter({ hasText: descriptor.providerLabel });
  await luna.waitFor();
  assert.equal(await luna.count(), 1, 'The picker must expose one Codex GPT-5.6-Luna choice.');
  assert.equal(await luna.isDisabled(), false, 'Codex GPT-5.6-Luna must be live-ready for this opt-in trial.');
  await luna.click();
  await until(async () => {
    const state = await invoke('provider_state');
    const active = state.settings.active;
    return active.providerId === requestedSelection.providerId
      && active.modelId === requestedSelection.modelId
      && active.reasoning === requestedSelection.reasoning
      && active.serviceTier === requestedSelection.serviceTier;
  }, 'requested Luna xhigh/priority selection');
  const provider = await invoke('provider_state');
  check('The native picker selected Codex GPT-5.6-Luna with xhigh reasoning and priority service tier.',
    provider.settings.active.providerId === requestedSelection.providerId
      && provider.settings.active.modelId === requestedSelection.modelId
      && provider.settings.active.reasoning === requestedSelection.reasoning
      && provider.settings.active.serviceTier === requestedSelection.serviceTier,
    provider.settings.active);
  check('The native dispatch is Codex CLI and the connection is ready.',
    provider.dispatch.kind === 'codexCli' && provider.codexConnection?.ready === true,
    { dispatch: provider.dispatch, connection: provider.codexConnection });
  const reasoning = page.locator('#model-traits-reasoning');
  const serviceTier = page.locator('#model-traits-service-tier');
  if (await reasoning.count()) check('The visible reasoning control reports xhigh.', await reasoning.inputValue() === 'xhigh');
  if (await serviceTier.count()) check('The visible service-tier control reports priority.', await serviceTier.inputValue() === 'priority');
  return provider;
}

async function createProject(title) {
  await page.getByRole('button', { name: 'New project', exact: true }).click();
  await page.getByRole('textbox', { name: 'Project title', exact: true }).fill(title);
  await page.getByRole('button', { name: 'Create project', exact: true }).click();
  await page.getByRole('button', { name: 'Start a conversation · trial', exact: true }).click();
  await page.getByRole('textbox', { name: 'Message the project assistant', exact: true }).waitFor();
}

function providerRows(runId) {
  return rows('SELECT * FROM provider_results WHERE run_id=?', runId);
}

async function verifyRun(runId, ordinal, namedDetail) {
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs WHERE id=? AND status=\'completed\' AND dispatch_state=\'delivered\'', runId) === 1,
    `Codex request ${ordinal} terminal completion`);
  const run = rows('SELECT * FROM discussion_runs WHERE id=?', runId)[0];
  assert(run, `Run ${runId} must remain durable.`);

  // context_packets.packet_json/packet_hash/input_hash are the immutable
  // packet columns declared by crates/core/src/storage/004_context_packets.sql.
  // provider_results.binding_json is the requested binding captured at
  // settlement; the same INSERT in crates/core/src/projects/discussions.rs
  // stores provider-reported identity, usage, and transport evidence in their
  // own columns. Do not infer effective delivery traits from binding_json.
  const packet = rows('SELECT * FROM context_packets WHERE id=?', run.packet_id)[0];
  assert(packet, `Run ${runId} must retain its immutable context packet.`);
  check(`Codex request ${ordinal} exposes the packet columns used by the persistence contract.`,
    ['packet_json', 'packet_hash', 'input_hash'].every(column => Object.hasOwn(packet, column)),
    { columns: ['packet_json', 'packet_hash', 'input_hash'] });
  const providerResult = providerRows(runId)[0];
  assert(providerResult, `Run ${runId} must retain its provider result.`);
  const binding = JSON.parse(providerResult.binding_json);
  const providerEvidence = {
    reportedModel: providerResult.reported_model ?? null,
    effectiveIdentity: providerResult.effective_identity ?? null,
    usage: providerResult.usage_json ? JSON.parse(providerResult.usage_json) : null,
    execDelivery: providerResult.delivery_json ? JSON.parse(providerResult.delivery_json) : null,
    appServerDelivery: providerResult.app_server_delivery_json ? JSON.parse(providerResult.app_server_delivery_json) : null,
  };
  check(`Codex request ${ordinal} retains the requested model traits in its immutable binding.`,
    binding.providerId === requestedSelection.providerId
      && binding.modelId === requestedSelection.modelId
      && binding.reasoning === requestedSelection.reasoning
      && binding.serviceTier === requestedSelection.serviceTier,
    { requested: requestedSelection, binding });
  check(`Codex request ${ordinal} records provider identity, usage, and delivery evidence separately.`,
    Object.hasOwn(providerResult, 'reported_model')
      && Object.hasOwn(providerResult, 'effective_identity')
      && Object.hasOwn(providerResult, 'usage_json')
      && Object.hasOwn(providerResult, 'delivery_json')
      && Object.hasOwn(providerResult, 'app_server_delivery_json'),
    { requestedBinding: binding, provider: providerEvidence });
  const parsed = JSON.parse(run.output_text);
  check(`Codex request ${ordinal} uses the project-assistant output parser contract.`,
    parsed.schemaVersion === 'project-assistant-output.v1' && Array.isArray(parsed.drafts) && parsed.drafts.length === 1,
    { schemaVersion: parsed.schemaVersion, draftCount: parsed.drafts?.length });
  check(`Codex request ${ordinal} preserves the named detail ${namedDetail}.`, run.output_text.includes(namedDetail));
  // Terminal settlement and draft materialization are separate durable steps;
  // wait for the materializeChatResult operation before asserting its rows.
  // Source: crates/core/src/projects/project_chat/materialize.rs.
  await until(() => count('SELECT COUNT(*) AS n FROM assistant_drafts WHERE origin_run_id=?', runId) === 1
    && count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='materializeChatResult' AND reference_id=?", runId) === 1,
    `Codex request ${ordinal} draft materialization`);
  check(`Codex request ${ordinal} materializes exactly one reviewable draft.`,
    count('SELECT COUNT(*) AS n FROM assistant_drafts WHERE origin_run_id=?', runId) === 1);
  check(`Codex request ${ordinal} records one project-chat materialization event.`,
    count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='materializeChatResult' AND reference_id=?", runId) === 1);

  await retainJson(`request-${ordinal}-packet.json`, packet);
  await retainJson(`request-${ordinal}-provider-result.json`, providerResult);
  await retainJson(`request-${ordinal}-identity.json`, {
    runId: run.id,
    operationId: run.operation_id,
    packetId: packet.id,
    packetHash: packet.packet_hash,
    inputHash: packet.input_hash,
    targetDocumentId: run.target_document_id,
    targetVersion: run.target_version,
    targetBodyHash: run.target_body_hash,
    status: run.status,
    dispatchState: run.dispatch_state,
    requested: requestedSelection,
    requestedBinding: binding,
    providerEvidence,
  });
  await retainText(`request-${ordinal}-output.txt`, run.output_text);
  const reloadedPacket = rows('SELECT packet_json,packet_hash,input_hash FROM context_packets WHERE id=?', packet.id)[0];
  check(`Codex request ${ordinal} packet is unchanged after terminal materialization.`,
    reloadedPacket.packet_json === packet.packet_json
      && reloadedPacket.packet_hash === packet.packet_hash
      && reloadedPacket.input_hash === packet.input_hash);
  return { run, packet, providerResult, parsed };
}

async function captureContractRun(runId, ordinal) {
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs WHERE id=? AND status=\'completed\' AND dispatch_state=\'delivered\'', runId) === 1,
    `Codex contract request ${ordinal} terminal completion`);
  const run = rows('SELECT * FROM discussion_runs WHERE id=?', runId)[0];
  assert(run, `Contract run ${runId} must remain durable.`);
  const packet = rows('SELECT * FROM context_packets WHERE id=?', run.packet_id)[0];
  const providerResult = providerRows(runId)[0];
  assert(packet && providerResult, `Contract request ${ordinal} must retain packet and provider evidence.`);
  const binding = JSON.parse(providerResult.binding_json);
  const providerEvidence = {
    reportedModel: providerResult.reported_model ?? null,
    effectiveIdentity: providerResult.effective_identity ?? null,
    usage: providerResult.usage_json ? JSON.parse(providerResult.usage_json) : null,
    execDelivery: providerResult.delivery_json ? JSON.parse(providerResult.delivery_json) : null,
    appServerDelivery: providerResult.app_server_delivery_json ? JSON.parse(providerResult.app_server_delivery_json) : null,
  };
  // Retain the complete evidence before semantic checks. A malformed live
  // response must still leave enough material to diagnose the qualification.
  await retainJson(`contract-request-${ordinal}-packet.json`, packet);
  await retainJson(`contract-request-${ordinal}-provider-result.json`, providerResult);
  await retainJson(`contract-request-${ordinal}-identity.json`, {
    runId: run.id,
    operationId: run.operation_id,
    packetId: packet.id,
    packetHash: packet.packet_hash,
    inputHash: packet.input_hash,
    targetDocumentId: run.target_document_id,
    targetVersion: run.target_version,
    targetBodyHash: run.target_body_hash,
    status: run.status,
    dispatchState: run.dispatch_state,
    requested: requestedSelection,
    requestedBinding: binding,
    providerEvidence,
  });
  await retainText(`contract-request-${ordinal}-output.txt`, run.output_text);
  return { run, packet, providerResult, binding, providerEvidence, parsed: JSON.parse(run.output_text) };
}

async function verifyContractProjectRun(runId, ordinal, expectedContract) {
  const captured = await captureContractRun(runId, ordinal);
  const { run, packet, binding, providerEvidence, parsed } = captured;
  check(`Contract request ${ordinal} retains the requested Luna traits separately from provider evidence.`,
    binding.providerId === requestedSelection.providerId
      && binding.modelId === requestedSelection.modelId
      && binding.reasoning === requestedSelection.reasoning
      && binding.serviceTier === requestedSelection.serviceTier,
    { requested: requestedSelection, binding, providerEvidence });
  check(`Contract request ${ordinal} is completed and delivery is explicitly settled.`,
    run.status === 'completed' && run.dispatch_state === 'delivered',
    { status: run.status, dispatchState: run.dispatch_state });
  check(`Contract request ${ordinal} retains the expected response contract.`,
    packet.packet_json.includes(expectedContract),
    { packetId: packet.id, packetHash: packet.packet_hash, inputHash: packet.input_hash, expectedContract });
  if (expectedContract === 'project-assistant-output.v1') {
    check(`Contract request ${ordinal} retains the new project-chat prompt recipe.`, packet.packet_json.includes('project-chat-prompt.v3'));
    check(`Contract request ${ordinal} is parsed as a project-assistant response with a chapter handoff.`,
      parsed.schemaVersion === expectedContract
        && parsed.chapterHandoff
        && (parsed.chapterHandoff.targetHandle === null || typeof parsed.chapterHandoff.targetHandle === 'string')
        && typeof parsed.chapterHandoff.proposedTitle === 'string'
        && typeof parsed.chapterHandoff.instruction === 'string'
        && typeof parsed.chapterHandoff.brief === 'string',
      { schemaVersion: parsed.schemaVersion, chapterHandoff: parsed.chapterHandoff });
  } else {
    check(`Contract request ${ordinal} is parsed as chapter discussion feedback with a range proposal.`,
      parsed.schemaVersion === expectedContract
        && typeof parsed.answer === 'string'
        && parsed.rangeProposal
        && typeof parsed.rangeProposal.sourceHead === 'object'
        && typeof parsed.rangeProposal.firstBlockId === 'string'
        && typeof parsed.rangeProposal.lastBlockId === 'string'
        && typeof parsed.rangeProposal.quote === 'string',
      { schemaVersion: parsed.schemaVersion, rangeProposal: parsed.rangeProposal });
  }
  const reloadedPacket = rows('SELECT packet_json,packet_hash,input_hash FROM context_packets WHERE id=?', packet.id)[0];
  check(`Contract request ${ordinal} packet remains byte-identical after settlement.`,
    reloadedPacket.packet_json === packet.packet_json
      && reloadedPacket.packet_hash === packet.packet_hash
      && reloadedPacket.input_hash === packet.input_hash);
  return captured;
}

function draftText(draft) {
  return (draft.blocks ?? []).flatMap(block => block.content ?? [])
    .filter(inline => inline.type === 'text')
    .map(inline => inline.text)
    .join(' ');
}

async function verifyGroupedRun(runId, ordinal, sharedDetail) {
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs WHERE id=? AND status=\'completed\' AND dispatch_state=\'delivered\'', runId) === 1,
    `Codex grouped request ${ordinal} terminal completion`);
  const run = rows('SELECT * FROM discussion_runs WHERE id=?', runId)[0];
  assert(run, `Grouped run ${runId} must remain durable.`);
  const packet = rows('SELECT * FROM context_packets WHERE id=?', run.packet_id)[0];
  const providerResult = providerRows(runId)[0];
  assert(packet && providerResult, `Grouped request ${ordinal} must retain packet and provider evidence.`);
  check('The grouped live request retains the grouped project-chat prompt recipe.', packet.packet_json.includes('project-chat-prompt.v3'));
  const binding = JSON.parse(providerResult.binding_json);
  const providerEvidence = {
    reportedModel: providerResult.reported_model ?? null,
    effectiveIdentity: providerResult.effective_identity ?? null,
    usage: providerResult.usage_json ? JSON.parse(providerResult.usage_json) : null,
    execDelivery: providerResult.delivery_json ? JSON.parse(providerResult.delivery_json) : null,
    appServerDelivery: providerResult.app_server_delivery_json ? JSON.parse(providerResult.app_server_delivery_json) : null,
  };
  check('The grouped live request retains requested model traits separately from provider-reported evidence.',
    binding.providerId === requestedSelection.providerId
      && binding.modelId === requestedSelection.modelId
      && binding.reasoning === requestedSelection.reasoning
      && binding.serviceTier === requestedSelection.serviceTier,
    { requested: requestedSelection, binding, providerEvidence });
  check('The grouped live request records provider identity, usage, and delivery evidence without inferring effective tier.',
    Object.hasOwn(providerResult, 'reported_model')
      && Object.hasOwn(providerResult, 'effective_identity')
      && Object.hasOwn(providerResult, 'usage_json')
      && Object.hasOwn(providerResult, 'delivery_json')
      && Object.hasOwn(providerResult, 'app_server_delivery_json'),
    { requestedBinding: binding, provider: providerEvidence });

  const parsed = JSON.parse(run.output_text);
  const drafts = Array.isArray(parsed.drafts) ? parsed.drafts : [];
  const kinds = new Set(drafts.map(draft => draft.kind));
  check('The grouped live response uses the project-assistant contract with exactly one character and one world draft.',
    parsed.schemaVersion === 'project-assistant-output.v1'
      && drafts.length === 2
      && kinds.size === 2
      && kinds.has('character')
      && kinds.has('world'),
    { schemaVersion: parsed.schemaVersion, draftCount: drafts.length, kinds: [...kinds] });
  check('The grouped live drafts are short, new targets, and share the requested connecting detail.',
    drafts.length === 2
      && drafts.every(draft => draft.targetHandle == null && Array.isArray(draft.blocks) && draft.blocks.length > 0 && draft.blocks.length <= 6 && draftText(draft).includes(sharedDetail)),
    { sharedDetail, drafts: drafts.map(draft => ({ key: draft.key, kind: draft.kind, title: draft.title, text: draftText(draft) })) });

  const groupEffects = parsed.groupEffects && typeof parsed.groupEffects === 'object' ? parsed.groupEffects : null;
  const relationships = Array.isArray(groupEffects?.relationships) ? groupEffects.relationships : [];
  check('The grouped live response includes exactly one nonempty relationship proposal for the two response-local drafts.',
    relationships.length === 1
      && Array.isArray(groupEffects?.impacts) && groupEffects.impacts.length === 0
      && Array.isArray(groupEffects?.supersessions) && groupEffects.supersessions.length === 0
      && Array.isArray(groupEffects?.placements) && groupEffects.placements.length === 0,
    { groupEffects });
  const draftKeys = new Set(drafts.map(draft => draft.key));
  const relationship = relationships[0];
  check('The grouped relationship maps fromRef and toRef to distinct response draft keys and retains its explanation fields.',
    !!relationship
      && draftKeys.has(relationship.fromRef)
      && draftKeys.has(relationship.toRef)
      && relationship.fromRef !== relationship.toRef
      && typeof relationship.key === 'string' && relationship.key.trim().length > 0
      && typeof relationship.type === 'string' && relationship.type.trim().length > 0
      && typeof relationship.description === 'string' && relationship.description.trim().length > 0
      && typeof relationship.uncertainty === 'string' && relationship.uncertainty.trim().length > 0,
    { relationship, draftKeys: [...draftKeys] });
  report.groupedRelationship = { present: true, count: relationships.length, relationship };

  await until(() => count('SELECT COUNT(*) AS n FROM assistant_drafts WHERE origin_run_id=?', runId) === 2
    && count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='materializeChatResult' AND reference_id=?", runId) === 1,
  `Codex grouped request ${ordinal} draft materialization`);
  check('The grouped live request materializes exactly two isolated review drafts.', count('SELECT COUNT(*) AS n FROM assistant_drafts WHERE origin_run_id=?', runId) === 2);
  check('The grouped live request records one materialization event.', count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='materializeChatResult' AND reference_id=?", runId) === 1);

  await retainJson(`grouped-request-${ordinal}-packet.json`, packet);
  await retainJson(`grouped-request-${ordinal}-provider-result.json`, providerResult);
  await retainJson(`grouped-request-${ordinal}-identity.json`, {
    runId: run.id,
    operationId: run.operation_id,
    packetId: packet.id,
    packetHash: packet.packet_hash,
    inputHash: packet.input_hash,
    status: run.status,
    dispatchState: run.dispatch_state,
    requested: requestedSelection,
    requestedBinding: binding,
    providerEvidence,
  });
  await retainText(`grouped-request-${ordinal}-output.txt`, run.output_text);
  return { run, packet, providerResult, parsed, drafts };
}

async function runGroupedTrial() {
  await connectApp();
  const provider = await chooseCodexLuna();
  await createProject('Live Codex grouped chat trial');
  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === 'Live Codex grouped chat trial');
  assert(entry, 'The grouped live project must be present in the native library.');
  const projectId = entry.projectId;
  const projectPath = await realpath(entry.path);
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`), 'The grouped project must stay inside the isolated data directory.');
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  check('The grouped trial uses an isolated synthetic project database.', count('SELECT COUNT(*) AS n FROM project') === 1);
  const composer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });
  const sharedDetail = 'Lantern Archive';
  const instruction = `Create exactly two short connected drafts for my review about Mara Vey, a courier who carries memories through the ${sharedDetail}. Return one character draft for Mara Vey and one world draft for the ${sharedDetail}; use the exact shared detail in both drafts so their connection is clear. Keep both drafts new and concise. Return exactly one nonempty groupEffects.relationships proposal connecting the response-local character and world draft keys, with a type, description, and uncertainty; return empty impacts, supersessions, and placements arrays. Do not create a chapter, adopt anything, or make another model call.`;
  await composer.fill(instruction);
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === 1, 'one durable grouped live request');
  const run = rows('SELECT * FROM discussion_runs ORDER BY created_at, id')[0];
  const captured = await verifyGroupedRun(run.id, 1, sharedDetail);
  check('The grouped trial has exactly one explicit provider run with no fallback or hidden retry.', count('SELECT COUNT(*) AS n FROM discussion_runs') === 1, { runId: run.id });

  const draftTabs = page.getByRole('tablist', { name: 'Available drafts', exact: true }).getByRole('tab');
  await page.getByRole('button', { name: /^Review drafts/ }).click();
  await draftTabs.first().waitFor({ timeout: 30_000 });
  let selected = 0;
  for (const draftTab of await draftTabs.all()) {
    await draftTab.click();
    const draftId = await draftTab.getAttribute('data-draft-id');
    const checkbox = page.locator(`.coauthor-review-selection input[data-draft-id="${draftId}"]`);
    if (!(await checkbox.count())) continue;
    if (!(await checkbox.isChecked())) await checkbox.check();
    selected += 1;
  }
  assert.equal(selected, 2, 'The grouped live response must expose exactly two selectable drafts.');
  await page.getByRole('button', { name: /^Prepare grouped review/ }).click();
  const previewRegion = page.getByRole('region', { name: 'Exact adoption preview', exact: true });
  await previewRegion.waitFor({ timeout: 30_000 });
  const relationshipSection = previewRegion.getByRole('region', { name: 'Proposed relationships', exact: true });
  await relationshipSection.waitFor({ timeout: 30_000 });
  const relationshipDescription = captured.parsed.groupEffects.relationships[0].description;
  check('The exact grouped preview exposes the proposed relationship before the author adopts either document.',
    (await relationshipSection.textContent())?.includes(relationshipDescription) === true,
    { relationshipDescription });
  const adoptionButton = page.locator('.coauthor-review-footer').getByRole('button', { name: /^Adopt all 2 documents:/ });
  await adoptionButton.waitFor({ timeout: 30_000 });
  const ordinaryBefore = ordinary();
  const epochBefore = Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n);
  const receiptsBefore = count("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'");
  const runsBefore = count('SELECT COUNT(*) AS n FROM discussion_runs');
  await adoptionButton.click();
  await previewRegion.waitFor({ state: 'detached', timeout: 60_000 });
  await until(() => ordinary().length === ordinaryBefore.length + 2, 'two grouped ordinary documents committed');
  const ordinaryAfter = ordinary();
  check('Grouped adoption commits both documents atomically and advances one source epoch.',
    ordinaryAfter.length === ordinaryBefore.length + 2
      && Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n) === epochBefore + 1
      && count("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'") === receiptsBefore + 1,
    { ordinaryBefore, ordinaryAfter, epochBefore, epochAfter: Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n) });
  check('Grouped adoption does not create another provider run.', count('SELECT COUNT(*) AS n FROM discussion_runs') === runsBefore);
  const committedWorkshop = workshopState();
  const committedRelationships = Array.isArray(committedWorkshop.state.relationships) ? committedWorkshop.state.relationships : [];
  const expectedRelationship = captured.parsed.groupEffects.relationships[0];
  const adoptedIds = new Set(ordinaryAfter.map(document => document.id));
  const committedRelationship = committedRelationships.find(relationship =>
    relationship && adoptedIds.has(relationship.fromDocumentId) && adoptedIds.has(relationship.toDocumentId));
  check('Grouped adoption persists exactly one relationship with ordinary adopted endpoints and no assistant/control endpoint IDs.',
    committedRelationships.length === 1
      && !!committedRelationship
      && committedRelationship.fromDocumentId !== committedRelationship.toDocumentId
      && committedRelationship.sourceHeads?.length === 2
      && committedRelationship.sourceHeads[0].documentId === committedRelationship.fromDocumentId
      && committedRelationship.sourceHeads[1].documentId === committedRelationship.toDocumentId
      && committedRelationship.sourceHeads.every(head => adoptedIds.has(head.documentId))
      && committedRelationship.status === 'chosen',
    { committedRelationships, adoptedIds: [...adoptedIds] });
  const adoptedById = new Map(ordinaryAfter.map(document => [document.id, document]));
  check('The committed relationship source heads match the final adopted ordinary document heads and the exact reviewed relationship fields.',
    !!committedRelationship
      && committedRelationship.type === expectedRelationship.type
      && committedRelationship.description === expectedRelationship.description
      && committedRelationship.uncertainty === expectedRelationship.uncertainty
      && committedRelationship.sourceHeads.every(head => {
        const document = adoptedById.get(head.documentId);
        return document && head.version === String(document.working_version) && head.bodyHash === document.body_hash;
      }),
    { committedRelationship, expectedRelationship });
  const adoptedDocuments = rows("SELECT id,title,kind,working_version,body_hash,body_json FROM documents WHERE role='ordinary' AND trashed=0 ORDER BY id");
  await retainJson('grouped-adoption.json', {
    runId: captured.run.id,
    ordinaryBefore,
    ordinaryAfter: adoptedDocuments,
    sourceEpochBefore: epochBefore,
    sourceEpochAfter: Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n),
    adoptionReceipts: rows("SELECT operation_id,payload_hash FROM command_receipts WHERE operation_kind='adoptChatPreview' ORDER BY rowid"),
    workshopRelationship: committedRelationship,
    workshopVersion: committedWorkshop.version,
  });

  database.close(); database = undefined;
  await browser.close(); browser = undefined;
  await stopOwned(app);
  appLog = ''; spawnError = undefined;
  const previousPort = port;
  do { port = await reservePort(); } while (port === previousPort);
  app = launch();
  await connectApp();
  const reopenedButton = page.getByRole('button', { name: /^Live Codex grouped chat trial Last opened/ });
  await reopenedButton.waitFor({ timeout: 30_000 });
  await reopenedButton.click();
  await page.getByRole('textbox', { name: 'Message the project assistant', exact: true }).waitFor({ timeout: 30_000 });
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  const retained = rows("SELECT id,title,kind,working_version,body_hash,body_json FROM documents WHERE role='ordinary' AND trashed=0 ORDER BY id");
  const retainedWorkshop = workshopState();
  const retainedRelationships = Array.isArray(retainedWorkshop.state.relationships) ? retainedWorkshop.state.relationships : [];
  check('After native restart, both grouped documents and the adoption receipt remain durable.',
    retained.length === adoptedDocuments.length
      && JSON.stringify(retained) === JSON.stringify(adoptedDocuments)
      && count('SELECT COUNT(*) AS n FROM discussion_runs') === 1
      && Number(database.prepare('SELECT context_source_epoch AS n FROM project').get().n) === epochBefore + 1
      && count("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind='adoptChatPreview'") === receiptsBefore + 1,
    { retainedDocuments: retained, expectedDocuments: adoptedDocuments, runCount: count('SELECT COUNT(*) AS n FROM discussion_runs') });
  check('After native restart, the grouped relationship and its committed source heads remain unchanged.',
    retainedWorkshop.version === committedWorkshop.version
      && retainedRelationships.length === 1
      && JSON.stringify(retainedRelationships[0]) === JSON.stringify(committedRelationship),
    { retainedWorkshop, expectedRelationship: committedRelationship });
  await page.getByRole('tab', { name: 'All documents', exact: true }).click();
  for (const document of retained) await page.getByText(document.title, { exact: true }).first().waitFor({ timeout: 30_000 });
  check('The reopened native workspace exposes both adopted document titles.', retained.every(document => document.title && document.title.length > 0), { titles: retained.map(document => document.title) });
  report.runtime = await invoke('runtime_info');
  report.provider = provider;
  report.project = { id: projectId, path: projectPath };
  report.requestCount = count('SELECT COUNT(*) AS n FROM discussion_runs');
  report.grouped = {
    sharedDetail,
    draftKinds: captured.drafts.map(draft => draft.kind),
    adoptedDocumentCount: retained.length,
    relationshipPersisted: retainedRelationships.length === 1,
    restarted: true,
  };
  report.status = 'passed';
  await saveReport();
  console.log(JSON.stringify({ status: report.status, mode: report.mode, output, requestCount: report.requestCount }, null, 2));
}

async function runContractTrial() {
  await connectApp();
  await chooseCodexLuna();
  await createProject('Live Codex chat contract trial');
  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === 'Live Codex chat contract trial');
  assert(entry, 'The synthetic live project must be present in the native library.');
  const projectId = entry.projectId;
  assert(typeof projectId === 'string' && projectId.length > 0, 'The synthetic live project must expose its verified project ID.');
  const projectPath = await realpath(entry.path);
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`), 'The live project must stay inside the isolated data directory.');
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  check('The contract trial uses an isolated synthetic project database.', count('SELECT COUNT(*) AS n FROM project') === 1);
  const composer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });

  // Request 1 is an author-room handoff proposal only. The instruction
  // explicitly forbids a second call, chapter creation, and adoption so the
  // qualification can prove the UI leaves those author decisions explicit.
  const handoffInstruction = 'Propose a chapter handoff for a short three-paragraph chapter about a healer named Mei selling memories above the clouds. Return the handoff proposal for my review only. Do not write chapter prose, create a chapter, adopt anything, or make a second model call.';
  await composer.fill(handoffInstruction);
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === 1, 'contract handoff request');
  const firstRun = rows('SELECT * FROM discussion_runs ORDER BY created_at, id')[0];
  const first = await verifyContractProjectRun(firstRun.id, 1, 'project-assistant-output.v1');
  const handoff = page.getByRole('region', { name: 'Proposed chapter writing task', exact: true });
  await handoff.waitFor();
  check('The handoff remains an author-reviewed proposal with no automatic chapter call or adoption.',
    count('SELECT COUNT(*) AS n FROM discussion_runs') === 1
      && ordinary().length === 0
      && count("SELECT COUNT(*) AS n FROM command_receipts WHERE operation_kind IN ('adoptChatPreview','createDocument')") === 0,
    { runId: first.run.id });

  // Create the chapter locally through the normal native document flow, then
  // write exactly three paragraphs. This is an author action between calls.
  await page.getByRole('tab', { name: 'All documents', exact: true }).click();
  await page.getByRole('button', { name: 'Create blank chapter', exact: true }).click();
  const chapterEditor = page.getByRole('textbox', { name: 'Manuscript', exact: true });
  await chapterEditor.waitFor();
  const firstParagraph = 'Mei opened the memory market before sunrise.';
  await chapterEditor.fill(firstParagraph);
  await chapterEditor.press('End');
  await chapterEditor.press('Enter');
  await chapterEditor.pressSequentially('Cloudwater carried other people’s forgotten summers.');
  await chapterEditor.press('Enter');
  await chapterEditor.pressSequentially('Above the clouds, the city waited for a buyer.');
  await chapterEditor.press('Control+s');
  await until(async () => (await page.locator('.writing .save-status').textContent())?.trim() === 'Saved'
    && Number(rows("SELECT working_version FROM documents WHERE kind='chapter' AND trashed=0 ORDER BY id")[0]?.working_version ?? 0) > 0
    && await chapterEditor.getAttribute('contenteditable') === 'true', 'three-paragraph chapter checkpoint');
  const chapter = rows("SELECT id,title,kind,working_version,body_hash,body_json FROM documents WHERE kind='chapter' AND trashed=0 ORDER BY id")[0];
  assert(chapter, 'The contract trial must create one ordinary chapter.');
  const chapterBody = JSON.parse(chapter.body_json);
  const chapterHead = {
    head: { documentId: chapter.id, version: String(chapter.working_version), bodyHash: chapter.body_hash },
    body: chapterBody,
  };
  check('The synthetic chapter has three locally written paragraphs before feedback.', chapterBody.body.content.length === 3, { chapterId: chapter.id, head: chapterHead.head });
  const headsBeforeFeedback = ordinary().map(document => ({ id: document.id, bodyHash: document.body_hash, version: document.working_version }));

  // Request 2 is an unselected chapter Discuss. The native getter and review
  // card must expose a range proposal, and confirmation stages only a fresh
  // scoped task; it must not dispatch a third request or mutate prose.
  await page.getByRole('button', { name: 'Discuss in project chat', exact: true }).click();
  await composer.fill('Review this chapter without changing it. Give concise feedback and propose an edit range covering exactly the first paragraph, quoting it exactly. Do not propose the other paragraphs and do not make another model call.');
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === 2, 'contract chapter feedback request');
  const secondRun = rows('SELECT * FROM discussion_runs ORDER BY created_at, id')[1];
  const second = await verifyContractProjectRun(secondRun.id, 2, 'chapter-discussion-output.v1');
  assert.equal(secondRun.target_document_id, chapterHead.head.documentId);
  check('The second request is an unselected chapter Discuss against the exact saved chapter head.',
    String(secondRun.target_version) === chapterHead.head.version && secondRun.target_body_hash === chapterHead.head.bodyHash
      && count("SELECT COUNT(*) AS n FROM conversation_items WHERE kind='chapterRequest' AND reference_id=?", secondRun.id) === 1,
    { runId: secondRun.id, target: { documentId: secondRun.target_document_id, version: secondRun.target_version, bodyHash: secondRun.target_body_hash } });
  await page.getByRole('button', { name: 'Open chapter result', exact: true }).last().click();
  const rangeButton = page.getByRole('button', { name: 'Use this passage for an edit', exact: true });
  await rangeButton.waitFor();
  const visibleQuote = (await page.locator('[aria-label="Complete proposed passage"]').textContent()).trim();
  check('The getter-backed chapter review card shows exactly the first paragraph.', visibleQuote === firstParagraph, { visibleQuote, expected: firstParagraph });
  await rangeButton.click();
  await until(() => {
    const composerJson = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json);
    return composerJson.chapter?.intent === 'proposeEdits' && composerJson.chapter?.scope?.kind === 'blocks';
  }, 'author confirmation stages the proposed range');
  const staged = JSON.parse(database.prepare('SELECT composer_json FROM project_conversations').get().composer_json);
  check('Range confirmation stages an exact scoped edit without changing ordinary heads or dispatching a third request.',
    staged.chapter.target.documentId === chapterHead.head.documentId
      && staged.chapter.target.version === chapterHead.head.version
      && staged.chapter.target.bodyHash === chapterHead.head.bodyHash
      && staged.chapter.scope.quote === firstParagraph
      && staged.chapter.scope.start.blockId === staged.chapter.scope.end.blockId
      && count('SELECT COUNT(*) AS n FROM discussion_runs') === 2
      && JSON.stringify(ordinary().map(document => ({ id: document.id, bodyHash: document.body_hash, version: document.working_version }))) === JSON.stringify(headsBeforeFeedback),
    { stagedChapter: staged.chapter, runCount: count('SELECT COUNT(*) AS n FROM discussion_runs') });
  check('The two contract requests remain distinct durable operations.', first.run.id !== second.run.id && first.packet.id !== second.packet.id);
  check('No renderer page errors occurred during the contract trial.', pageErrors.length === 0, pageErrors);
  report.status = 'passed';
  report.runtime = await invoke('runtime_info');
  report.project = { id: projectId, path: projectPath };
  report.requestCount = count('SELECT COUNT(*) AS n FROM discussion_runs');
  await saveReport();
  console.log(JSON.stringify({ status: report.status, mode: report.mode, output, requestCount: report.requestCount }, null, 2));
}

try {
  if (groupedMode) {
    await runGroupedTrial();
  } else if (contractMode) {
    await runContractTrial();
  } else {
  await connectApp();
  await chooseCodexLuna();
  await createProject('Live Codex chat contract trial');
  const library = await invoke('library_snapshot');
  const entry = library.entries.find(item => item.title === 'Live Codex chat contract trial');
  assert(entry, 'The synthetic live project must be present in the native library.');
  const projectId = entry.projectId;
  assert(typeof projectId === 'string' && projectId.length > 0, 'The synthetic live project must expose its verified project ID.');
  const projectPath = await realpath(entry.path);
  const child = relative(toNamespacedPath(data), toNamespacedPath(projectPath));
  assert(child && !isAbsolute(child) && child !== '..' && !child.startsWith(`..${sep}`), 'The live project must stay inside the isolated data directory.');
  database = new DatabaseSync(resolve(projectPath, 'project.sqlite3'), { readOnly: true });
  check('The native app created an isolated project database.', count('SELECT COUNT(*) AS n FROM project') === 1);
  const composer = page.getByRole('textbox', { name: 'Message the project assistant', exact: true });

  // Request 1: one short world draft from a rough idea.
  await composer.fill('A healer named Mei sells memories in a city above the clouds. Create one short world draft for review and keep the named detail Mei.');
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === 1, 'first durable live request');
  const firstRun = rows('SELECT * FROM discussion_runs ORDER BY created_at, id')[0];
  const first = await verifyRun(firstRun.id, 1, 'Mei');
  check('The first live request has no hidden retry or fallback run.', count('SELECT COUNT(*) AS n FROM discussion_runs') === 1);

  // Request 2: a fresh project turn must recover the protagonist's name from
  // retained conversation context; the new instruction does not repeat it.
  await composer.fill('Continue the same conversation. Make the memory market depend on a river of cloudwater. Include the established protagonist by name. Return one short world draft for review.');
  await until(() => page.getByRole('button', { name: /^Send/ }).isEnabled(), 'composer Send enabled for the second request');
  await composer.press('Control+Enter');
  await until(() => count('SELECT COUNT(*) AS n FROM discussion_runs') === 2, 'second durable live request');
  const secondRun = rows('SELECT * FROM discussion_runs ORDER BY created_at, id')[1];
  const second = await verifyRun(secondRun.id, 2, 'Mei');
  check('The second live request has no hidden retry or fallback run.', count('SELECT COUNT(*) AS n FROM discussion_runs') === 2);
  check('The second packet retains the conversational named detail.', second.packet.packet_json.includes('Mei'));
  check('The two live requests are distinct durable operations.', first.run.id !== second.run.id && first.packet.id !== second.packet.id);
  check('No renderer page errors occurred during the two-request trial.', pageErrors.length === 0, pageErrors);

  report.status = 'passed';
  report.runtime = await invoke('runtime_info');
  report.project = { id: projectId, path: projectPath };
  report.requestCount = count('SELECT COUNT(*) AS n FROM discussion_runs');
  await saveReport();
  console.log(JSON.stringify({ status: report.status, output, requestCount: report.requestCount }, null, 2));
  }
} catch (error) {
  report.status = 'failed';
  report.error = String(error?.stack ?? error);
  report.appLog = appLog;
  report.pageErrors = pageErrors;
  await saveReport();
  await page?.screenshot({ path: resolve(output, 'failure.png') }).catch(() => {});
  await writeFile(resolve(output, 'failure.txt'), `${report.error}\n${appLog}`);
  throw error;
} finally {
  database?.close();
  await browser?.close().catch(() => {});
  await stopOwned(app);
}
