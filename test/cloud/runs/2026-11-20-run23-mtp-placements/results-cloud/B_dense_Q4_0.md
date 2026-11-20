qwen3.5-2b-mtp-gguf-Q4_0 | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.86 GB, on 2.04 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 196 | 165.01 | 188.98 | 1.15x | 1.14-1.15 | 67% | 1493.5 | 1059.4 | yes |
| chat | 206 | 164.72 | 138.29 | 0.84x | 0.84-0.84 | 39% | 1585.4 | 1119.1 | no |
| list | 181 | 164.57 | 222.06 | 1.35x | 1.35-1.35 | 88% | 1601.1 | 1164.8 | yes |
| long | 128 | 163.53 | 149.84 | 0.92x | 0.91-0.92 | 47% | 10179.7 | 8574.6 | yes |
