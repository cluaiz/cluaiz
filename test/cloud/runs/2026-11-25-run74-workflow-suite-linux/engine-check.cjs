const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const home = path.resolve(process.env.CLUAIZ_HOME || '');
const port = Number(process.env.CLUAIZ_PORT);
if (!process.env.CLUAIZ_HOME || !Number.isInteger(port) || [8000, 8011].includes(port) ||
    home === path.join(require('node:os').homedir(), '.cluaiz')) {
    throw new Error('A separate CLUAIZ_HOME and test port are required');
}
const engine = `http://127.0.0.1:${port}`;
const tokenFile = path.join(home, 'session.token');
const prefix = 'qa-' + Date.now();
const cases = [];
let id = 0;

async function cep(method, params) {
    const token = fs.readFileSync(tokenFile, 'utf8').trim();
    const response = await fetch(engine + '/v1/cep', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Authorization: 'Bearer ' + token },
        body: JSON.stringify({ jsonrpc: '2.0', id: ++id, method, params }),
        signal: AbortSignal.timeout(8000),
    });
    const reply = await response.json();
    if (!response.ok || reply.error) throw new Error(method + ': ' + JSON.stringify(reply.error ?? response.status));
    return reply.result;
}

async function check(label, test) {
    try {
        const evidence = await test();
        cases.push({ label, status: 'PASS', evidence });
        console.log('PASS ' + label + ' ' + JSON.stringify(evidence));
    } catch (error) {
        cases.push({ label, status: 'FAIL', error: error.message });
        console.log('FAIL ' + label + ' ' + error.message);
        process.exitCode = 1;
    }
}

async function save(suffix, steps, fields = {}) {
    const name = prefix + '-' + suffix;
    const text = JSON.stringify({ flow: 'cluaiz.flow/1', name, ...fields, steps });
    const check = await cep('flows.validate', { text, format: 'json' });
    assert.equal(check.valid, true, JSON.stringify(check.problems));
    return (await cep('flows.save', { text, format: 'json', scope: 'user', permissions_confirmed: true })).flow.name;
}

async function until(label, check, timeout = 18000) {
    const deadline = Date.now() + timeout;
    do {
        const value = await check();
        if (value) return value;
        await new Promise(resolve => setTimeout(resolve, 350));
    } while (Date.now() < deadline);
    throw Error('Timed out waiting for ' + label);
}

const terminal = new Set(['succeeded', 'failed', 'cancelled']);
const run = (name, params = {}) => cep('flows.run', { name, scope: 'user', ...params });
const read = runId => cep('flows.run_get', { run_id: runId });
const finished = id => until('finished run', async () => {
    const detail = await read(id);
    return terminal.has(detail.run.status) && detail;
});
const byTrigger = (name, kind) => until(name + ' to fire via ' + kind, async () => {
    const { runs } = await cep('flows.runs', { name, scope: 'user' });
    return runs.find(run => run.trigger?.kind === kind && run.status === 'succeeded');
}, 78000);
const returns = [{ id: 'result', return: { ok: true } }];

async function main() {
    await check('call, switch, parallel, repeat, wait, run, return and rerun_from', async () => {
        const child = await save('child', returns);
        const name = await save('logic', [
            { id: 'first', set: { ready: true } },
            { id: 'info', call: 'system.info' },
            { id: 'route', switch: [{ when: { var: 'vars.ready' }, do: [{ id: 'matched', set: { selected: true } }] }] },
            { id: 'parallel', parallel: { left: [{ id: 'left', set: { left: 1 } }], right: [{ id: 'right', set: { right: 2 } }] } },
            { id: 'repeat', repeat: [{ id: 'repeat_body', set: { ready: true } }], until: true, max: 2 },
            { id: 'pause', wait: { for: '2s' } },
            { id: 'child', run: child },
            { id: 'result', return: { child: '${{ steps.child.output }}' } },
        ]);
        const initial = await run(name);
        const done = await finished(initial.run_id);
        assert.equal(done.run.status, 'succeeded', JSON.stringify(done.run.error));
        assert.equal(done.run.outputs.child.ok, true);
        for (const id of ['info', 'route', 'matched', 'parallel', 'left', 'right', 'repeat', 'repeat_body', 'pause', 'child', 'result']) {
            assert.ok(done.steps.some(step => step.id === id && step.status === 'succeeded'), id);
        }
        const replay = await cep('flows.rerun_from', { run_id: initial.run_id, from: 'child' });
        const rerun = await finished(replay.run_id);
        assert.equal(rerun.run.status, 'succeeded');
        assert.deepEqual(rerun.run.outputs, done.run.outputs);
        return { status: done.run.status, rerun: rerun.run.status, outputs: done.run.outputs };
    });
    await check('fail step yields an explicit error', async () => {
        const name = await save('negative', [{ id: 'stop', fail: 'Expected acceptance failure' }]);
        const detail = await finished((await run(name)).run_id);
        assert.equal(detail.run.status, 'failed');
        assert.equal(detail.run.error.message, 'Expected acceptance failure');
        return { status: detail.run.status, error: detail.run.error.message };
    });
    await check('emit step, event trigger, Engine event trigger', async () => {
        const engineReceiver = await save('engine-event', returns, {
            on: [{ engine: 'flow.finished', filter: { eq: [{ var: 'trigger.data.flow' }, prefix + '-sender'] } }],
        });
        const receiver = await save('event', [{ id: 'receive', return: { value: '${{ trigger.data.value }}' } }], {
            on: [{ event: prefix + '.signal' }],
        });
        const sender = await save('sender', [{ id: 'send', emit: prefix + '.signal', with: { value: 42 } }, ...returns]);
        const done = await finished((await run(sender)).run_id);
        assert.equal(done.run.status, 'succeeded', JSON.stringify(done.run.error));
        const [event, engineEvent] = await Promise.all([byTrigger(receiver, 'event'), byTrigger(engineReceiver, 'engine')]);
        assert.equal(event.outputs.value, 42);
        return { emit: done.run.status, event: event.trigger.kind, value: event.outputs.value, engineEvent: engineEvent.trigger.kind };
    });
    const timerCases = [];
    for (const [kind, trigger] of [
        ['schedule', { schedule: '* * * * *' }],
        ['every', { every: '1m' }],
        ['poll', { poll: { call: 'system.info', every: '1m', first: 'run' } }],
    ]) {
        timerCases.push(check(kind + ' trigger', async () => {
            const name = await save(kind, returns, { on: [trigger] });
            const hit = await byTrigger(name, kind);
            await cep('flows.disable', { name, scope: 'user' });
            return { kind: hit.trigger.kind, status: hit.status, runId: hit.run_id };
        }));
    }
    await check('file trigger sees a new matching file', async () => {
        const folder = path.join(home, 'workspace', 'flow-watch');
        fs.mkdirSync(folder, { recursive: true });
        const name = await save('file', returns, {
            on: [{ file: { path: folder, glob: '*.txt', events: ['created'], debounce: '300ms' } }],
        });
        await new Promise(resolve => setTimeout(resolve, 800));
        fs.writeFileSync(path.join(folder, prefix + '.txt'), 'Isolated workflow trigger');
        const hit = await byTrigger(name, 'file');
        return { kind: hit.trigger.kind, status: hit.status, runId: hit.run_id };
    });
    await Promise.all(timerCases);
}

function record(task, filename = 'engine-result') {
    return task.catch(error => {
    cases.push({ label: 'suite', status: 'FAIL', error: error.message });
    process.exitCode = 1;
    }).finally(() => {
        fs.mkdirSync(path.join(__dirname, 'raw'), { recursive: true });
        fs.writeFileSync(path.join(__dirname, 'raw', filename + '-' + Date.now() + '.json'), JSON.stringify(cases, null, 2));
        console.log('Totals: ' + cases.filter(item => item.status === 'PASS').length + ' passed, ' +
            cases.filter(item => item.status === 'FAIL').length + ' failed');
    });
}

module.exports = { home, engine, tokenFile, cep, check, save, until, run, read, finished, byTrigger, returns, record };
if (require.main === module) void record(main());
