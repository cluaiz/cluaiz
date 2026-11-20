qwen3.5-2b-mtp-gguf-IQ4_XS | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.85 GB, on 2.04 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 211 | 160.52 | 186.22 | 1.16x | 1.16-1.16 | 68% | 1456.7 | 1028.0 | no |
| chat | 175 | 160.18 | 127.40 | 0.80x | 0.79-0.80 | 32% | 1521.2 | 1095.6 | no |
| list | 180 | 160.32 | 222.64 | 1.39x | 1.38-1.40 | 92% | 1610.1 | 1117.2 | yes |
| long | 128 | 158.86 | 143.04 | 0.90x | 0.90-0.90 | 44% | 10694.2 | 8201.7 | no |
