import json
import os
from collections import defaultdict

def main():
    audit_report_path = os.path.join(os.path.dirname(__file__), "tts_models_audit_report.json")
    output_path = os.path.join(os.path.dirname(__file__), "tts_family_bundle_manifest.json")

    if not os.path.exists(audit_report_path):
        print(f"❌ Error: Audit report not found at {audit_report_path}")
        return

    with open(audit_report_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    audit_results = data.get("audit_results", [])
    print(f"[AUDIT] Auditing {len(audit_results)} repository results across TTS families...")

    family_bundles = defaultdict(lambda: {
        "repos": [],
        "primary_onnx_files": set(),
        "manifest_config_files": set(),
        "vocab_and_token_files": set(),
        "voice_styles_and_weights": set(),
        "other_extra_files": set()
    })

    # Domain Knowledge: Field extraction schemas per family
    family_schema_extractors = {
        "kokoro-v1": {
            "expected_files": ["model_uint8.onnx", "tokenizer.json", "config.json", "voices/*.bin"],
            "data_fields_to_extract": [
                "sample_rate (from config.json -> 24000Hz)",
                "phoneme_vocab (from tokenizer.json -> token_id map)",
                "voice_style_vectors (from voices/*.bin -> [510, 256] float32 matrix)"
            ]
        },
        "piper-vits": {
            "expected_files": ["model.onnx", "config.json", "phoneme_id_map"],
            "data_fields_to_extract": [
                "sample_rate (from config.json -> audio.sample_rate e.g. 22050Hz)",
                "phoneme_id_map (from config.json -> char/phoneme to token ID map)",
                "inference_scales (from config.json -> noise_scale, length_scale, noise_w)"
            ]
        },
        "audio8": {
            "expected_files": ["slow_ar_int4.onnx", "fast_ar_int4.onnx", "codec_decoder_fp16.onnx", "runtime_manifest.json"],
            "data_fields_to_extract": [
                "sample_rate (from runtime_manifest.json -> codec_sample_rate e.g. 44100Hz)",
                "semantic_begin_id (from runtime_manifest.json -> 151678)",
                "num_codebooks (from runtime_manifest.json -> 10 codebooks)",
                "codebook_size (from runtime_manifest.json -> 4096)"
            ]
        },
        "supertonic-v3": {
            "expected_files": ["text_encoder.onnx", "duration_predictor.onnx", "vector_estimator.onnx", "vocoder.onnx", "config.json", "voice_styles/*.json"],
            "data_fields_to_extract": [
                "sample_rate (from config.json -> 24000Hz)",
                "unicode_indexer (from unicode_indexer.json -> char IDs)",
                "voice_style_matrix (from voice_styles/*.json -> [1, 50, 256] float32 matrix)"
            ]
        },
        "cosyvoice": {
            "expected_files": ["flow.decoder.estimator.fp32.onnx", "hift.onnx", "campplus.onnx", "cosyvoice2.yaml", "config.json"],
            "data_fields_to_extract": [
                "sample_rate (from cosyvoice2.yaml / config.json -> 24000Hz)",
                "mel_channels (from cosyvoice2.yaml -> 80 channels)",
                "spk_embedding_dim (from campplus.onnx -> 192/512 dims)"
            ]
        },
        "chatterbox": {
            "expected_files": ["speech_encoder_q4.onnx", "language_model_q4.onnx", "conditional_decoder_q4.onnx", "tokenizer.json", "config.json"],
            "data_fields_to_extract": [
                "sample_rate (from config.json / preprocessor_config.json)",
                "tokenizer_vocab (from tokenizer.json -> BPE tokens)",
                "decoder_quantization (from generation_config.json -> Q4)"
            ]
        },
        "omnivoice": {
            "expected_files": ["audio_embeddings_encoder.onnx", "llm_decoder.onnx", "audio_heads_decoder.onnx", "omnivoice_manifest.json"],
            "data_fields_to_extract": [
                "sample_rate (from omnivoice_manifest.json)",
                "llm_context_window (from model_config.json)",
                "codebook_ids (from tokenizer.json)"
            ]
        },
        "matcha-v1": {
            "expected_files": ["matcha.onnx", "vocoder.onnx", "config.json", "tokens.txt"],
            "data_fields_to_extract": [
                "sample_rate (from config.json)",
                "token_id_map (from tokens.txt -> line to index)",
                "ode_solver_steps (from config.json)"
            ]
        }
    }

    for item in audit_results:
        fam = item.get("detected_family", "generic-onnx-tts")
        repo_id = item.get("repo_id", "")
        family_bundles[fam]["repos"].append(repo_id)

        siblings = item.get("siblings", [])
        for fn in siblings:
            rpath = fn.get("rpath", "") if isinstance(fn, dict) else str(fn)
            low = rpath.lower()

            if low.endswith(".onnx"):
                family_bundles[fam]["primary_onnx_files"].add(rpath)
            elif low.endswith("tokenizer.json") or low.endswith("tokens.txt") or low.endswith("phoneme_id_map") or "vocab" in low:
                family_bundles[fam]["vocab_and_token_files"].add(rpath)
            elif low.endswith(".json") or low.endswith(".yaml") or low.endswith(".yml"):
                family_bundles[fam]["manifest_config_files"].add(rpath)
            elif low.endswith(".bin") or low.endswith(".pt") or low.endswith(".pth") or low.endswith(".safetensors") or low.endswith(".data"):
                family_bundles[fam]["voice_styles_and_weights"].add(rpath)
            else:
                family_bundles[fam]["other_extra_files"].add(rpath)

    output_manifest = {
        "title": "Cluaiz Universal TTS Family Bundle & Metadata Manifest",
        "total_audited_repos": len(audit_results),
        "total_families": len(family_bundles),
        "families": {}
    }

    for fam, bundle in family_bundles.items():
        extractor_info = family_schema_extractors.get(fam, {
            "expected_files": ["model.onnx", "config.json"],
            "data_fields_to_extract": ["sample_rate", "tokens"]
        })

        output_manifest["families"][fam] = {
            "repo_count": len(bundle["repos"]),
            "sample_repositories": bundle["repos"][:5],
            "bundle_files_breakdown": {
                "primary_onnx_graphs": sorted(list(bundle["primary_onnx_files"])),
                "manifest_config_files": sorted(list(bundle["manifest_config_files"])),
                "vocab_and_token_files": sorted(list(bundle["vocab_and_token_files"])),
                "voice_styles_and_weights": sorted(list(bundle["voice_styles_and_weights"]))[:20],
                "other_extra_files": sorted(list(bundle["other_extra_files"]))[:10]
            },
            "schema_contract": {
                "expected_key_files": extractor_info["expected_files"],
                "runtime_data_fields_extracted": extractor_info["data_fields_to_extract"]
            }
        }

    with open(output_path, "w", encoding="utf-8") as out:
        json.dump(output_manifest, out, indent=2)

    print(f"Successfully generated Bundle Manifest JSON: {output_path}")

if __name__ == "__main__":
    main()
