## soak.py scenario checks

| Scenario | Pass | Fail |
|---|---:|---:|
| bad_json | 20 | 0 |
| cancel_stream | 19 | 0 |
| concurrent | 19 | 0 |
| empty | 19 | 0 |
| logprobs | 19 | 0 |
| long_input | 0 | 20 |
| max_tokens | 0 | 19 |
| multi | 19 | 0 |
| n | 19 | 0 |
| single | 19 | 0 |
| stream | 19 | 0 |
| think_on | 19 | 0 |
| unknown_model | 19 | 0 |
| **total** | **210** | **39** |

Engine starts: 10; finished event: {'event': 'finished', 'passed': 210, 'failed': 39, 'sweeps': 20, 'time': 1795565056.1470006}

## soak.py request latency (ms)

| Kind | n | p50 | p95 | max |
|---|---:|---:|---:|---:|
| bad_json | 20 | 0.6 | 0.9 | 0.98 |
| empty | 19 | 0.7 | 1.0 | 1.29 |
| logprobs | 19 | 0.9 | 1.6 | 1.66 |
| long_input | 20 | 180085.4 | 180101.0 | 180101.04 |
| max_tokens | 19 | 25262.5 | 41082.5 | 45518.71 |
| multi | 190 | 11271.1 | 19186.4 | 27577.8 |
| n | 19 | 0.8 | 1.7 | 2.07 |
| single_or_concurrent | 95 | 14623.6 | 28615.4 | 34906.56 |
| stream | 19 | 13421.2 | 21610.4 | 25445.93 |
| think_on | 19 | 8243.5 | 15079.6 | 18897.69 |
| unknown_model | 19 | 1.8 | 2.6 | 2.63 |

Client-side errors: long_input: TimeoutError('timed out') x19

## long_input replies

| # | status | ms | marker in content | content (first 100 chars) / error |
|---|---|---:|---|---|
| 1 | None | 180101.04 | False | TimeoutError('timed out') |
| 2 | None | 180101.01 | False | TimeoutError('timed out') |
| 3 | None | 180100.9 | False | TimeoutError('timed out') |
| 4 | None | 180090.45 | False | TimeoutError('timed out') |
| 5 | None | 180060.93 | False | TimeoutError('timed out') |
| 6 | 200 | 164474.17 | False | <tool_call> {"arguments":"{\"content\":\"# Project Name\\n\\nThis is a project folder.\\n\\n## Files |
| 7 | None | 180028.6 | False | TimeoutError('timed out') |
| 8 | None | 180100.89 | False | TimeoutError('timed out') |
| 9 | None | 180020.5 | False | TimeoutError('timed out') |
| 10 | None | 180063.42 | False | TimeoutError('timed out') |
| 11 | None | 180100.97 | False | TimeoutError('timed out') |
| 12 | None | 180099.0 | False | TimeoutError('timed out') |
| 13 | None | 180100.74 | False | TimeoutError('timed out') |
| 14 | None | 180080.44 | False | TimeoutError('timed out') |
| 15 | None | 180005.64 | False | TimeoutError('timed out') |
| 16 | None | 180097.5 | False | TimeoutError('timed out') |
| 17 | None | 180099.14 | False | TimeoutError('timed out') |
| 18 | None | 180004.17 | False | TimeoutError('timed out') |
| 19 | None | 180016.16 | False | TimeoutError('timed out') |
| 20 | None | 180002.08 | False | TimeoutError('timed out') |

## extra users (bg_users.py)

| Kind | pass | fail in restart window | fail other | expected answer in passes | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|
| stream | 52 | 8 | 0 | 52 | 10990.2 | 350783.6 |
| think_off | 68 | 8 | 0 | 68 | 10070.0 | 351271.7 |
| think_on | 67 | 4 | 0 | 67 | 16905.5 | 25808.3 |

## memory over time (sampler.jsonl, first sample of each 10 minutes)

| min | pid | RSS MiB | GPU proc MiB | GPU used,free,util |
|---:|---|---:|---:|---|
| 0 | 329230 | 1068 | 1452 | 1458,21157,10 |
| 10 | 329230 | 1363 | 1456 | 1462,21153,12 |
| 20 | 331933 | 1229 | 1454 | 1460,21155,7 |
| 30 | 334205 | 1265 | 1454 | 1460,21155,0 |
| 40 | 336433 | 1279 | 1456 | 1462,21153,8 |
| 50 | 336433 | 1366 | 1456 | 1462,21153,10 |
| 60 | 338597 | 1327 | 1456 | 1462,21153,8 |
| 70 | 340737 | 1200 | 1456 | 1462,21153,7 |
| 80 | 342644 | 1341 | 1456 | 1462,21153,7 |
| 90 | 344671 | 1286 | 1456 | 1462,21153,9 |
| 100 | 347022 | 1101 | 1454 | 1460,21155,79 |
| 110 | 347022 | 1323 | 1458 | 1464,21151,7 |
| 120 | 349123 | 1240 | 1454 | 1460,21155,7 |

### per server process

| pid | samples | minutes alive | RSS first MiB | RSS last MiB | RSS max MiB | GPU first | GPU last | GPU max |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 329230 | 74 | 12.2 | 1068 | 1364 | 1364 | 1452 | 1456 | 1456 |
| 331933 | 75 | 12.4 | 906 | 1278 | 1278 | 1452 | 1454 | 1454 |
| 334205 | 75 | 12.4 | 967 | 1352 | 1378 | 1452 | 1458 | 1458 |
| 336433 | 75 | 12.4 | 420 | 1366 | 1366 | 186 | 1456 | 1456 |
| 338597 | 74 | 12.3 | 977 | 1327 | 1348 | 1452 | 1456 | 1456 |
| 340737 | 66 | 10.9 | 880 | 1279 | 1288 | 1438 | 1458 | 1458 |
| 342644 | 70 | 11.6 | 1024 | 1351 | 1351 | 1452 | 1456 | 1456 |
| 344671 | 81 | 13.4 | 957 | 1350 | 1350 | 1452 | 1456 | 1456 |
| 347022 | 74 | 12.2 | 905 | 1323 | 1331 | 1442 | 1458 | 1458 |
| 349123 | 58 | 9.6 | 409 | 1252 | 1252 | 20 | 1454 | 1454 |
