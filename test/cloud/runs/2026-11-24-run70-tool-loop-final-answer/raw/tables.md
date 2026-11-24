| Set | n | Real answer | Marker in answer | Ended at step/repeat limit | Final no-tools turn | Engine tool steps (sum / max) | Median s | Total s |
|---|---|---|---|---|---|---|---|---|
| before_marker | 10 | 0 | 0 | 10 | 0 | 50 / 5 | 203.4 | 2062.5 |
| before2_marker | 10 | 0 | 0 | 8 | 0 | 155 / 99 | 144.9 | 1826.0 |
| after_marker | 10 | 8 | 6 | 8 | 8 | 155 / 99 | 146.7 | 1913.7 |
| before_maxsteps2 | 5 | 0 | 0 | 5 | 0 | 5 / 1 | 5.8 | 52.4 |
| after_maxsteps2 | 5 | 5 | 3 | 5 | 5 | 5 / 1 | 7.1 | 44.6 |
| before_stream | 3 | 0 | 0 | 3 | 0 | 18 / 6 | 140.9 | 433.6 |
| after_stream | 3 | 3 | 2 | 3 | 3 | 16 / 6 | 139.4 | 430.4 |
| before_client_tools | 3 | 0 | 0 | 0 | 0 | 0 / 0 | 2.9 | 10.0 |
| after_client_tools | 3 | 0 | 0 | 0 | 0 | 0 / 0 | 3.0 | 10.3 |

Per request, before2 vs after (same binary order, same prompts):

| i | before2 steps | before2 limit | before2 s | after steps | after limit | final turn | after s | after answer (first 60 chars) |
|---|---|---|---|---|---|---|---|---|
| 0 | 6 | True | 151.51 | 6 | True | True | 156.04 | Based on the tool results and the project context, here is t |
| 1 | 6 | True | 146.9 | 6 | True | True | 140.67 | The marker is **5028**.  **Analysis:** The tool call `find_t |
| 2 | 6 | True | 144.39 | 6 | True | True | 139.55 | The marker is **5139**.  **Context:** The user is asking for |
| 3 | 8 | False | 154.64 | 8 | False | False | 147.7 | <tool_call> {"arguments":"{\"content\":\"# Project Documenta |
| 4 | 5 | True | 138.56 | 5 | True | True | 138.24 | The marker is **5361**.  **Context:** This is a **filesystem |
| 5 | 6 | True | 144.79 | 6 | True | True | 145.64 | The marker is **5472**.  **Context:** The user is asking for |
| 6 | 5 | True | 141.56 | 5 | True | True | 140.77 | The marker is **5583**.  **Context:** The user is asking for |
| 7 | 99 | True | 521.55 | 99 | True | True | 599.2 | The marker is **filesystem**.  **Reason:** The tool `filesys |
| 8 | 6 | True | 137.17 | 6 | True | True | 152.22 | The marker is **5805**.  **Analysis:** The tool call `find_t |
| 9 | 8 | False | 144.96 | 8 | False | False | 153.68 | <tool_call> {"arguments":"{\"content\":\"# Project Documenta |
