"""Lightweight neighbour web app backed by native PostgreSQL.

Simulates a developer's existing web service sharing the VPS with an AI engine.
Every GET /health runs one indexed PostgreSQL read and one small write.
"""
import argparse
import json
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import psycopg2

CATEGORIES = ("orders", "users", "billing", "events", "audit")
THREAD_LOCAL = threading.local()
COUNTER_LOCK = threading.Lock()
REQ_COUNT = 0


def get_conn(dsn):
    conn = getattr(THREAD_LOCAL, "conn", None)
    if conn is not None and conn.closed == 0:
        return conn
    conn = psycopg2.connect(dsn)
    conn.autocommit = False
    THREAD_LOCAL.conn = conn
    return conn


def drop_conn():
    conn = getattr(THREAD_LOCAL, "conn", None)
    if conn is not None:
        try:
            conn.close()
        except Exception:
            pass
        THREAD_LOCAL.conn = None


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    dsn = "dbname=neighbour_app"

    def log_message(self, fmt, *args):
        return

    def do_GET(self):
        global REQ_COUNT
        if self.path not in ("/health", "/"):
            body = b'{"error":"not_found"}'
            self.send_response(404)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return

        with COUNTER_LOCK:
            REQ_COUNT += 1
            idx = REQ_COUNT
        cat = CATEGORIES[idx % len(CATEGORIES)]
        t0 = time.perf_counter()
        try:
            conn = get_conn(self.dsn)
            with conn.cursor() as cur:
                cur.execute("SELECT count(*) FROM items WHERE category = %s", (cat,))
                count = cur.fetchone()[0]
                cur.execute("INSERT INTO pings(note) VALUES (%s) RETURNING id", (f"h-{idx}",))
                ping_id = cur.fetchone()[0]
                if idx % 500 == 0:
                    cur.execute("DELETE FROM pings WHERE id < %s", (ping_id - 5000,))
            conn.commit()
            db_ms = round((time.perf_counter() - t0) * 1000.0, 2)
            payload = json.dumps({"ok": True, "category": cat, "count": count, "db_ms": db_ms}).encode()
            self.send_response(200)
        except Exception as exc:
            drop_conn()
            db_ms = round((time.perf_counter() - t0) * 1000.0, 2)
            payload = json.dumps({"ok": False, "db_ms": db_ms, "error": str(exc)[:200]}).encode()
            self.send_response(500)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", default="127.0.0.1")
    ap.add_argument("--port", type=int, default=3100)
    ap.add_argument("--dbname", default="neighbour_app")
    args = ap.parse_args()
    Handler.dsn = f"dbname={args.dbname}"
    srv = ThreadingHTTPServer((args.host, args.port), Handler)
    srv.daemon_threads = True
    srv.serve_forever()


if __name__ == "__main__":
    main()
