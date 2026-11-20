qwen3.5-2b-mtp-gguf-Q8_0 | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 2.59 GB, on 2.79 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 204 | 106.71 | 139.71 | 1.31x | 1.30-1.31 | 65% | 1118.6 | 803.0 | yes |
| chat | 114 | 106.61 | 90.03 | 0.84x | 0.84-0.85 | 28% | 1184.4 | 846.9 | yes |
| list | 180 | 106.61 | 170.10 | 1.60x | 1.59-1.60 | 92% | 1233.4 | 872.6 | no |
| long | 128 | 106.09 | 102.17 | 0.96x | 0.96-0.97 | 38% | 9510.7 | 7792.5 | no |
