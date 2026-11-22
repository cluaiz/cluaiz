// A small Node.js website with a PostgreSQL database, run next to the AI engine during a test: a shop page
// (HTML front end), a product API and an order API (back end). /health keeps neighbour_app.py's contract
// (one indexed read and one write) so run26.sh's wait and sampler.py's per-second check work unchanged.
// Needs Node 18+ and the `pg` module (Ubuntu: apt install nodejs node-pg).
// Usage: node server.js [port] [dbname]
"use strict";
const http = require("http");
const { Pool } = require("pg");

const PORT = Number(process.argv[2] || 3100);
// The Unix socket uses peer login like psycopg2 in neighbour_app.py; TCP would ask for a password
const pool = new Pool({ host: "/var/run/postgresql", database: process.argv[3] || "neighbour_app", max: 8 });
const CATEGORIES = ["orders", "users", "billing", "events", "audit"];
let count = 0;

async function setup() {
  await pool.query(`CREATE TABLE IF NOT EXISTS items (id serial PRIMARY KEY, category text NOT NULL,
    payload text NOT NULL, updated_at timestamptz NOT NULL DEFAULT now())`);
  await pool.query("CREATE INDEX IF NOT EXISTS idx_items_category ON items (category)");
  await pool.query("CREATE TABLE IF NOT EXISTS pings (id serial PRIMARY KEY, ts timestamptz NOT NULL DEFAULT now(), note text NOT NULL)");
  await pool.query(`CREATE TABLE IF NOT EXISTS orders (id serial PRIMARY KEY, item_id int NOT NULL, qty int NOT NULL,
    customer text NOT NULL, ts timestamptz NOT NULL DEFAULT now())`);
  const { rows } = await pool.query("SELECT count(*)::int AS n FROM items");
  if (rows[0].n === 0) {
    await pool.query(`INSERT INTO items (category, payload) SELECT (ARRAY['orders','users','billing','events','audit'])[1 + g % 5],
      repeat(md5(g::text), 4) FROM generate_series(1, 5000) g`);
  }
}

function send(res, code, type, body) {
  res.writeHead(code, { "Content-Type": type, "Content-Length": Buffer.byteLength(body) });
  res.end(body);
}

const escape = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

async function handle(req, res) {
  const url = new URL(req.url, "http://x");
  const idx = ++count;
  if (req.method === "GET" && url.pathname === "/health") {
    const t0 = performance.now();
    const cat = CATEGORIES[idx % CATEGORIES.length];
    const c = await pool.connect();
    try {
      const n = (await c.query("SELECT count(*)::int AS n FROM items WHERE category = $1", [cat])).rows[0].n;
      const id = (await c.query("INSERT INTO pings (note) VALUES ($1) RETURNING id", [`h-${idx}`])).rows[0].id;
      if (idx % 500 === 0) await c.query("DELETE FROM pings WHERE id < $1", [id - 5000]);
      const db_ms = Math.round((performance.now() - t0) * 100) / 100;
      return send(res, 200, "application/json", JSON.stringify({ ok: true, category: cat, count: n, db_ms }));
    } finally {
      c.release();
    }
  }
  if (req.method === "GET" && url.pathname === "/") {
    const { rows } = await pool.query(`SELECT o.id, o.qty, o.customer, i.category FROM orders o JOIN items i ON i.id = o.item_id
      ORDER BY o.id DESC LIMIT 20`);
    const list = rows.map((r) => `<li>#${r.id} ${escape(r.customer)}: ${r.qty} x ${escape(r.category)}</li>`).join("");
    return send(res, 200, "text/html; charset=utf-8",
      `<!doctype html><html><head><meta charset="utf-8"><title>Shop</title></head><body><h1>Shop</h1>` +
      `<p>Latest orders</p><ul>${list}</ul><script>fetch('/api/items?category=billing')</script></body></html>`);
  }
  if (req.method === "GET" && url.pathname === "/api/items") {
    const cat = url.searchParams.get("category") || "orders";
    const { rows } = await pool.query("SELECT id, category, left(payload, 32) AS name FROM items WHERE category = $1 ORDER BY id DESC LIMIT 50", [cat]);
    return send(res, 200, "application/json", JSON.stringify(rows));
  }
  if (req.method === "POST" && url.pathname === "/api/orders") {
    let body = "";
    for await (const chunk of req) body += chunk;
    const o = JSON.parse(body || "{}");
    const { rows } = await pool.query("INSERT INTO orders (item_id, qty, customer) VALUES ($1, $2, $3) RETURNING id",
      [Number(o.item_id) || 1, Number(o.qty) || 1, String(o.customer || "guest").slice(0, 40)]);
    if (rows[0].id % 1000 === 0) await pool.query("DELETE FROM orders WHERE id < $1", [rows[0].id - 5000]);
    return send(res, 201, "application/json", JSON.stringify({ id: rows[0].id }));
  }
  send(res, 404, "application/json", '{"error":"not_found"}');
}

setup().then(() => {
  http.createServer((req, res) => {
    handle(req, res).catch((e) => send(res, 500, "application/json", JSON.stringify({ ok: false, error: String(e).slice(0, 200) })));
  }).listen(PORT, "127.0.0.1", () => console.log(`webapp on ${PORT}`));
}).catch((e) => { console.error(e); process.exit(1); });
