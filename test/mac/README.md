# Cluaiz Mac test (Apple Silicon)

Ye test aapke MacBook par Cluaiz chalata hai aur use **llama.cpp** ke saath, same model aur same setting
par, compare karta hai. Lagbhag **1 ghanta** lagta hai. Aapko sirf **ek line** chalani hai, baaki sab
apne aap hota hai. Aakhir mein Desktop par **ek file** banti hai: bas wahi wapas bhejni hai.

> Kuch install ya kharab nahi hota: sab kuch ek alag folder `~/cluaiz-mactest` mein jaata hai, jo baad mein
> ek command se hat jaata hai.

## Shuru karne se pehle

| | |
|---|---|
| 💻 Mac | Apple Silicon (M1, M2, M3, M4, M5). Intel Mac par ye test nahi chalta. |
| 🔌 Charger | **Laga ke rakhiye.** Battery par Mac dheema chalta hai aur test galat aata hai. |
| 📶 Internet | Achha Wi-Fi. 10 se 40 GB download hota hai (RAM ke hisaab se, neeche dekhiye). |
| 💾 Khali jagah | Kam se kam **40 GB** free. |
| ⏳ Dhyan | Test ke dauraan Mac band ya sleep mat kijiye, lid khuli rakhiye, bhaari kaam mat chalaiye. |

## Steps

**1. Terminal kholiye.** `Cmd + Space` dabaiye, `Terminal` likhiye, `Enter`.

**2. Ye poori line copy karke Terminal mein paste kijiye aur `Enter` dabaiye:**

```bash
bash -c "$(curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/mac/mac_test.sh)"
```

**3. Agar ek window aaye "Command Line Tools install karein?"**, to **Install** dabaiye. 5 se 10 minute
mein poora ho jaata hai. Uske baad **step 2 wali line dobara** chalaiye. (Ye window sirf pehli baar aati hai.)

**4. Ab bas intezaar kijiye.** Screen par aisa likha aata rahega:

```
== [0 min] machine
== [2 min] install (the release's one command)
== [3 min] downloads
== [21 min] test unsloth/Qwen3-4B-GGUF:Q4_K_M (auto,gpu,split,cpu)
...
== [55 min] done in 55 min
== results: /Users/aap/Desktop/cluaiz-mac-results-20261029-1130.tar.gz (send this file)
```

Beech mein kuch mat dabaiye. Lambi lines aur numbers aana normal hai.

**5. Khatam hone par** Desktop par ek file milegi: **`cluaiz-mac-results-….tar.gz`**.
📎 **Wahi file WhatsApp, email ya Google Drive se wapas bhej dijiye.** Bas itna hi.

## Test kya karta hai

1. Mac ki jaankari likhta hai: chip, RAM, macOS, SSD ki speed, battery aur garmi.
2. Cluaiz ko uske official ek-command installer se install karta hai.
3. Mac ki RAM dekh kar **Q4 model** chunta hai aur download karta hai:

   | Mac ki RAM | Model | Download |
   |---|---|---|
   | 16 GB | Qwen3 4B, Gemma 3 12B | ~9 GB |
   | 24 GB | upar wale + Mistral Small 24B | ~22 GB |
   | 32 GB ya zyada | upar wale + Gemma 4 31B | ~40 GB |

4. Saath mein **llama.cpp** (Apple ke Metal GPU ke saath) banata hai, usi version se jo Cluaiz ke andar hai.
5. Har model ko **4 tarah** se chalata hai, pehle Cluaiz par, phir llama.cpp par:

   | Mode | Matlab |
   |---|---|
   | auto | Cluaiz khud tay karta hai model kahan chalana hai |
   | gpu | poora model GPU par |
   | split | aadha GPU par, aadha CPU par |
   | cpu | sirf CPU |

6. Saare logs aur nateeje ek file mein pack karke Desktop par rakhta hai.

## Error aaye to

- Terminal ka **screenshot** le kar bhej dijiye.
- Lal rang mein `ERROR` dikhe, ya **15 se 20 minute** tak screen par kuch naya na aaye, tab bhi bata dijiye.
- Beech mein rokna ho to `Ctrl + C` dabaiye. Tab bhi jitna test hua, uski file Desktop par ban jaati hai:
  wo bhej dijiye.
- Dobara chalana ho to step 2 wali line phir se chalaiye. Jo model download ho chuke hain, wo dobara
  download nahi hote.

## Aur zyada test (optional)

16 GB wale Mac par bada model (Gemma 4 31B, 17 GB) bhi chalana ho, jo RAM se bada hai aur SSD se padha
jaata hai. Isme 20 se 30 minute zyada lagte hain:

```bash
BIG=1 bash -c "$(curl -fsSL https://raw.githubusercontent.com/cluaiz/cluaiz/main/test/mac/mac_test.sh)"
```

## Baad mein saaf karna (zaroori nahi)

```bash
rm -rf ~/cluaiz-mactest
```

Installer `~/.zshrc` mein `# Cluaiz` naam ki ek PATH line bhi jodta hai. Wo bhi hatani ho to `~/.zshrc` ko
TextEdit mein kholiye aur `# Cluaiz` wali line aur uske neeche wali `export PATH=...` line delete kar dijiye.

## Developers ke liye

- Script: [`mac_test.sh`](mac_test.sh). Comparison: [`test/engine_regression/run.py`](../engine_regression/run.py).
- `RUNS=2` har model aur mode par requests ki ginti badalta hai (default 3, pehli cold run hoti hai).
- Archive mein: `machine.txt`, `install.log`, `installed_files.txt`, `pulls.log`, `pull_*.log`,
  `llama_bench_build.log`, `server.log`, `planner.log` (planner ne kya chuna), `run_*.log`,
  `RESULTS_*.md` (Cluaiz vs llama.cpp, har setting aur exact llama-bench command ke saath), `loaded_libs.txt`.
- Server alag folder (`CLUAIZ_HOME=~/cluaiz-mactest/home`) aur port 8011 par chalta hai, kisi aur Cluaiz
  install ko nahi chhoota.

Shukriya! 🙏
