# Ollama generate answers a different question

Status: open, P2 answer correctness on this build. Root cause unknown.

Repro: run `suite.py` with `--only ollama_generate` on an isolated home containing `qwen3.5-0.8b-gguf-Q4_K_M`. The request is `POST /api/generate`, `prompt: "The capital of France is"`, `stream: false`, `think: false`, `temperature: 0`, `num_predict: 16`.

Expected: response mentions Paris. Actual: HTTP 200, `response: "**Cluaiz** is a Rust-based AI engine built on llama.cpp and"`, `prompt_eval_count: 3725`. Four attempts gave the same response: the full matrix, a clean Engine working directory, a fresh configuration home, and a second existing debug executable. See `../raw/api_checks.jsonl` and the three `api_checks_*` control files. Run 49 had answered Paris with 881 prompt tokens, but used a different binary and home. The adapter maps the prompt in `api/src/handlers/ollama.rs`; the source of the additional prompt content is not established.

No Engine source was changed. A targeted trace of the final constructed chat messages on both builds would separate adapter, prompt construction and model behavior.
