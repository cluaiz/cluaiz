"""
TTS Audit Report Analyzer
Reads the full 10,226 line audit JSON and compares:
  - siblings (actual files in repo) vs bundled_files (what downloader would bundle)
  - Detects: quantization redundancy, pipeline fragmentation, missing files, wrong bundles
"""
import json
import os
from pathlib import Path
from collections import defaultdict

AUDIT_FILE = r"c:\Users\Aryan\my\Cluaiz-workspace\Cluaiz-Technologies\cluaiz\test\tts_repo_audit\tts_models_audit_report.json"

def strip_quant_suffix(name: str) -> str:
    """Strip quantization suffix to get base model name"""
    name = name.lower()
    for ext in ['.gguf', '.onnx', '.onnx_data']:
        if name.endswith(ext):
            name = name[:len(name) - len(ext)]
    suffixes = ['_q4', '_q4f16', '_q8f16', '_fp16', '_int8', '_int4', 
                '_uint8', '_uint8f16', '_quantized', '_bnb4', '_q8', '_q2',
                '_q4f16', '.fp32', '.int8']
    for s in suffixes:
        if name.endswith(s):
            name = name[:len(name) - len(s)]
            break
    return name

def analyze():
    with open(AUDIT_FILE, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    repos = data.get("audit_results", data.get("repos", data)) if isinstance(data, dict) else data
    if isinstance(repos, dict):
        repos = repos.get("audit_results", repos.get("repos", [repos]))
    
    total_repos = len(repos)
    issues = []
    
    for repo in repos:
        repo_id = repo.get("repo_id", "UNKNOWN")
        detected_family = repo.get("detected_family", "unknown")
        siblings = set(repo.get("siblings", []))
        flow = repo.get("terminal_selection_flow", {})
        variants = flow.get("variants", [])
        
        # Get all actual ONNX files from siblings
        actual_onnx = [s for s in siblings if s.lower().endswith('.onnx')]
        actual_gguf = [s for s in siblings if s.lower().endswith('.gguf')]
        actual_data = [s for s in siblings if s.lower().endswith('.onnx_data') or s.lower().endswith('.onnx.data') or s.lower().endswith('.data')]
        
        for variant in variants:
            vid = variant.get("variant_id", "?")
            fmt = variant.get("format", "?")
            quant = variant.get("precision_quant_tag", "?")
            bundled = variant.get("bundled_files", [])
            size_gb = variant.get("total_download_size_gb", 0)
            
            # === CHECK 1: QUANTIZATION REDUNDANCY ===
            # In an ONNX bundle, check if multiple files are the same model but different quants
            if fmt == "ONNX":
                onnx_in_bundle = [f for f in bundled if f.lower().endswith('.onnx')]
                base_names = defaultdict(list)
                for f in onnx_in_bundle:
                    base = strip_quant_suffix(os.path.basename(f))
                    base_names[base].append(f)
                
                for base, files in base_names.items():
                    if len(files) > 1:
                        issues.append({
                            "repo": repo_id,
                            "family": detected_family,
                            "variant": vid,
                            "bug_type": "QUANT_REDUNDANCY",
                            "severity": "HIGH",
                            "detail": f"Same model '{base}' bundled {len(files)} times with different quants: {files}"
                        })
            
            # === CHECK 2: BUNDLED FILE NOT IN SIBLINGS ===
            # Every bundled file must exist in siblings (phantom files)
            for bf in bundled:
                if bf not in siblings:
                    # Check if it's a metadata file that could be auto-fetched
                    bl = bf.lower()
                    is_metadata = bl.endswith('.json') or bl.endswith('.txt') or bl.endswith('.yaml') or bl.endswith('.yml')
                    if not is_metadata:
                        issues.append({
                            "repo": repo_id,
                            "family": detected_family,
                            "variant": vid,
                            "bug_type": "PHANTOM_FILE",
                            "severity": "CRITICAL",
                            "detail": f"Bundled file '{bf}' does NOT exist in repo siblings"
                        })
            
            # === CHECK 3: MISSING .onnx.data COMPANION ===
            # If an ONNX file has a .data companion in siblings but it's not in the bundle
            if fmt == "ONNX":
                for f in onnx_in_bundle:
                    data_file = f + "_data"
                    data_file2 = f + ".data"
                    data_file3 = f.replace('.onnx', '.data')
                    
                    for df in [data_file, data_file2, data_file3]:
                        if df in siblings and df not in bundled:
                            issues.append({
                                "repo": repo_id,
                                "family": detected_family,
                                "variant": vid,
                                "bug_type": "MISSING_DATA_COMPANION",
                                "severity": "CRITICAL",
                                "detail": f"Data companion '{df}' exists in repo but NOT in bundle for '{f}'"
                            })
        
        # === CHECK 4: PIPELINE FRAGMENTATION (CosyVoice pattern) ===
        # If family is cosyvoice and there are 2+ ONNX variants where each is incomplete
        if detected_family == "cosyvoice" and len(variants) > 1:
            onnx_variants = [v for v in variants if v.get("format") == "ONNX"]
            if len(onnx_variants) > 1:
                # Check if any variant contains ALL ONNX files from siblings
                all_onnx_siblings = set(actual_onnx)
                any_complete = False
                for v in onnx_variants:
                    v_onnx = set(f for f in v.get("bundled_files", []) if f.lower().endswith('.onnx'))
                    if all_onnx_siblings.issubset(v_onnx):
                        any_complete = True
                
                if not any_complete:
                    issues.append({
                        "repo": repo_id,
                        "family": detected_family,
                        "variant": "ALL_ONNX",
                        "bug_type": "PIPELINE_FRAGMENTATION",
                        "severity": "HIGH",
                        "detail": f"CosyVoice repo has {len(onnx_variants)} ONNX variants but NONE contains all {len(all_onnx_siblings)} pipeline stages: {sorted(all_onnx_siblings)}"
                    })
        
        # === CHECK 5: MISSING voice_styles / voices DIRECTORY ===
        # Supertonic repos have voice_styles/, Kokoro has voices/
        voice_files = [s for s in siblings if 'voice_styles/' in s or ('voices/' in s and s.lower().endswith('.bin'))]
        if voice_files:
            for v in variants:
                if v.get("format") == "ONNX":
                    bundled_voices = [f for f in v.get("bundled_files", []) if 'voice_styles/' in f or 'voices/' in f]
                    if not bundled_voices:
                        issues.append({
                            "repo": repo_id,
                            "family": detected_family,
                            "variant": v.get("variant_id", "?"),
                            "bug_type": "MISSING_VOICES",
                            "severity": "HIGH",
                            "detail": f"Repo has {len(voice_files)} voice files but variant bundles NONE of them"
                        })
        
        # === CHECK 6: GGUF variants missing companion ONNX frontend ===
        # CosyVoice GGUF repos need frontend-onnx/ files
        if detected_family == "cosyvoice":
            frontend_files = [s for s in siblings if 'frontend-onnx/' in s and s.lower().endswith('.onnx')]
            for v in variants:
                if v.get("format") == "GGUF":
                    bundled_frontend = [f for f in v.get("bundled_files", []) if 'frontend-onnx/' in f]
                    if frontend_files and not bundled_frontend:
                        issues.append({
                            "repo": repo_id,
                            "family": detected_family,
                            "variant": v.get("variant_id", "?"),
                            "bug_type": "GGUF_MISSING_FRONTEND",
                            "severity": "HIGH",
                            "detail": f"CosyVoice GGUF variant missing frontend-onnx/ companion files"
                        })
        
        # === CHECK 7: VieNeu/multi-stage fragmentation ===
        # Non-cosyvoice repos with multiple DEFAULT ONNX variants might be pipeline fragmentation
        if detected_family not in ("cosyvoice",):
            onnx_default_variants = [v for v in variants if v.get("format") == "ONNX" and v.get("precision_quant_tag") == "DEFAULT"]
            if len(onnx_default_variants) > 1:
                # Check if these are actually pipeline stages that should be merged
                all_primary_files = [v.get("bundled_files", [None])[0] for v in onnx_default_variants if v.get("bundled_files")]
                
                # If all are in different dirs or root, might be fragmentation
                dirs = set()
                for pf in all_primary_files:
                    if pf:
                        parent = str(Path(pf).parent) 
                        dirs.add(parent if parent != "." else "ROOT")
                
                if len(dirs) >= 2:
                    issues.append({
                        "repo": repo_id,
                        "family": detected_family,
                        "variant": "MULTIPLE_DEFAULT",
                        "bug_type": "POSSIBLE_FRAGMENTATION",
                        "severity": "MEDIUM",
                        "detail": f"{len(onnx_default_variants)} separate DEFAULT ONNX variants across dirs {dirs}. May need merging: {all_primary_files}"
                    })

        # === CHECK 8: Duplicate variant_ids ===
        seen_vids = defaultdict(int)
        for v in variants:
            vid = v.get("variant_id", "?")
            seen_vids[vid] += 1
        for vid, count in seen_vids.items():
            if count > 1:
                issues.append({
                    "repo": repo_id,
                    "family": detected_family,
                    "variant": vid,
                    "bug_type": "DUPLICATE_VARIANT_ID",
                    "severity": "MEDIUM",
                    "detail": f"Variant ID '{vid}' appears {count} times — user can't distinguish between them in UI"
                })

    # === PRINT RESULTS ===
    print(f"\n{'='*80}")
    print(f"TTS AUDIT REPORT ANALYSIS -- {total_repos} repos scanned")
    print(f"{'='*80}\n")
    
    # Group by bug type
    by_type = defaultdict(list)
    for issue in issues:
        by_type[issue["bug_type"]].append(issue)
    
    for bug_type in sorted(by_type.keys()):
        type_issues = by_type[bug_type]
        print(f"\n{'-'*60}")
        print(f"[BUG] {bug_type} ({len(type_issues)} occurrences)")
        print(f"{'-'*60}")
        for i, issue in enumerate(type_issues, 1):
            print(f"  {i}. [{issue['severity']}] {issue['repo']} (family={issue['family']})")
            print(f"     Variant: {issue['variant']}")
            print(f"     Detail: {issue['detail']}")
            print()
    
    # Summary
    print(f"\n{'='*80}")
    print(f"SUMMARY")
    print(f"{'='*80}")
    print(f"Total repos analyzed: {total_repos}")
    print(f"Total issues found: {len(issues)}")
    print(f"By severity:")
    for sev in ["CRITICAL", "HIGH", "MEDIUM", "LOW"]:
        count = sum(1 for i in issues if i["severity"] == sev)
        if count > 0:
            print(f"  {sev}: {count}")
    print(f"By type:")
    for bt in sorted(by_type.keys()):
        print(f"  {bt}: {len(by_type[bt])}")
    
    # Save structured output
    output_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "audit_analysis_results.json")
    with open(output_path, 'w', encoding='utf-8') as f:
        json.dump({
            "total_repos": total_repos,
            "total_issues": len(issues),
            "issues": issues,
            "summary_by_type": {k: len(v) for k, v in by_type.items()}
        }, f, indent=2, ensure_ascii=False)
    print(f"\nResults saved to: {output_path}")

if __name__ == "__main__":
    analyze()
