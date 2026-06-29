# 🚀 cluaiz Hardware Benchmark Report

## 🤖 Model: gemma4:e2b
### 🛠️ Hardware & Environment
- **Compute Node**: "C:\\Users\\Aryan\\my\\cluaiz-workspace\\cluaiz\\test\\benchmark\\GPU\\NVIDIA_GeForce_RTX_3050_A_Laptop_GPU"
- **Approx. Parameters**: ~0B
- **Context Window**: 4096 (Hard Limit)

## 📊 Benchmark Results

### 🧪 [logic_math_apples] - Basic Logic (Easy)
**Prompt**: `If I have 5 apples, I eat 2, and then I buy 3 more, how many apples do I have left? Explain your reasoning.`

#### 🧠 Thinking Mode: OFF
- **Speed**: 27.15 TPS
- **TTFT**: 0.06 s
- **Tokens**: 80
- **Time**: 2.95 s

**Output:**
```text
**Answer:** You have **6** apples left.

**Reasoning:**

1. **Starting amount:** You begin with 5 apples.
2. **Eating apples:** You eat 2 apples ($5 - 2 = 3$ apples remaining).
3. **Buying more apples:** You buy 3 more apples ($3 + 3 = 6$ apples).<turn|>
```

#### 🧠 Thinking Mode: ON
- **Speed**: 27.07 TPS
- **TTFT**: 0.07 s
- **Tokens**: 98
- **Time**: 3.62 s

**Output:**
```text
Here is the step-by-step reasoning:

1. **Starting amount:** You begin with 5 apples.
2. **Eating apples:** You eat 2 apples.
   * Calculation: $5 - 2 = 3$ apples remaining.
3. **Buying more apples:** You buy 3 more apples.
   * Calculation: $3 + 3 = 6$ apples.

**Answer:** You have **6** apples left.<turn|>
```

