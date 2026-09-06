import assert from 'node:assert/strict';
import { resolve } from 'node:path';

/**
 * Qualify the bounded, local-only story lookup journey through the real
 * WebView2 UI. The local Rust worker is deterministic: it requests a search,
 * reads the matching passage, and then returns one final discussion answer.
 * No provider key or network service is involved.
 */
export async function qualifyContextLookup({ page, output, createWritingProject, checks }) {
  const prose = 'Mei made one promise before dawn. The brass key waited beside the lantern.';
  await createWritingProject('Lookup story', 'chapter', 'The promise', prose);

  const lookupOptIn = page.getByRole('checkbox', { name: 'Look up story details when needed', exact: true });
  await lookupOptIn.check();
  assert.equal(await lookupOptIn.isChecked(), true);

  const composer = page.getByRole('textbox', { name: 'Discuss this document', exact: true });
  await composer.fill('Find "promise" and explain what the exact passage says.');
  await page.getByRole('button', { name: 'Send', exact: true }).click();

  // The worker has three durable invocations. Waiting for the final text is
  // the observable completion signal; there are no fixed sleeps in this flow.
  await page.getByText(/Local test assistant: I read this exact saved passage:/, { exact: false }).waitFor({ timeout: 30_000 });
  const finalText = await page.locator('.feedback-note').last().innerText();
  assert(finalText.includes('Local test assistant: I read this exact saved passage:'));
  assert(finalText.includes(prose));
  assert(finalText.includes('No live AI model was called.'));

  const selector = page.locator('#discussion-context-call');
  await selector.waitFor({ timeout: 30_000 });
  await page.waitForFunction(() => document.querySelectorAll('#discussion-context-call option').length === 3);
  const calls = await selector.locator('option').allTextContents();
  assert.equal(calls.length, 3);
  assert(calls[0].includes('read complete'));
  assert(calls[1].includes('read complete'));
  assert(calls[2].includes('answer received'));

  const visibleDiscussion = await page.locator('.feedback-scroll').innerText();
  assert(!visibleDiscussion.includes('"kind":"needsContext"'), 'Intermediate lookup JSON must not appear in chat');
  assert(!visibleDiscussion.includes('story-lookup.v1'), 'Lookup protocol metadata must remain out of chat');

  const inspector = page.locator('.context-inspector');
  if (await inspector.getAttribute('open') === null) await inspector.locator(':scope>summary').click();
  await inspector.locator('.context-lookup').waitFor();
  await inspector.getByText(/2 of 2 additional lookups completed/).waitFor();
  assert.equal(await inspector.locator('.context-lookup-exchange').count(), 2);
  assert.equal(await inspector.getByText(prose, { exact: true }).count(), 2);

  const packetIds = await selector.locator('option').evaluateAll(options => options.map(option => option.value));
  await selector.selectOption(packetIds[0]);
  await inspector.getByText('No additional story evidence was supplied.', { exact: true }).waitFor();
  assert.equal(await inspector.locator('.context-lookup-exchange').count(), 0);
  await selector.selectOption(packetIds[2]);
  await inspector.getByText(/2 of 2 additional lookups completed/).waitFor();
  assert.equal(await inspector.locator('.context-lookup-exchange').count(), 2);

  const sourceButton = inspector.locator('.context-lookup button', { hasText: 'Open exact source' }).first();
  await sourceButton.click();
  const source = page.locator('[aria-label="Saved story source"]');
  await source.waitFor();
  assert.equal(await source.locator('h3').innerText(), 'The promise');
  assert.equal(await source.getByText(prose, { exact: true }).count(), 1);
  await page.screenshot({ path: resolve(output, 'context-lookup-evidence.png') });

  // Reopen the durable project and discussion. The lookup exchanges and
  // final packet are loaded from Rust persistence, not reconstructed from the
  // current editor body.
  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();
  await page.reload();
  await page.getByRole('button', { name: /^Lookup story Last opened/ }).click();
  await page.getByRole('heading', { name: 'The promise', exact: true }).waitFor();
  await page.getByText(/Local test assistant: I read this exact saved passage:/, { exact: false }).waitFor({ timeout: 30_000 });
  const reopenedDiscussion = await page.locator('.feedback-scroll').innerText();
  assert(!reopenedDiscussion.includes('"kind":"needsContext"'), 'Reopened discussion must not expose intermediate lookup JSON');

  const reopenedSelector = page.locator('#discussion-context-call');
  await reopenedSelector.waitFor({ timeout: 30_000 });
  await page.waitForFunction(() => document.querySelectorAll('#discussion-context-call option').length === 3);
  assert.equal(await reopenedSelector.locator('option').count(), 3);
  const reopenedInspector = page.locator('.context-inspector');
  if (await reopenedInspector.getAttribute('open') === null) await reopenedInspector.locator(':scope>summary').click();
  await reopenedInspector.locator('.context-lookup').waitFor();
  assert.equal(await reopenedInspector.locator('.context-lookup-exchange').count(), 2);
  assert.equal(await reopenedInspector.getByText(prose, { exact: true }).count(), 2);
  await reopenedInspector.locator('.context-lookup button', { hasText: 'Open exact source' }).first().click();
  const reopenedSource = page.locator('[aria-label="Saved story source"]');
  await reopenedSource.waitFor();
  assert.equal(await reopenedSource.locator('h3').innerText(), 'The promise');
  assert.equal(await reopenedSource.getByText(prose, { exact: true }).count(), 1);

  await page.getByRole('button', { name: 'All projects', exact: true }).click();
  await page.getByRole('heading', { name: 'Your stories', exact: true }).waitFor();

  checks.push('Native local lookup opts in to exactly three durable calls (search, read, final), keeps intermediate JSON out of chat, exposes authenticated packet evidence and exact-source navigation, and preserves the final packet across project reopen');
}
