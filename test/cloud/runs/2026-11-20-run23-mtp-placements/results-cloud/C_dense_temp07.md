qwen3.5-2b-mtp-gguf-Q4_K_M | temperature 0.699999988079071, thinking off, 3 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.92 GB, on 2.11 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 226 | 155.18 | 165.06 | 1.06x | 0.92-1.14 | 58% | 1377.5 | 1022.2 | no |
| chat | 160 | 155.43 | 123.20 | 0.79x | 0.74-0.82 | 31% | 1464.1 | 1089.0 | no |
| list | 181 | 155.45 | 218.64 | 1.41x | 1.40-1.41 | 88% | 1494.9 | 1120.4 | no |
| long | 128 | 154.23 | 131.53 | 0.85x | 0.82-0.87 | 37% | 9473.1 | 7914.8 | no |
