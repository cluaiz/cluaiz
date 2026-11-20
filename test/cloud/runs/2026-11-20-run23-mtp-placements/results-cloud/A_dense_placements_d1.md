qwen3.5-2b-mtp-gguf-Q4_K_M | greedy, thinking off, 2 rounds of MTP off then on

Full GPU | ngl -1 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.92 GB, on 2.07 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 222 | 156.35 | 167.59 | 1.07x | 1.07-1.07 | 82% | 1401.5 | 1046.1 | yes |
| chat | 154 | 156.37 | 153.13 | 0.98x | 0.98-0.98 | 63% | 1466.8 | 1106.5 | no |
| list | 181 | 156.34 | 180.80 | 1.16x | 1.16-1.16 | 98% | 1516.3 | 1137.0 | yes |
| long | 128 | 155.10 | 148.02 | 0.95x | 0.95-0.96 | 59% | 9836.7 | 8052.5 | no |

GPU + RAM split | ngl 25 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off 1.87 GB, on 2.04 GB

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 232 | 122.59 | 134.31 | 1.10x | 1.09-1.10 | 91% | 768.5 | 642.3 | no |
| chat | 142 | 123.30 | 118.15 | 0.96x | 0.96-0.96 | 65% | 812.2 | 677.8 | no |
| list | 181 | 122.82 | 139.86 | 1.14x | 1.13-1.14 | 98% | 835.7 | 694.8 | yes |
| long | 128 | 122.30 | 115.33 | 0.94x | 0.94-0.95 | 63% | 7405.3 | 6471.4 | no |

CPU only | ngl 0 | n_cpu_moe 0 | n_ctx 16384 | from disk 0.0 GB | VRAM taken: off ?, on ?

| prompt | tokens | t/s off | t/s on | gain | gain range | kept | prompt t/s off | prompt t/s on | same text |
| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------------: | ------------: | :-------: |
| code | 235 | 18.19 | 16.54 | 0.91x | 0.91-0.91 | 82% | 73.0 | 66.8 | no |
| chat | 161 | 18.27 | 14.82 | 0.81x | 0.81-0.81 | 62% | 77.0 | 70.5 | no |
| list | 181 | 18.21 | 18.05 | 0.99x | 0.99-0.99 | 98% | 76.5 | 70.0 | yes |
| long | 128 | 17.80 | 14.78 | 0.83x | 0.83-0.83 | 67% | 112.9 | 109.7 | no |
