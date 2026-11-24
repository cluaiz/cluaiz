| model | layout | red / runs | hint_tokens seen | prompt_tokens seen | timings.prompt_n seen |
|---|---|---|---|---|---|
| qwen3.5-0.8b-gguf-Q4_K_M | L1_text_first_hint | 5 / 5 (red) | [37] | [228] | [410] |
| qwen3.5-0.8b-gguf-Q4_K_M | L2_text_first_nohint | 5 / 5 (red) | [0] | [161] | [343] |
| qwen3.5-0.8b-gguf-Q4_K_M | L3_image_first_hint | 5 / 5 (red) | [37] | [227] | [409] |
| qwen3.5-0.8b-gguf-Q4_K_M | TOOL_image_first | 0 / 5 (tool call) | [37] | [233] | [415] |
| qwen3.5-4b-gguf-Q4_K_M | L1_text_first_hint | 5 / 5 (red) | [37] | [228] | [410] |
| qwen3.5-4b-gguf-Q4_K_M | L2_text_first_nohint | 5 / 5 (red) | [0] | [161] | [343] |
| qwen3.5-4b-gguf-Q4_K_M | L3_image_first_hint | 5 / 5 (red) | [37] | [227] | [409] |
| qwen3.5-4b-gguf-Q4_K_M | TOOL_image_first | 0 / 5 (tool call) | [37] | [233] | [415] |

| model | case | get_weather calls / runs | timings.prompt_n seen | usage.prompt_tokens seen |
|---|---|---|---|---|
| qwen3.5-0.8b-gguf-Q4_K_M | T0_text_only_tools | 5 / 5 | [933] | [218, 233, 933] |
| qwen3.5-0.8b-gguf-Q4_K_M | T3_image_first_tools | 0 / 5 | [415] | [233] |
| qwen3.5-4b-gguf-Q4_K_M | T0_text_only_tools | 5 / 5 | [933] | [933] |
| qwen3.5-4b-gguf-Q4_K_M | T3_image_first_tools | 0 / 5 | [415] | [233] |
