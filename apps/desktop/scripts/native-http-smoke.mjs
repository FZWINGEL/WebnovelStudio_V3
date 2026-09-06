// Native Tauri HTTP transport qualification against a synthetic loopback server.
// Uses temporary projects and removes only its own synthetic Windows credential.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { mkdtemp, mkdir, stat, writeFile, readFile, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, relative, isAbsolute, sep } from 'node:path';
import { createServer } from 'node:net';
import { createServer as createHttpServer } from 'node:http';
import { DatabaseSync } from 'node:sqlite';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const evidence = resolve(root, '.local/native-results/http');
await mkdir(evidence, { recursive: true });
const executable = process.env.WNS_V3_NATIVE_EXE ? resolve(process.env.WNS_V3_NATIVE_EXE) : resolve(root, 'target/debug/webnovel-desktop.exe');
const build = await stat(executable);
const executableSha256 = createHash('sha256').update(await readFile(executable)).digest('hex');
const data = await mkdtemp(resolve(tmpdir(), 'wns-v3-http-native-'));
const server = createServer();
await new Promise(resolvePromise => server.listen(0, '127.0.0.1', resolvePromise));
const port = server.address().port;
await new Promise(resolvePromise => server.close(resolvePromise));
const app = spawn(executable, [], {
  cwd: data,
  windowsHide: true,
  stdio: ['ignore', 'ignore', 'ignore'],
  env: {
    ...process.env,
    WNS_V3_NATIVE_CDP_PORT: String(port),
    WNS_V3_TRIAL_WEBVIEW_DIR: resolve(data, 'webview'),
    WNS_V3_TEST_DATA_DIR: resolve(data, 'library'),
  },
});

let releaseFirst;
let modelReads = 0;
let memoryRequests = 0;
const requests=[];
const api=createHttpServer(async (req,res)=>{
  if(req.method==='GET'&&req.url.endsWith('/models')) { if (++modelReads === 2) return; res.writeHead(200,{'Content-Type':'application/json'}); res.end(JSON.stringify({data:[{id:'test-editor-v1'},{id:'test-editor-v2'},{id:'gpt-5.6-luna'}]})); return; }
  const chunks=[]; for await(const chunk of req) chunks.push(chunk);
  const raw=Buffer.concat(chunks); const body=JSON.parse(raw.toString());
  const n=requests.length+1;
  requests.push({path:req.url,body,hash:createHash('sha256').update(raw).digest('hex'),bytes:raw.length,authorizationCorrect:req.headers.authorization===`Bearer ${n===1?'synthetic-native-key-one':'synthetic-native-key-two'}`});
  if(n===4) { res.writeHead(500,{'Content-Type':'application/json'}); res.end(JSON.stringify({error:{message:'internal-secret-must-not-be-shown'}})); return; }
  res.writeHead(200,{'Content-Type':'text/event-stream','Cache-Control':'no-cache'});
  const event=(delta,finish=null)=>res.write(`data: ${JSON.stringify({model:body.model,choices:[{index:0,delta,finish_reason:finish}]})}\n\n`);
  event({role:'assistant'});
  const finish=()=>{event({},'stop');res.write(`data: ${JSON.stringify({choices:[],usage:{prompt_tokens:111,completion_tokens:22,total_tokens:133}})}\n\n`);res.end('data: [DONE]\n\n');};
  if(body.model==='gpt-5.6-luna') {
    memoryRequests++;
    if(memoryRequests===1) {
      const envelope=body.messages.map(m=>{try{return JSON.parse(m.content);}catch{return null;}}).find(m=>m?.purpose==='memoryAnalysis');
      assert(envelope, 'Memory request must carry its frozen analysis envelope');
      const blocks=(envelope.target.body.body??envelope.target.body).content;
      const paragraph=blocks.find(b=>b.type==='paragraph');
      const quote=paragraph.content.map(n=>n.type==='hardBreak'?'\n':n.text??'').join('');
      event({content:JSON.stringify({schemaVersion:'navigation-digest.v1',source:envelope.target.source,items:[{text:'The chapter keeps its ending unchanged.',evidence:[{blockId:paragraph.attrs.id,fromUtf16:0,toUtf16:quote.length,quote}],uncertainty:null}]})});
      finish();
    } else { event({content:'{"schemaVersion":"navigation-digest.v1"'}); }
  }
  else if(n===1) { event({content:'Mara waits at the station. '}); releaseFirst=()=>{event({content:'Her ending is unchanged.'});finish();}; }
  else if(n===2) { event({content:JSON.stringify({suggestions:[{title:'Sister perspective',replacementText:'Her sister',explanation:'Changes only the selected name.'}]})}); finish(); }
  else { event({content:'This is retained partial text. '}); }
});
await new Promise(resolvePromise=>api.listen(0,'127.0.0.1',resolvePromise));
const apiPort=api.address().port;

const metadata = {
  startedAt: new Date().toISOString(),
  executable,
  executableLength: build.size,
  executableSha256,
  executableModifiedAt: build.mtime.toISOString(),
  dataDirectory: data,
  dispatchCount: 0,
  model: null,
  checks: [],
};
let browser;
let page;
let clean = false;
async function cleanup() {
  if (clean) return;
  clean = true;
  await browser?.close().catch(() => {});
  if (app.exitCode === null) {
    app.kill();
    await new Promise(resolvePromise => {
      const timeout = setTimeout(resolvePromise, 5000);
      app.once('exit', () => { clearTimeout(timeout); resolvePromise(); });
    });
  }
  if (app.exitCode === null && app.pid) {
    await new Promise(resolvePromise => {
      const killer = spawn('taskkill.exe', ['/PID', String(app.pid), '/T', '/F'], { windowsHide: true, stdio: 'ignore' });
      killer.once('exit', resolvePromise);
      killer.once('error', resolvePromise);
    });
  }
}
try {
  const started = Date.now();
  while (Date.now() - started < 90000) {
    if (app.exitCode !== null) throw new Error(`native app exited before CDP (${app.exitCode})`);
    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2000) });
      if (response.ok && (await response.json()).webSocketDebuggerUrl) { browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, { timeout: 2000 }); break; }
    } catch {}
    await new Promise(resolvePromise => setTimeout(resolvePromise, 500));
  }
  if (!browser) throw new Error('Native HTTP qualification could not attach before its readiness deadline');
  const context = browser.contexts()[0];
  page = context.pages()[0] ?? await context.waitForEvent('page', { timeout: 10000 });
  const pageErrors = [];
  page.on('pageerror', error => pageErrors.push(error.message));

  await page.getByRole('heading',{name:'Your stories',exact:true}).waitFor();
  await page.getByRole('button',{name:'New project',exact:true}).click();
  await page.getByRole('textbox',{name:'Project title',exact:true}).fill('HTTP adapter qualification');
  await page.getByRole('button',{name:'Create project',exact:true}).click();
  await page.getByRole('button',{name:'Create a chapter',exact:true}).click();
  await page.getByLabel('Start with',{exact:true}).selectOption('chapter');
  await page.getByRole('textbox',{name:'Title',exact:true}).fill('The station');
  await page.getByRole('button',{name:'Create',exact:true}).click();
  assert.equal(await page.getByRole('tab',{name:/^Chapters/}).getAttribute('aria-selected'),'true');
  const manuscript=page.getByRole('textbox',{name:'Manuscript',exact:true});
  await manuscript.fill('Mara held the lantern. The ending stays unchanged.');
  await page.getByRole('status').filter({hasText:/^Saved$/}).waitFor();
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByRole('button',{name:'Add API connection',exact:true}).click();
  await page.getByLabel('Connection name',{exact:true}).fill('Synthetic compatible API');
  await page.getByLabel('Base URL',{exact:true}).fill(`http://127.0.0.1:${apiPort}/custom`);
  await page.locator('#endpoint-key').fill('synthetic-native-key-one');
  await page.locator('#endpoint-models').fill('test-editor-v1\ngpt-5.6-luna');
  await page.getByLabel('Request JSON mode for suggestions',{exact:true}).check();
  await page.getByRole('button',{name:'Save connection',exact:true}).click();
  await page.getByRole('button',{name:'Find models for Synthetic compatible API',exact:true}).click();
  await page.getByText('Model list refreshed. You can choose a model from the picker.',{exact:true}).waitFor();
  assert.match(await page.locator('.endpoint-list').innerText(), /3 models/);
  await page.getByRole('button',{name:'Find models for Synthetic compatible API',exact:true}).click();
  await page.getByRole('button',{name:'Stop model search',exact:true}).click();
  await page.getByRole('button',{name:'Stop model search',exact:true}).waitFor({state:'detached'});
  await page.getByText('Model search stopped. Your saved model list is unchanged.', {exact:true}).waitFor();
  assert.match(await page.locator('.endpoint-list').innerText(), /3 models/);
  await page.screenshot({path:resolve(evidence,'api-settings.png')});
  await page.getByRole('button',{name:'Close settings',exact:true}).click();
  await page.getByRole('button',{name:/^Choose model:/}).click();
  await page.getByRole('searchbox',{name:'Search models'}).fill('test-editor-v1');
  assert.equal(await page.locator('.model-choice').count(), 1);
  await page.screenshot({path:resolve(evidence,'model-picker.png')});
  await page.getByRole('searchbox',{name:'Search models'}).press('Enter');
  await page.getByRole('button',{name:'Choose model: test-editor-v1',exact:true}).waitFor();
  await page.getByRole('textbox',{name:'Discuss this document',exact:true}).fill('Discuss the scene without changing it.');
  await page.getByRole('button',{name:'Send',exact:true}).click();
  for(let i=0;i<100&&!releaseFirst;i++) await new Promise(r=>setTimeout(r,100));
  assert(releaseFirst,'HTTP server must receive the first request');
  // Route/key changes affect later requests; the captured first request stays on /custom.
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  await page.getByRole('button',{name:'Edit connection Synthetic compatible API',exact:true}).click();
  await page.getByLabel('Base URL',{exact:true}).fill(`http://127.0.0.1:${apiPort}/changed`);
  await page.locator('#endpoint-key').fill('synthetic-native-key-two');
  await page.getByRole('button',{name:'Save connection',exact:true}).click();
  await page.getByText('Connection saved. Choose its model from the model picker.',{exact:true}).waitFor();
  await page.getByRole('button',{name:'Close settings',exact:true}).click();
  releaseFirst();
  await page.locator('.feedback-note p').filter({hasText:/^Mara waits at the station\. Her ending is unchanged\.$/}).waitFor();
  await page.getByRole('button',{name:'Stop response',exact:true}).waitFor({state:'detached'});
  assert.equal(await manuscript.innerText(),'Mara held the lantern. The ending stays unchanged.');
  assert.equal(requests[0].path,'/custom/chat/completions'); assert(requests[0].authorizationCorrect); assert.equal(requests[0].body.model,'test-editor-v1'); assert.equal(requests[0].body.response_format,undefined);
  metadata.checks.push('Model discovery can be stopped without replacing the cached models');
  metadata.checks.push('Settings stores a credential privately; explicit model discovery and persistent picker select the exact HTTP model; in-flight route/key stays captured across Settings changes');
  await page.evaluate(()=>document.querySelector('.tiptap').editor.commands.setTextSelection({from:1,to:5}));
  await page.getByRole('button',{name:'Discuss selection',exact:true}).click();
  await page.getByRole('button',{name:'Suggest edits',exact:true}).click();
  await page.getByRole('textbox',{name:'Request edits for this passage',exact:true}).fill('Change only the selected name to her sister.');
  await page.getByRole('button',{name:'Send',exact:true}).click();
  const card=page.locator('.proposal-card').filter({hasText:'Sister perspective'});
  await card.getByRole('button',{name:'Preview',exact:true}).click();
  await card.locator('.after-text').filter({hasText:/^Her sister$/}).waitFor();
  assert.equal(await manuscript.innerText(),'Mara held the lantern. The ending stays unchanged.');
  await card.scrollIntoViewIfNeeded(); await page.screenshot({path:resolve(evidence,'http-proposal.png')});
  await card.getByRole('button',{name:'Apply',exact:true}).click();
  await card.locator('.proposal-status').filter({hasText:/^Applied$/}).waitFor();
  assert.equal(await manuscript.innerText(),'Her sister held the lantern. The ending stays unchanged.');
  assert.equal(requests[1].path,'/changed/chat/completions'); assert(requests[1].authorizationCorrect); assert.equal(requests[1].body.response_format.type,'json_object');
  metadata.checks.push('A second exact HTTP request uses updated route/key; strict JSON retains one scoped candidate and explicit Apply preserves the ending');
  await page.getByRole('button',{name:'Discuss',exact:true}).click();
  if(await page.getByRole('button',{name:'Use whole document',exact:true}).count()) await page.getByRole('button',{name:'Use whole document',exact:true}).click();
  await page.getByRole('textbox',{name:'Discuss this document',exact:true}).fill('Give a long scene discussion.');
  await page.getByRole('button',{name:'Send',exact:true}).click();
  await page.locator('.feedback-note p').filter({hasText:/^This is retained partial text\./}).last().waitFor();
  await page.getByRole('button',{name:'Stop response',exact:true}).click();
  await page.getByRole('button',{name:'Stop response',exact:true}).waitFor({state:'detached'});
  await page.getByRole('textbox',{name:'Discuss this document',exact:true}).fill('Check failure retention.');
  await page.getByRole('button',{name:'Send',exact:true}).click();
  await page.getByRole('button',{name:'Stop response',exact:true}).waitFor({state:'detached'});
  await page.waitForFunction(()=>document.querySelector('#discussion-composer')?.value==='');
  await page.getByRole('button',{name:'All projects',exact:true}).click();
  await page.getByRole('heading',{name:'Your stories',exact:true}).waitFor();
  await page.reload();
  await page.getByRole('button',{name:/^HTTP adapter qualification Last opened/}).click();
  await page.getByRole('heading',{name:'The station',exact:true}).waitFor();
  assert.equal(await manuscript.innerText(),'Her sister held the lantern. The ending stays unchanged.');
  const lib=new DatabaseSync(resolve(data,'library/library.sqlite3'),{readOnly:true}); const path=lib.prepare('SELECT path FROM entries LIMIT 1').get().path;lib.close();
  const db=new DatabaseSync(resolve(path,'project.sqlite3'),{readOnly:true});
  const results=db.prepare('SELECT outcome,confirmed_stdin_bytes,binding_json,delivery_json FROM provider_results ORDER BY rowid').all();
  assert.deepEqual(results.map(x=>x.outcome),['completed','completed','stopped','failed']);
  assert.equal(requests.length,4);
  results.forEach((r,i)=>{const d=JSON.parse(r.delivery_json); assert.equal(r.confirmed_stdin_bytes,0);assert.equal(d.bodyHash,requests[i].hash);assert.equal(Number(d.bodyBytes),requests[i].bytes);assert.equal(d.submission,'responseReceived');assert(!r.binding_json.includes('synthetic-native-key'));});
  assert.equal(JSON.parse(results[0].delivery_json).usage.inputTokens,111);
  assert.equal(db.prepare('SELECT count(*) n FROM proposals').get().n,1);
  assert.equal(db.prepare('SELECT count(*) n FROM proposal_decisions').get().n,1);
  db.close();
  assert(!String(await page.locator('body').innerText()).includes('internal-secret-must-not-be-shown'));
  metadata.results=results.map(r=>({...r,binding_json:JSON.parse(r.binding_json),delivery_json:JSON.parse(r.delivery_json)}));
  metadata.checks.push('Stop retains partial text, HTTP error is sanitized, exact body receipts survive reopen without another request, and only explicit Apply changes prose');
  await page.screenshot({path:resolve(evidence,'reopened.png')});
  // Story-memory transport is explicitly chosen independently of the author model.
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  const endpoint = await page.evaluate(async()=> (await window.__TAURI_INTERNALS__.invoke('endpoint_settings')).profiles.find(p=>p.label==='Synthetic compatible API'));
  await page.getByLabel('Maintenance provider',{exact:true}).selectOption(endpoint.id);
  await page.waitForFunction(async id=>{const state=await window.__TAURI_INTERNALS__.invoke('provider_state');return state.storyMemory.providerId===id&&state.storyMemory.ready;},endpoint.id);
  assert.equal(await page.getByLabel('Maintenance provider',{exact:true}).inputValue(),endpoint.id);
  await page.screenshot({path:resolve(evidence,'memory-settings.png')});
  await page.getByRole('button',{name:'Close settings',exact:true}).click();
  // Claude's static picker is a read-only synthetic qualification. It must
  // expose all reference rows and effort choices without probing Claude or
  // inspecting credentials; an unchecked connection keeps Send disabled.
  const memoryBeforeClaude = await page.evaluate(async()=>{
     const state=await window.__TAURI_INTERNALS__.invoke('provider_state');
     return { providerId:state.storyMemory.providerId, modelId:state.storyMemory.modelId, reasoning:state.storyMemory.reasoning, serviceTier:state.storyMemory.serviceTier, ready:state.storyMemory.ready };
  });
  await page.getByRole('button',{name:'Choose model: test-editor-v1',exact:true}).click();
  await page.getByRole('button',{name:'Claude Code',exact:true}).click();
  assert.equal(await page.locator('.model-choice').count(),3,'Claude rail must expose the three static reference models');
  for(const label of ['Claude Fable 5','Claude Opus 5','Claude Sonnet 5']) assert.equal(await page.locator('.model-choice').filter({hasText:label}).count(),1,`Claude rail must show ${label}`);
  await page.screenshot({path:resolve(evidence,'claude-picker.png')});
  await page.locator('.model-choice').filter({hasText:'Claude Sonnet 5'}).click();
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  const claudeEffort=page.locator('#model-reasoning');
  await claudeEffort.waitFor();
  assert.deepEqual(await claudeEffort.locator('option').evaluateAll(options=>options.map(option=>option.value).filter(Boolean)),['low','medium','high','xhigh','max']);
  await claudeEffort.selectOption('xhigh');
  await page.waitForFunction(async()=>{
     const state=await window.__TAURI_INTERNALS__.invoke('provider_state');
     return state.settings.active.providerId==='claude'&&state.settings.active.modelId==='claude-sonnet-5'&&state.settings.active.reasoning==='xhigh';
  });
  await page.getByRole('button',{name:'Close settings',exact:true}).click();
  await page.getByRole('button',{name:'Settings',exact:true}).click();
  assert.equal(await page.locator('#model-reasoning').inputValue(),'xhigh');
  await page.screenshot({path:resolve(evidence,'claude-settings.png')});
  const memoryAfterClaude=await page.evaluate(async()=>{
     const state=await window.__TAURI_INTERNALS__.invoke('provider_state');
     return { providerId:state.storyMemory.providerId, modelId:state.storyMemory.modelId, reasoning:state.storyMemory.reasoning, serviceTier:state.storyMemory.serviceTier, ready:state.storyMemory.ready };
  });
  assert.deepEqual(memoryAfterClaude,memoryBeforeClaude,'Changing the author model must not retarget Story Memory');
  await page.getByRole('button',{name:'Close settings',exact:true}).click();
  const claudeComposer=page.getByRole('textbox',{name:'Discuss this document',exact:true});
  if(!await claudeComposer.isVisible()) await page.getByRole('button',{name:'Discussion',exact:true}).click();
  await claudeComposer.fill('Synthetic Claude readiness check.');
  assert.equal(await page.getByRole('button',{name:'Send',exact:true}).isDisabled(),true,'Unchecked Claude connection must keep Send disabled');
  await page.getByRole('button',{name:'Hide discussion',exact:true}).click();
  await page.getByRole('button',{name:/^Choose model: Claude Sonnet 5$/}).click();
  await page.getByRole('searchbox',{name:'Search models'}).fill('test-editor-v1');
  await page.getByRole('searchbox',{name:'Search models'}).press('Enter');
  await page.getByRole('button',{name:'Choose model: test-editor-v1',exact:true}).waitFor();
  metadata.checks.push('Claude rail exposes three static models and five effort choices, saves and reopens an author choice, leaves the independent HTTP Luna memory provider unchanged, and keeps Send disabled until the native connection is checked');
  await page.getByRole('button',{name:'Choose model: test-editor-v1',exact:true}).waitFor();
  const within=relative(await realpath(data),await realpath(path));
  assert(within&&!isAbsolute(within)&&within!=='..'&&!within.startsWith(`..${sep}`),'Memory write fixture must stay inside this synthetic run directory');
  const memoryDb=new DatabaseSync(resolve(path,'project.sqlite3'));
  try {
    memoryDb.exec("CREATE TRIGGER native_http_memory_save_fault BEFORE INSERT ON memory_results BEGIN SELECT RAISE(ABORT,'synthetic HTTP memory save failure'); END;");
    await page.getByRole('button',{name:'Story memory',exact:true}).click();
    await page.getByRole('heading',{name:'No story memory yet',exact:true}).waitFor();
    assert.equal(requests.length,4,'Opening memory never starts a request');
    await page.getByRole('button',{name:'Refresh story memory',exact:true}).click();
    await page.getByRole('button',{name:'Check saved result',exact:true}).waitFor();
    assert.equal(requests.length,5);
    assert.equal(requests[4].body.model,'gpt-5.6-luna');
    assert.equal(requests[4].body.reasoning_effort,'xhigh');
    assert.equal(requests[4].body.service_tier,undefined);
    assert(requests[4].authorizationCorrect);
    assert.equal(requests[4].path,'/changed/chat/completions');
    memoryDb.exec('DROP TRIGGER native_http_memory_save_fault;');
    await page.getByRole('button',{name:'Check saved result',exact:true}).click();
    await page.getByRole('heading',{name:'Current story memory',exact:true}).waitFor();
    assert.equal(requests.length,5,'Local result reconciliation cannot repeat the POST');
    await page.getByRole('button',{name:'Inspect saved request',exact:true}).click();
    const inspectedRequest = page.getByRole('region',{name:'Saved story memory request'});
    await inspectedRequest.getByText('Story context',{exact:true}).click();
    await inspectedRequest.getByText(/Used · 1 source/).waitFor();
    await page.screenshot({path:resolve(evidence,'memory-result.png')});
    await page.getByRole('button',{name:'Back to writing',exact:true}).click();
    await page.reload();
    await page.getByRole('button',{name:/^HTTP adapter qualification Last opened/}).click();
    await page.getByRole('heading',{name:'The station',exact:true}).waitFor();
    await page.getByRole('button',{name:'Story memory',exact:true}).click();
    await page.getByRole('heading',{name:'Current story memory',exact:true}).waitFor();
    assert.equal(requests.length,5);
    await page.getByRole('button',{name:'Refresh story memory',exact:true}).click();
    const deadline=Date.now()+15000;
    while(requests.length<6&&Date.now()<deadline)await new Promise(r=>setTimeout(r,50));
    assert.equal(requests.length,6);
    await page.getByRole('button',{name:'Stop',exact:true}).click();
    await page.getByRole('button',{name:'Stop',exact:true}).waitFor({state:'detached'});
    let memoryResults=[];
    const stopSettlementDeadline=Date.now()+15000;
    while(memoryResults.length<2&&Date.now()<stopSettlementDeadline) {
      memoryResults=memoryDb.prepare('SELECT outcome,confirmed_stdin_bytes,delivery_json FROM memory_results ORDER BY rowid').all();
      if(memoryResults.length<2) await new Promise(r=>setTimeout(r,100));
    }
    assert.deepEqual(memoryResults.map(r=>r.outcome),['completed','stopped']);
    memoryResults.forEach((r,i)=>{const receipt=JSON.parse(r.delivery_json);assert.equal(r.confirmed_stdin_bytes,null);assert.equal(receipt.bodyHash,requests[i+4].hash);assert.equal(Number(receipt.bodyBytes),requests[i+4].bytes);});
    assert.equal(memoryDb.prepare('SELECT count(*) n FROM memory_views').get().n,1);
    assert.equal(await manuscript.innerText(),'Her sister held the lantern. The ending stays unchanged.');
    metadata.memoryResults=memoryResults.map(r=>({...r,delivery_json:JSON.parse(r.delivery_json)}));
    metadata.checks.push('API-only memory requests Luna/xhigh independently of the writer, saves exact HTTP delivery, recovers a failed local result save without another POST, survives reopen, and supports Stop without changing prose');
  } finally {memoryDb.exec('DROP TRIGGER IF EXISTS native_http_memory_save_fault;');memoryDb.close();}
  metadata.pageErrors=pageErrors; assert.deepEqual(pageErrors,[]); metadata.status='passed';
} catch(error) {
  metadata.status='failed';metadata.failure=String(error?.message??error).slice(0,1600);
  if(page) {metadata.body=await page.locator('body').innerText();await page.screenshot({path:resolve(evidence,'failure.png')}).catch(()=>{});}
  throw error;
} finally {
  // Remove only the synthetic profile's key through native app commands.
  if(page) metadata.credentialCleanup=await page.evaluate(async()=>{const s=await window.__TAURI_INTERNALS__.invoke('endpoint_settings');const p=s.profiles.find(p=>p.label==='Synthetic compatible API');if(!p)return 'not-created';await window.__TAURI_INTERNALS__.invoke('save_endpoint_settings',{request:{expectedRevision:s.revision,profileId:p.id,label:p.label,baseUrl:p.baseUrl,enabled:false,jsonMode:p.jsonMode,manualModelIds:p.manualModelIds,apiKey:{kind:'remove'}}});return 'removed';}).catch(()=> 'unconfirmed');
  if (metadata.status === 'passed' && metadata.credentialCleanup !== 'removed') {
    metadata.status = 'failed'; metadata.failure = 'Synthetic credential cleanup was not confirmed.';
  }
  metadata.requests=requests;metadata.liveModelCalls=0;metadata.finishedAt=new Date().toISOString();await cleanup();
  api.closeAllConnections();await new Promise(r=>api.close(r));
  await writeFile(resolve(evidence,'qualification.json'),JSON.stringify(metadata,null,2));
}
assert.equal(metadata.status, 'passed', metadata.failure);
console.log(JSON.stringify({status:metadata.status,checks:metadata.checks,requests:requests.length,credentialCleanup:metadata.credentialCleanup},null,2));
