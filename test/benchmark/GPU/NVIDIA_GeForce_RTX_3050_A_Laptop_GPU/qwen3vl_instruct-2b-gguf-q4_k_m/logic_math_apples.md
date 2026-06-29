# 🚀 cluaiz Hardware Benchmark Report

## 🤖 Model: qwen3vl_instruct:2b:gguf:q4_k_m
### 🛠️ Hardware & Environment
- **Compute Node**: "C:\\Users\\Aryan\\my\\cluaiz-workspace\\cluaiz\\test\\benchmark\\GPU\\NVIDIA_GeForce_RTX_3050_A_Laptop_GPU"
- **Approx. Parameters**: ~0B
- **Context Window**: 4096 (Dynamic Limit)
- **VRAM Used**: 2.00 GB

## 📊 Benchmark Results

### 🧪 [logic_math_apples] - Basic Logic (Easy)
**Prompt**: `If I have 5 apples, I eat 2, and then I buy 3 more, how many apples do I have left? Explain your reasoning.`

#### 🧠 Thinking Mode: OFF
- **Speed**: 43.31 TPS
- **TTFT**: 0.04 s
- **Tokens**: 129
- **Time**: 2.98 s

**Output:**
Let's go step by step:

1. You start with **5 apples**.
2. You eat **2 apples**, so you now have:  
   5 - 2 = **3 apples**.
3. Then you buy **3 more apples**, so you now have:  
   3 + 3 = **6 apples**.

So, after eating 2 and buying 3 more, you have **6 apples** in total.

✅ **Answer: 6 apples.**

The reasoning is simple subtraction and addition: start with 5, subtract 2 (eaten), then add 3 (bought).

#### 🧠 Thinking Mode: ON
- **Speed**: 44.14 TPS
- **TTFT**: 0.04 s
- **Tokens**: 122
- **Time**: 2.76 s

**Output:**
Let's go step by step:

1. You start with **5 apples**.
2. You **eat 2 apples**, so you now have:  
   $ 5 - 2 = 3 $ apples left.
3. Then, you **buy 3 more apples**, so you now have:  
   $ 3 + 3 = 6 $ apples.

### Final Answer:
**You have 6 apples**.

✅ Reasoning: You started with 5, ate 2 (left with 3), and then bought 3 more. So the total is 6.

