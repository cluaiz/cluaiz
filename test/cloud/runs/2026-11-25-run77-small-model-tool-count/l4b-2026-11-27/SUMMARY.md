# Run 77, extra cells on cluaiz-l4b (2026-11-27)

Same 40-question tool matrix as Run 77 (`../run_matrix.py`). Counts are "correct" lines in each
`raw/*.console.txt` (simple count; `../analyze.py` also checks status and wrong_no_tool).

| Cell | Correct |
|---|---|
| qwen3.5-0.8B thinking off, set B | 32/40 |
| qwen3.5-0.8B thinking off, set C | 32/40 |
| qwen3.5-0.8B thinking on, set A | 37/40 |
| qwen3.5-0.8B thinking on, set B | 34/40 |
| qwen3.5-0.8B thinking on, set C | 35/40 |
| qwen3.5-2B thinking off, set A | 37/40 |
| qwen3.5-2B thinking off, set B | incomplete (9/9 rows written, run stopped) |

Not run: qwen3.5-0.8B off-A on this VM (exists in the main folder), qwen3.5-2B off-C and all
qwen3.5-2B thinking-on cells. Rerun with `drv.sh`.
