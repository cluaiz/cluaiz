import json
import collections
import os

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPORT_PATH = os.path.join(SCRIPT_DIR, "tts_models_audit_report.json")
OUTPUT_JSON = os.path.join(SCRIPT_DIR, "extracted_family_mapping.json")

def main():
    print("[+] Reading tts_models_audit_report.json...")
    with open(REPORT_PATH, "r", encoding="utf-8") as f:
        data = json.load(f)

    audit_results = data.get("audit_results", [])
    print(f"[*] Total Audited Repos: {len(audit_results)}")

    family_counts = collections.Counter()
    family_to_repos = collections.defaultdict(list)

    for item in audit_results:
        repo_id = item.get("repo_id", "unknown")
        detected_family = item.get("detected_family", "missing")
        family_counts[detected_family] += 1
        family_to_repos[detected_family].append({
            "repo_id": repo_id,
            "primary_onnx": item.get("terminal_selection_flow", {}).get("raw_input_choices", [])
        })

    result_payload = {
        "total_repos_audited": len(audit_results),
        "total_unique_families": len(family_counts),
        "family_summary_counts": dict(family_counts.most_common()),
        "families": family_to_repos
    }

    with open(OUTPUT_JSON, "w", encoding="utf-8") as f:
        json.dump(result_payload, f, indent=2)

    print(f"[SUCCESS] Saved family mapping to: {OUTPUT_JSON}")

if __name__ == "__main__":
    main()
