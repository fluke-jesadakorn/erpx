import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { describe, it } from "vitest";

const node = resolve(process.env.ERPX_NODE_BIN ?? ".runtime/node/node-v24.20.0-darwin-arm64/bin/node");
const helper = pathToFileURL(resolve("scripts/testing/browser-readiness.mts")).href;
const execute = promisify(execFile);
async function contract(body: string) {
  await execute(node, ["--input-type=module", "-e", `
    import assert from 'node:assert/strict';
    import { createServer } from 'node:http';
    import { spawn } from 'node:child_process';
    import { waitForBrowserReadiness } from ${JSON.stringify(helper)};
    assert.equal(process.version, 'v24.20.0');
    const options = { deadlineMs: 350, requestMs: 60, intervalMs: 5, settleMs: 0 };
    ${body}
  `], { timeout: 5_000 });
}

describe("pinned Node browser readiness transport", () => {
  it("requires both endpoint successes and drains each response before proceeding", async () => {
    await contract(`
      const paths = new Set(); let finished = 0;
      const server = createServer((req, res) => {
        paths.add(req.url); res.writeHead(200); res.write('private partial body');
        setTimeout(() => { finished++; res.end('complete'); }, 30);
      });
      await new Promise(done => server.listen(0, '127.0.0.1', done));
      try {
        await waitForBrowserReadiness('http://127.0.0.1:' + server.address().port, process.pid, options);
        assert.deepEqual([...paths].sort(), ['/api/health', '/api/ready']);
        assert.equal(finished, 2);
      } finally { await new Promise(done => server.close(done)); }
    `);
  });
  it("rejects an unhealthy ready endpoint within the deadline even when health succeeds", async () => {
    await contract(`
      const paths = new Set();
      const server = createServer((req, res) => { paths.add(req.url); res.writeHead(req.url === '/api/health' ? 200 : 503); res.end(); });
      await new Promise(done => server.listen(0, '127.0.0.1', done));
      const start = Date.now();
      try {
        await assert.rejects(waitForBrowserReadiness('http://127.0.0.1:' + server.address().port, process.pid, options), /BROWSER_COHORT_NOT_READY/);
        assert.deepEqual([...paths].sort(), ['/api/health', '/api/ready']);
        assert.ok(Date.now() - start < 1000);
      } finally { await new Promise(done => server.close(done)); }
    `);
  });
  it("times out unfinished 200 bodies and closes their requests", async () => {
    await contract(`
      let closed = 0;
      const server = createServer((req, res) => { res.writeHead(200); res.write('partial'); res.on('close', () => closed++); });
      await new Promise(done => server.listen(0, '127.0.0.1', done));
      try {
        await assert.rejects(waitForBrowserReadiness('http://127.0.0.1:' + server.address().port, process.pid, options), /BROWSER_COHORT_NOT_READY/);
        await new Promise(done => setTimeout(done, 20));
        assert.ok(closed >= 2);
      } finally { server.closeAllConnections(); await new Promise(done => server.close(done)); }
    `);
  });
  it("terminates the tracked Node probe on interruption without leaving HTTP requests open", async () => {
    await contract(`
      let connected; const connection = new Promise(done => connected = done); let closed = 0;
      const server = createServer((req, res) => { res.writeHead(200); res.write('partial'); res.on('close', () => closed++); connected(); });
      await new Promise(done => server.listen(0, '127.0.0.1', done));
      const child = spawn(process.execPath, [new URL(${JSON.stringify(helper)}).pathname, 'http://127.0.0.1:' + server.address().port, String(process.pid)], { stdio: 'ignore' });
      try {
        await connection;
        const exit = new Promise(done => child.once('exit', (code, signal) => done({code, signal})));
        child.kill('SIGTERM');
        assert.deepEqual(await exit, {code: null, signal: 'SIGTERM'});
        await new Promise(done => setTimeout(done, 20));
        assert.ok(closed >= 1);
      } finally { child.kill('SIGTERM'); server.closeAllConnections(); await new Promise(done => server.close(done)); }
    `);
  });
  it("rejects an exited cohort process before making HTTP requests", async () => {
    await contract(`
      const child = spawn(process.execPath, ['-e', '']); const pid = child.pid;
      await new Promise(done => child.once('exit', done));
      await assert.rejects(waitForBrowserReadiness('http://127.0.0.1:3102', pid, options), /BROWSER_COHORT_PROCESS_EXITED/);
    `);
  });
});
