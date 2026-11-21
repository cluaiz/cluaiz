"""Deterministic fixed workload input generator for vps26.

Generates identical W1..W5 inputs once under <out_dir>/inputs/ so every engine
(Cluaiz, Ollama, llama-server) runs on exact same data.
"""
import argparse
import json
import random
from pathlib import Path

LABELS = ["billing", "bug", "account", "feature", "outage"]

TICKET_TEMPLATES = {
    "billing": [
        "I was charged twice on invoice #{num} for my {plan} subscription. Please refund the duplicate {amt} USD charge on card ending in {card}.",
        "My monthly renewal failed even though my Visa ending in {card} has funds, and I cannot download my PDF receipt for {month}.",
        "We upgraded from {plan} to team seats mid-cycle, and the prorated bill of {amt} USD looks wrong compared to our quote.",
        "Please update our billing VAT ID and send the paid tax invoice #{num} to our finance email before Friday.",
    ],
    "bug": [
        "When exporting a CSV report over 5,000 rows in {feature}, the browser tab freezes and logs TypeError cannot read property length in {module}.",
        "Clicking the Save Filter button in {module} throws HTTP 422 even though all required date fields are filled out properly.",
        "Dark mode contrast on the {module} modal makes the Cancel button text invisible on Firefox {ver} under Linux.",
        "After the latest {ver} update, webhook payloads from {module} arrive with an empty signature header and break our parser.",
    ],
    "account": [
        "I lost my authenticator phone and need my two-factor authentication reset so I can log back into my workspace account.",
        "Please transfer organization owner permissions from our departed manager to my email and revoke their active API sessions.",
        "My SSO SAML login keeps redirecting in a loop after we rotated our identity provider certificate this morning.",
        "I changed my email address last week, but the verification link expired and now my login says account locked.",
    ],
    "feature": [
        "Could you add a keyboard shortcut to archive notifications and support bulk tagging inside {module}?",
        "It would help our team if scheduled PDF exports could be delivered directly to a Slack channel every Monday morning.",
        "Please consider adding read-only audit log filtering by IP range and custom date presets in {module}.",
        "We would love support for dark-mode email digests and per-project retention rules in the settings panel.",
    ],
    "outage": [
        "All API requests to eu-west endpoints have been returning 503 Service Unavailable for the past 15 minutes across our production servers.",
        "Our dashboard shows Connection Timed Out for every database worker since 09:10 UTC and none of our scheduled jobs are running.",
        "Urgent: the login gateway is completely unreachable from multiple regions and returning TLS handshake failures.",
        "Production webhook delivery stopped completely 20 minutes ago and the status endpoint is returning 502 Bad Gateway.",
    ],
}

SERVICES = ["payments-api", "ingest-worker", "search-index", "auth-gateway", "report-builder", "billing-cron"]
REGIONS = ["eu-central-1", "us-east-1", "ap-south-1", "eu-west-2"]
TOPICS = [
    ("postgres-vacuum", "PostgreSQL autovacuum tuning and dead tuple bloat prevention"),
    ("redis-eviction", "Redis volatile-lru memory eviction and latency spikes"),
    ("tls-rotation", "Automated TLS certificate rotation and zero-downtime nginx reloads"),
    ("sqlite-wal", "SQLite WAL checkpoint stalling under concurrent background writes"),
    ("cgroup-oom", "Linux cgroup v2 MemoryMax throttling and swapless OOM kills"),
    ("sse-timeout", "Server-Sent Events proxy buffering timeouts in reverse proxies"),
    ("jwt-clock", "JWT token validation failures caused by NTP clock skew"),
    ("disk-iops", "NVMe burst IOPS exhaustion during container image extraction"),
    ("dns-cache", "systemd-resolved stub cache misses under high microservice lookup load"),
    ("backup-verify", "Automated SHA-256 restore drills for encrypted offsite backups"),
]


def make_w1(rng):
    plans = ["Starter", "Pro", "Growth", "Scale"]
    months = ["September", "October", "November", "December"]
    modules = ["Analytics", "Workflows", "Billing", "Webhooks", "Reports"]
    features = ["exports", "dashboards", "audit logs", "pipelines"]
    tickets = []
    for i in range(200):
        label = LABELS[i % len(LABELS)]
        tpl = TICKET_TEMPLATES[label][(i // len(LABELS) + rng.randint(0, 3)) % len(TICKET_TEMPLATES[label])]
        text = tpl.format(
            num=10000 + i * 7,
            plan=rng.choice(plans),
            amt=rng.choice([19, 49, 99, 149, 299]),
            card=1000 + (i * 13) % 8999,
            month=rng.choice(months),
            feature=rng.choice(features),
            module=rng.choice(modules),
            ver=f"2.{rng.randint(1, 9)}.{rng.randint(0, 8)}",
        )
        tickets.append({"id": i, "label": label, "text": text})
    rng.shuffle(tickets)
    return tickets


def make_w2(rng):
    reports = []
    for i in range(50):
        svc = SERVICES[i % len(SERVICES)]
        reg = REGIONS[i % len(REGIONS)]
        p95 = rng.randint(420, 3900)
        err_rate = round(rng.uniform(0.8, 14.5), 2)
        mem_mb = rng.randint(1400, 3600)
        paras = [
            (
                f"Incident Report #{2000 + i} covers operational behavior observed on {svc} in {reg} "
                f"between 08:00 and 11:30 UTC. Monitoring alerts triggered when p95 request latency "
                f"crossed {p95} ms and HTTP 5xx error rates reached {err_rate} percent across worker nodes. "
                f"At peak load, resident memory climbed to {mem_mb} MB while background compaction jobs "
                f"competed with interactive API traffic for CPU time."
            ),
        ]
        while len(" ".join(paras).split()) < 950:
            topic_slug, topic_desc = TOPICS[(i + len(paras)) % len(TOPICS)]
            qps = rng.randint(45, 420)
            dur = rng.randint(4, 38)
            paras.append(
                f"Subsystem trace {len(paras)} ({topic_slug}) recorded {topic_desc} during a burst of "
                f"{qps} requests per second lasting {dur} minutes. Engineers inspected kernel page cache "
                f"pressure, connection pool wait queues, and lock contention metrics across primary and "
                f"replica hosts. Temporary rate limits reduced tail latency by {rng.randint(25, 65)} percent, "
                f"but batch workers still queued behind synchronous database writes whenever checkpoint "
                f"intervals overlapped with scheduled reporting jobs. Mitigation steps included raising "
                f"pool timeouts, isolating long-running exports to a separate worker queue, lowering "
                f"per-request memory caps, adding explicit circuit breakers on slow upstream calls, "
                f"and scheduling automated regression checks before re-enabling full concurrency."
            )
        text = "\n\n".join(paras)
        reports.append({"id": i, "service": svc, "region": reg, "word_count": len(text.split()), "text": text})
    return reports


def make_w3(rng):
    sessions = []
    domains = ["inventory sync", "invoice export", "alert routing", "backup retention", "API rate limits"]
    for i in range(20):
        dom = domains[i % len(domains)]
        code = f"PRJ-{100 + i}"
        turns = [
            f"We are designing a small service called {code} for {dom} on a 1 vCPU Linux server. List two main risks in two short sentences.",
            f"For {code}, assume memory is capped at 3 GB with zero swap. How should we limit queue depth in one short paragraph?",
            f"What single health metric should our monitor check every second for {code}?",
            f"If {code} restarts after an OOM event, what two state checks should run before accepting traffic again?",
            f"Summarize our entire {code} plan in three bullet points.",
        ]
        sessions.append({"id": i, "code": code, "turns": turns})
    return sessions


def make_w4(rng):
    chunks = []
    for i in range(2000):
        slug, desc = TOPICS[i % len(TOPICS)]
        svc = SERVICES[(i // 10) % len(SERVICES)]
        reg = REGIONS[(i // 60) % len(REGIONS)]
        code = f"K-{10000 + i}"
        chunks.append({
            "id": i,
            "code": code,
            "topic": slug,
            "text": (
                f"Runbook {code} for {svc} in {reg}: {desc}. "
                f"Threshold limit is {50 + (i % 90)} percent and escalation target is team-{i % 12}. "
                f"Recovery command: restart unit {svc}-worker-{i % 8} after checking disk free space."
            ),
        })
    picked = rng.sample(range(2000), 200)
    queries = []
    for qid, target_id in enumerate(picked):
        c = chunks[target_id]
        queries.append({
            "id": qid,
            "target_id": target_id,
            "query": f"What is the runbook {c['code']} recovery command and threshold for {c['topic']}?",
        })
    return {"chunks": chunks, "queries": queries}


def make_w5():
    return {
        "texts": [
            {"id": "short", "text": "The server responded in twenty milliseconds with zero database errors."},
            {"id": "medium", "text": "Good infrastructure software should stay responsive on a small server even when background jobs and chat requests run together."},
            {"id": "long", "text": "On a shared virtual machine with limited memory, keeping the web application and database healthy matters more than peak batch throughput, because users notice every timeout immediately."},
        ]
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", required=True)
    args = ap.parse_args()
    inputs_dir = Path(args.out_dir) / "inputs"
    inputs_dir.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20261120)

    (inputs_dir / "w1_tickets.json").write_text(json.dumps(make_w1(rng), indent=2))
    (inputs_dir / "w2_reports.json").write_text(json.dumps(make_w2(rng), indent=2))
    (inputs_dir / "w3_chats.json").write_text(json.dumps(make_w3(rng), indent=2))
    (inputs_dir / "w4_embeddings.json").write_text(json.dumps(make_w4(rng), indent=2))
    (inputs_dir / "w5_speech.json").write_text(json.dumps(make_w5(), indent=2))
    print(f"Generated deterministic inputs in {inputs_dir}")


if __name__ == "__main__":
    main()
