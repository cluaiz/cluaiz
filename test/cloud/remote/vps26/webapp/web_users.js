// Simulated website users for webapp/server.js. Each user opens the shop page, reads a product list,
// places an order, then waits 1-3 s, until SIGTERM. Every request goes to a CSV (ts, user, route, status, ms);
// a summary per minute goes to stdout. A request that fails or takes over 5 s counts as a failure.
// Usage: node web_users.js <base_url> <users> <out.csv>
"use strict";
const fs = require("fs");

const BASE = process.argv[2] || "http://127.0.0.1:3100";
const USERS = Number(process.argv[3] || 20);
const out = fs.createWriteStream(process.argv[4] || "web_users.csv");
out.write("ts,user,route,status,ms\n");
const CATS = ["orders", "users", "billing", "events", "audit"];
let running = true;
let minute = { n: 0, fail: 0, ms: [] };

async function call(user, route, opts) {
  const t0 = performance.now();
  let status = 0;
  try {
    const r = await fetch(BASE + route, { ...opts, signal: AbortSignal.timeout(5000) });
    await r.text();
    status = r.status;
  } catch {
    status = 0;
  }
  const ms = Math.round(performance.now() - t0);
  out.write(`${new Date().toISOString()},${user},${route.split("?")[0]},${status},${ms}\n`);
  minute.n++;
  minute.ms.push(ms);
  if (status === 0 || status >= 500) minute.fail++;
}

async function user(id) {
  await new Promise((r) => setTimeout(r, Math.random() * 3000));
  while (running) {
    await call(id, "/");
    await call(id, `/api/items?category=${CATS[Math.floor(Math.random() * CATS.length)]}`);
    await call(id, "/api/orders", { method: "POST", headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ item_id: 1 + Math.floor(Math.random() * 5000), qty: 1 + Math.floor(Math.random() * 3), customer: `user${id}` }) });
    await new Promise((r) => setTimeout(r, 1000 + Math.random() * 2000));
  }
}

setInterval(() => {
  const s = minute.ms.sort((a, b) => a - b);
  const p = (q) => (s.length ? s[Math.min(s.length - 1, Math.floor(q * s.length))] : 0);
  console.log(JSON.stringify({ ts: new Date().toISOString(), requests: minute.n, failed: minute.fail, p50_ms: p(0.5), p95_ms: p(0.95), max_ms: s[s.length - 1] || 0 }));
  minute = { n: 0, fail: 0, ms: [] };
}, 60000);

process.on("SIGTERM", () => { running = false; setTimeout(() => { out.end(); process.exit(0); }, 6000); });
for (let i = 1; i <= USERS; i++) user(i);
