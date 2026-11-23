const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

const engine = 'http://127.0.0.2:8011';
const app = 'http://127.0.0.1:1430';
const profile = fs.mkdtempSync(path.join(__dirname, 'browser-'));
const token = fs.readFileSync(path.join(__dirname, 'engine-home', 'session.token'), 'utf8').trim();
const chrome = spawn('C:/Program Files/Google/Chrome/Application/chrome.exe', [
    '--headless', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', '--user-data-dir=' + profile, '--window-size=1550,920', 'about:blank',
], { windowsHide: true, stdio: 'ignore' });

const evidence = { checks: [], browserCalls: [] };
const pending = new Map();
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
let socket;
let requestId = 0;
let send;
let evaluate;

async function until(label, check, timeout = 25000) {
    const deadline = Date.now() + timeout;
    do {
        const value = await check();
        if (value) return value;
        await sleep(250);
    } while (Date.now() < deadline);
    throw new Error('Timed out waiting for ' + label);
}

async function cep(method, params) {
    const response = await fetch(engine + '/v1/cep', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + token },
        body: JSON.stringify({ jsonrpc: '2.0', id: ++requestId, method, params }),
    });
    const reply = await response.json();
    if (!response.ok || reply.error) throw new Error(method + ': ' + JSON.stringify(reply.error ?? response.status));
    return reply.result;
}

async function click(label, matchTitle = false) {
    const action = `(() => {
        const button = [...document.querySelectorAll('button')].find(button =>
            !button.disabled && ${matchTitle ? '(button.title || button.getAttribute("aria-label"))' : 'button.textContent.trim()'} === ${JSON.stringify(label)});
        if (!button) return false;
        button.click();
        return true;
    })()`;
    await until('button ' + label, () => evaluate(action));
}

async function screenshot(name) {
    const image = await send('Page.captureScreenshot', { format: 'png' });
    fs.writeFileSync(path.join(__dirname, name + '.png'), Buffer.from(image.data, 'base64'));
}

async function waiting(excluding, name = 'weekly-report') {
    return until('run waiting for approval', async () => {
        const { runs } = await cep('flows.runs', { name, scope: 'user' });
        return runs.find(run => !excluding.has(run.run_id) && run.waiting_for?.kind === 'approval');
    });
}

async function decide(run, decision) {
    const approval = await until('approval request for ' + run.run_id, async () => {
        const { pending } = await cep('approvals.list', {});
        return pending.find(item => item.target?.includes(run.run_id));
    });
    return cep('approvals.' + decision, { approval_id: approval.id });
}

async function studioCheck() {
    const flowRef = { name: 'new-flow', scope: 'user' };
    await send('Page.navigate', { url: app + '/automation?s=workflows&flow=new' });
    await click('Add a step or trigger', true);
    const labels = [
        'By hand', 'Schedule', 'Every', 'Webhook', 'Poll', 'File', 'Engine event', 'Flow event', 'Called as a tool', 'On startup',
        'Ask a model', 'Agent', 'Code', 'If', 'Switch', 'For each', 'Parallel', 'Repeat', 'Wait', 'Approve', 'Set values',
        'Run a flow', 'Send an event', 'Return', 'Fail',
    ];
    const paletteNames = await until('palette entries', async () => {
        const values = await evaluate("Array.from(document.querySelectorAll('aside [draggable=true] span')).map(el => el.textContent.trim())");
        return values.length > 0 && values;
    });
    const missing = labels.filter(label => !paletteNames.includes(label));
    assert.deepEqual(missing, []);
    evidence.palette = { triggers: 10, stepTypes: 15, separateGroups: ['Installed tools', 'Engine methods'] };
    for (const group of evidence.palette.separateGroups) {
        await until(group, () => evaluate(`document.querySelector('aside').innerText.includes(${JSON.stringify(group)})`));
    }
    await screenshot('03-palette');
    async function pick(label) {
        await until('palette item ' + label, () => evaluate(`(() => {
            const item = [...document.querySelectorAll('aside [draggable=true]')].find(el =>
                [...el.querySelectorAll('span')].some(span => span.textContent.trim() === ${JSON.stringify(label)}));
            if (!item) return false;
            item.click();
            return true;
        })()`));
    }
    await pick('Return');
    await click('Undo (Ctrl+Z)', true);
    await click('Redo (Ctrl+Y)', true);
    await click('Save (Ctrl+S)', true);
    const saved = await until('App saved the new canvas', async () => {
        try { return await cep('flows.get', flowRef); } catch { return false; }
    });
    assert.match(saved.text, /return:/);
    evidence.checks.push('New App canvas: selected Return, used Undo and Redo, and saved the resulting YAML');
    await click('Run', true);
    const completed = await until('new App canvas run succeeded', async () =>
        (await cep('flows.runs', flowRef)).runs.find(run => run.status === 'succeeded'));
    evidence.created = { name: flowRef.name, runId: completed.run_id, status: completed.status };
    await cep('flows.disable', flowRef);
    assert.equal((await cep('flows.get', flowRef)).flow.enabled, false);
    await send('Page.navigate', { url: app + '/automation?s=workflows&flow=user:new-flow' });
    await click('Add a step or trigger', true);
    await pick('Set values');
    await click('Save (Ctrl+S)', true);
    const overwritten = await until('saved edited flow', async () => {
        const reply = await cep('flows.get', flowRef);
        return /id: set/.test(reply.text) && reply;
    });
    evidence.saveDisabled = { before: false, after: overwritten.flow.enabled };
    evidence.status = 'PASS';
}

async function aiReportCheck() {
    const name = 'weekly-report-ai';
    const prior = new Set((await cep('flows.runs', { name, scope: 'user' })).runs.map(run => run.run_id));
    await send('Page.navigate', { url: app + '/automation?s=workflows&flow=user:' + name });
    await click('Run', true);
    const started = await waiting(prior, name);
    const before = await cep('flows.run_get', { run_id: started.run_id });
    const written = path.join(__dirname, 'engine-home', 'workspace', 'weekly-report', 'report.md');
    assert.equal(fs.existsSync(written), false, 'report file must not exist before approval');
    const draft = before.steps.find(step => step.id === 'draft');
    assert.equal(draft?.status, 'succeeded');
    assert.match(draft.output, /build/i);
    await screenshot('04-ai-report-before-approval');
    await decide(started, 'approve');
    const result = await until('App AI report run to finish', async () => {
        const detail = await cep('flows.run_get', { run_id: started.run_id });
        return ['succeeded', 'failed', 'cancelled'].includes(detail.run.status) && detail;
    });
    assert.equal(result.run.status, 'succeeded', JSON.stringify(result.run.error));
    assert.equal(fs.readFileSync(written, 'utf8'), result.run.outputs.text);
    assert.equal(result.run.outputs.text, draft.output);
    evidence.aiReport = {
        runId: started.run_id, status: result.run.status, writtenAfterApproval: true,
        modelOutput: result.run.outputs.text, path: result.run.outputs.file,
    };
    evidence.status = 'PASS';
}

async function runBrowserCheck() {
    await until('browser endpoint', () => fs.existsSync(path.join(profile, 'DevToolsActivePort')));
    const port = fs.readFileSync(path.join(profile, 'DevToolsActivePort'), 'utf8').split('\n')[0];
    const tabs = await (await fetch('http://127.0.0.1:' + port + '/json/list')).json();
    socket = new WebSocket(tabs.find(tab => tab.type === 'page').webSocketDebuggerUrl);
    await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
    socket.onmessage = event => {
        const message = JSON.parse(event.data);
        if (message.method === 'Network.requestWillBeSent' && message.params.request.url.endsWith('/v1/cep')) {
            try {
                const method = JSON.parse(message.params.request.postData).method;
                evidence.browserCalls.push({ url: message.params.request.url, method });
            } catch { /* preflight */ }
        }
        const waiter = pending.get(message.id);
        if (!waiter) return;
        clearTimeout(waiter.timer);
        pending.delete(message.id);
        if (message.error) waiter.reject(new Error(message.error.message));
        else waiter.resolve(message.result);
    };
    send = (method, params = {}) => new Promise((resolve, reject) => {
        const id = ++requestId;
        const timer = setTimeout(() => {
            pending.delete(id);
            reject(new Error('Browser request timed out: ' + method));
        }, 15000);
        pending.set(id, { resolve, reject, timer });
        socket.send(JSON.stringify({ id, method, params }));
    });
    evaluate = async expression => {
        const reply = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
        if (reply.exceptionDetails) throw new Error(reply.exceptionDetails.text);
        return reply.result.value;
    };

    await send('Page.enable');
    await send('Network.enable');
    await send('Network.setBlockedURLs', { urls: ['*:8000/*', '*127.0.0.1:8011/*', '*localhost:8011/*'] });
    const configure = `(() => {
        if (location.origin !== ${JSON.stringify(app)}) return;
        const fetchOriginal = window.fetch.bind(window);
        window.fetch = (input, init) => {
            if (typeof input === 'string') {
                const url = new URL(input, location.href);
                if (['127.0.0.1', 'localhost'].includes(url.hostname) && ['8000', '8011'].includes(url.port)) {
                    input = ${JSON.stringify(engine)} + url.pathname + url.search;
                }
            }
            return fetchOriginal(input, init);
        };
        localStorage.setItem('cluaiz_api_token', ${JSON.stringify(token)});
        localStorage.setItem('cluaiz_connection_fallback', ${JSON.stringify(JSON.stringify({ protocol: 'http', host: '127.0.0.2', port: 8011 }))});
        window.workflowTestConfigured = true;
    })()`;
    await send('Page.addScriptToEvaluateOnNewDocument', { source: configure });
    await send('Page.navigate', { url: app + '/automation?s=workflows' });
    await until('browser route', () => evaluate(`location.origin === ${JSON.stringify(app)}`).catch(() => false));
    if (!(await evaluate('Boolean(window.workflowTestConfigured)'))) {
        await evaluate(configure);
        await send('Page.reload');
    }
    if (process.argv.includes('--studio')) {
        await studioCheck();
        return;
    }
    if (process.argv.includes('--ai-report')) {
        await aiReportCheck();
        return;
    }
    await until('Automation import', () => evaluate("Array.from(document.querySelectorAll('button')).some(el => el.textContent.trim() === 'Import')"));
    const { root } = await send('DOM.getDocument');
    const { nodeId } = await send('DOM.querySelector', { nodeId: root.nodeId, selector: 'input[type=file][accept=".yaml,.yml,.json"]' });
    assert.ok(nodeId, 'workflow file input is available');
    const hasWorkflow = async () => (await cep('flows.list', { scope: 'user' })).flows.some(flow => flow.name === 'weekly-report');
    if (!(await hasWorkflow())) {
        await send('DOM.setFileInputFiles', { nodeId, files: [path.join(__dirname, 'weekly-report.flow.yaml')] });
        await until('workflow imported', hasWorkflow);
        evidence.checks.push('App import saved weekly-report to the isolated Engine');
    }
    const previous = (await cep('flows.runs', { name: 'weekly-report', scope: 'user' })).runs;
    const prior = new Set(previous.map(run => run.run_id));
    for (const run of previous) {
        if (run.waiting_for?.kind === 'approval') {
            await decide(run, 'reject');
        }
    }
    await send('Page.navigate', { url: app + '/automation?s=workflows&flow=user:weekly-report' });
    await click('Run', true);
    const first = await waiting(prior);
    const firstDetail = await cep('flows.run_get', { run_id: first.run_id });
    const step = firstDetail.steps.find(item => item.id === 'review');
    assert.equal(step.kind, 'approve');
    assert.ok(['running', 'waiting'].includes(step.status), 'the started review step must have a status');
    evidence.waiting = { runStatus: first.status, stepStatus: step.status, kind: step.kind };
    evidence.checks.push('App Run reached approval with a valid live step record');
    await screenshot('01-studio-waiting');
    await send('Page.navigate', { url: app + '/automation?s=approvals' });
    await until('Approvals lists the waiting flow', () => evaluate("document.body.innerText.includes('weekly-report') && document.body.innerText.includes('waits for approval')"));
    evidence.approvalButtons = await evaluate("Array.from(document.querySelectorAll('button')).map(button => (button.getAttribute('aria-label') || button.textContent).trim()).filter(text => /^(approve|reject|yes|no)$/i.test(text))");
    await screenshot('02-approvals');
    await decide(first, 'approve');
    const approved = await until('approved run completes', async () => {
        const detail = await cep('flows.run_get', { run_id: first.run_id });
        return detail.run.status === 'succeeded' && detail;
    });
    assert.deepEqual(approved.run.outputs.updates, [
        'Drafted the release notes.', 'Checked the build.', 'Opened the documentation review.',
    ]);
    evidence.approved = { runId: first.run_id, status: approved.run.status, outputs: approved.run.outputs };
    await send('Page.navigate', { url: app + '/automation?s=workflows&flow=user:weekly-report' });
    await click('Run', true);
    prior.add(first.run_id);
    const second = await waiting(prior);
    await decide(second, 'reject');
    const rejected = await until('rejected run settles', async () => {
        const detail = await cep('flows.run_get', { run_id: second.run_id });
        return detail.run.status === 'failed' && detail;
    });
    assert.equal(rejected.run.error.kind, 'approval_rejected');
    evidence.rejected = { runId: second.run_id, status: rejected.run.status, kind: rejected.run.error.kind };
    evidence.checks.push('Approval through the Engine produced the report; rejection stopped the run');
    evidence.status = 'PASS';
}

runBrowserCheck().catch(async error => {
    evidence.status = 'FAIL';
    evidence.error = error.message;
    if (evaluate) {
        try { evidence.visible = await evaluate('document.body.innerText.slice(-3200)'); } catch { /* browser unavailable */ }
        try { await screenshot('browser-failure'); } catch { /* browser unavailable */ }
    }
    process.exitCode = 1;
}).finally(async () => {
    evidence.browserCalls = [...new Set(evidence.browserCalls.map(call => call.method + ' ' + call.url))];
    fs.writeFileSync(path.join(__dirname, 'browser-result-' + Date.now() + '.json'), JSON.stringify(evidence, null, 2));
    console.log(JSON.stringify(evidence, null, 2));
    if (evaluate) try { await evaluate("localStorage.removeItem('cluaiz_api_token')"); } catch { /* disposable profile */ }
    if (send) try { await send('Browser.close'); } catch { /* browser already closed */ }
    socket?.close();
    chrome.kill();
    try {
        await until('own Chrome exit', () => chrome.exitCode !== null || chrome.signalCode !== null, 10000);
        const localProfile = fs.realpathSync(profile);
        assert.equal(path.dirname(localProfile), fs.realpathSync(__dirname));
        assert.ok(path.basename(localProfile).startsWith('browser-'));
        fs.rmSync(localProfile, { recursive: true, force: true, maxRetries: 5, retryDelay: 150 });
    } catch (error) { console.error('Temporary browser cleanup: ' + error.message); }
});
