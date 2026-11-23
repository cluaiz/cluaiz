const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { randomBytes } = require('node:crypto');
const { spawn } = require('node:child_process');
const { home, engine, tokenFile, cep, check, save, until, run, read, finished, byTrigger, returns, record } = require('./engine-check.cjs');

const exe = path.resolve(__dirname, '../../Cluaiz-Technologies/cluaiz/target/release/cluaiz.exe');
const stateFile = path.join(__dirname, 'restart-fixture.json');
const hookSecret = randomBytes(20).toString('hex');
const mathTool = 'plugin:math/math_evaluate';
const outputRef = id => '${{ steps.' + id + '.output }}';

async function prepare() {
    const startup = await save('startup', returns, { on: [{ startup: {} }] });
    const hook = await save('webhook', [{ id: 'result', return: { value: '${{ trigger.body.value }}' } }], {
        on: [{ webhook: { secret: 'env:CLUAIZ_WORKFLOW_ACCEPTANCE_HOOK' } }],
        secrets: ['CLUAIZ_WORKFLOW_ACCEPTANCE_HOOK'],
    });
    const pause = await save('restart', [
        { id: 'first', set: { survived: true } },
        { id: 'pause', wait: { for: '22s' } },
        { id: 'result', return: { survived: '${{ vars.survived }}' } },
    ]);
    const pending = (await run(pause)).run_id;
    await until('old Engine to record a waiting run', async () => (await read(pending)).run.status === 'waiting');
    fs.writeFileSync(stateFile, JSON.stringify({ startup, hook, pending }));
    console.log('Prepared restart fixture with waiting run ' + pending);
}

async function verifiedRun(suffix, steps, timeout = 25000) {
    const name = await save(suffix, steps);
    const launched = await run(name);
    return until(name + ' to complete', async () => {
        const detail = await read(launched.run_id);
        if (detail.run.status === 'waiting') {
            const { pending } = await cep('approvals.list', {});
            const approval = pending.find(item => item.target?.includes(launched.run_id));
            if (approval) await cep('approvals.approve', { approval_id: approval.id });
        }
        if (['succeeded', 'failed', 'cancelled'].includes(detail.run.status)) {
            assert.equal(detail.run.status, 'succeeded', JSON.stringify(detail.run.error));
            return detail;
        }
        return false;
    }, timeout);
}

async function mcp(method, params = {}) {
    const token = fs.readFileSync(tokenFile, 'utf8').trim();
    const response = await fetch(engine + '/mcp', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + token },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params }),
        signal: AbortSignal.timeout(15000),
    });
    const reply = await response.json();
    assert.ok(response.ok && !reply.error, JSON.stringify(reply.error ?? response.status));
    return reply.result;
}

async function withEngine(test) {
    const engineProcess = spawn(exe, ['serve'], {
        windowsHide: true,
        env: {
            ...process.env, CLUAIZ_HOME: home, CLUAIZ_PORT: '8011', CLUAIZ_HOST: '127.0.0.2',
            CLUAIZ_WORKFLOW_ACCEPTANCE_HOOK: hookSecret,
        },
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    let bound = false;
    engineProcess.stdout.on('data', bytes => { if (bytes.toString().includes('running on http://127.0.0.2:8011')) bound = true; });
    engineProcess.stderr.on('data', bytes => { if (bytes.toString().includes('running on http://127.0.0.2:8011')) bound = true; });
    try {
        await until('new isolated Engine listening', () => {
            if (engineProcess.exitCode !== null) throw Error('Engine exited with code ' + engineProcess.exitCode);
            return bound;
        });
        return await test();
    } finally {
        engineProcess.kill();
        await until('own Engine process stops', () => engineProcess.exitCode !== null || engineProcess.signalCode !== null);
    }
}

async function afterRestart() {
    const { hook, startup, pending } = JSON.parse(fs.readFileSync(stateFile, 'utf8'));
    return withEngine(async () => {
        await check('startup trigger', async () => {
            const hit = await byTrigger(startup, 'startup');
            return { kind: hit.trigger.kind, status: hit.status, runId: hit.run_id };
        });
        await check('restart recovery keeps the completed first step', async () => {
            const resumed = await until('resumed run to finish', async () => {
                const detail = await read(pending);
                return ['succeeded', 'failed', 'cancelled'].includes(detail.run.status) && detail;
            }, 55000);
            assert.equal(resumed.run.status, 'succeeded');
            assert.equal(resumed.run.outputs.survived, true);
            assert.equal(resumed.steps.filter(step => step.id === 'first').length, 1);
            return { status: resumed.run.status, resumed: pending, output: resumed.run.outputs };
        });
        await check('webhook rejects missing secret and processes valid body', async () => {
            const send = secret => fetch(engine + '/v1/hooks/' + hook, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json', ...(secret && { 'X-Cluaiz-Hook-Secret': secret }) },
                body: JSON.stringify({ value: 84 }),
            });
            const rejected = await send();
            assert.equal(rejected.status, 401);
            const accepted = await send(hookSecret);
            assert.equal(accepted.status, 202, await accepted.clone().text());
            const hit = await byTrigger(hook, 'webhook');
            assert.equal(hit.outputs.value, 84);
            return { rejected: rejected.status, accepted: accepted.status, output: hit.outputs.value };
        });
        await check('tool trigger is listed and callable through MCP', async () => {
            const name = await save('tool-trigger', returns, { on: [{ tool: { description: 'Returns a workflow acceptance result', confirm: false } }] });
            const tool = 'flow.' + name;
            const { tools } = await mcp('tools/list');
            assert.ok(tools.some(item => item.name === tool), 'the saved flow appears in MCP tools');
            const result = await mcp('tools/call', { name: tool, arguments: {} });
            assert.equal(Boolean(result.isError), false, JSON.stringify(result));
            const hit = await byTrigger(name, 'tool');
            assert.equal(hit.outputs.ok, true);
            return { listed: true, kind: hit.trigger.kind, outputs: hit.outputs };
        });
        await check('Code runs real WebAssembly', async () => {
            const detail = await verifiedRun('code', [
                { id: 'code', code: 'plugin:math', fn: 'execute_cel', with: { expression: '7 * 6' } },
                { id: 'result', return: { answer: outputRef('code') } },
            ]);
            assert.equal(detail.run.outputs.answer.result, 42);
            return detail.run.outputs;
        });
        await check('Tool runs installed math plugin', async () => {
            const detail = await verifiedRun('tool-step', [
                { id: 'tool', tool: mathTool, with: { expression: '7 * 6' } },
                { id: 'result', return: { answer: outputRef('tool') } },
            ]);
            assert.equal(detail.run.outputs.answer.result, 42);
            return detail.run.outputs;
        });
        const models = await cep('models.list', { category: 'chat' });
        const model = models.find(item => item.id.startsWith('qwen3.5-0.8b'));
        await check('Ask gets an answer from the local model', async () => {
            assert.ok(model, 'The isolated registry has no 0.8B model');
            const detail = await verifiedRun('ask', [
                { id: 'ask', ask: 'Answer with exactly the word READY in uppercase and nothing else.', model: model.id, think: 'off', max_tokens: 20 },
                { id: 'result', return: { answer: outputRef('ask') } },
            ], 60000);
            assert.match(detail.run.outputs.answer, /\bREADY\b/i);
            return { model: model.id, ...detail.run.outputs };
        });
        await check('Agent calls the math tool and answers', async () => {
            assert.ok(model, 'The isolated registry has no 0.8B model');
            const detail = await verifiedRun('agent', [
                { id: 'agent', agent: 'You must call the provided math tool to compute 743 * 9. After getting the tool result, reply with the number.', model: model.id, think: 'off', max_tokens: 100, max_turns: 3, tools: [mathTool] },
                { id: 'result', return: { answer: outputRef('agent') } },
            ], 60000);
            const step = detail.steps.find(item => item.id === 'agent');
            assert.match(JSON.stringify(step), /math_evaluate/);
            assert.match(detail.run.outputs.answer, /\b6687\b/);
            return { model: model.id, ...detail.run.outputs };
        });
    });
}

async function runBrowser(mode) {
    const child = spawn(process.execPath, [path.join(__dirname, 'browser-check.cjs'), mode], {
        windowsHide: true, stdio: 'inherit',
    });
    const result = await new Promise((resolve, reject) => {
        child.once('exit', resolve);
        child.once('error', reject);
    });
    if (result !== 0) process.exitCode = 1;
}

async function seedAiReport() {
    const folder = path.join(home, 'workspace', 'weekly-report');
    fs.mkdirSync(folder, { recursive: true });
    const source = path.join(folder, 'notes.txt');
    if (!fs.existsSync(source)) {
        fs.writeFileSync(source, 'The Windows build passed. The team drafted release notes. Documentation review is pending.\n');
    }
    const models = await cep('models.list', { category: 'chat' });
    const model = models.find(item => item.id.startsWith('qwen3.5-0.8b'));
    assert.ok(model);
    const ref = { name: 'weekly-report-ai', scope: 'user' };
    const existing = await cep('flows.list', { scope: ref.scope });
    const text = fs.readFileSync(path.join(__dirname, 'weekly-report-ai.flow.yaml'), 'utf8')
        .replace('    think: off', '    model: ' + JSON.stringify(model.id) + '\n    think: off');
    const result = await cep('flows.validate', { text, format: 'yaml' });
    assert.equal(result.valid, true, JSON.stringify(result.problems));
    await cep('flows.save', {
        text, format: 'yaml', scope: ref.scope, permissions_confirmed: true,
        overwrite: existing.flows.some(flow => flow.name === ref.name),
    });
    await runBrowser('--ai-report');
}

const prepareOnly = process.argv[2] === 'prepare';
const full = process.argv[2] === 'full';
const studio = process.argv[2] === 'studio';
const aiReport = process.argv[2] === 'ai-report';
void record(prepareOnly ? prepare() : full ? withEngine(prepare).then(afterRestart) : studio ? withEngine(() => runBrowser('--studio')) : aiReport ? withEngine(seedAiReport) : afterRestart(),
    prepareOnly ? 'restart-preparation' : studio ? 'studio-engine-result' : aiReport ? 'ai-engine-result' : 'runtime-result');
