# 🚀 cluaiz Hardware Benchmark Report

## 🤖 Model: gemma4:e4b:gguf:q4_k_m
### 🛠️ Hardware & Environment
- **Compute Node**: "C:\\Users\\Aryan\\my\\cluaiz-workspace\\cluaiz\\test\\benchmark\\GPU\\NVIDIA_GeForce_RTX_3050_A_Laptop_GPU"
- **Approx. Parameters**: ~0B
- **Context Window**: 4096 (Hard Limit)

## 📊 Benchmark Results

### 🧪 [logic_math_apples] - Basic Logic (Easy)
**Prompt**: `If I have 5 apples, I eat 2, and then I buy 3 more, how many apples do I have left? Explain your reasoning.`

#### 🧠 Thinking Mode: OFF
- **Speed**: 20.54 TPS
- **TTFT**: 0.12 s
- **Tokens**: 191
- **Time**: 9.30 s

**Output:**
```text
***

## Calculation

You will have **6** apples left.

## Step-by-Step Reasoning

This problem requires tracking the changes in the number of apples through a series of actions: starting, subtracting, and adding.

1. **Starting Amount:** You begin with 5 apples.
   * *Current total: 5*

2. **Eating Apples (Subtraction):** You eat 2 apples, so you subtract 2 from your starting amount.
   * $5 - 2 = 3$ apples.
   * *Current total: 3*

3. **Buying Apples (Addition):** You then buy 3 more apples, so you add 3 to your current total.
   * $3 + 3 = 6$ apples.
   * *Final total: 6*

**In summary:** $5 - 2 + 3 = 6$<turn|>
```

#### 🧠 Thinking Mode: ON
- **Speed**: 19.42 TPS
- **TTFT**: 0.08 s
- **Tokens**: 124
- **Time**: 6.39 s

**Output:**
```text
Here is the step-by-step reasoning to solve the problem:

**1. Start with the initial amount:**
You begin with **5** apples.

**2. Account for what you ate (Subtraction):**
You eat 2 apples.
$5 - 2 = 3$ apples.
*After eating, you have 3 apples left.*

**3. Account for what you bought (Addition):**
You buy 3 more apples.
$3 + 3 = 6$ apples.

**Answer:**
You have **6** apples left.<turn|>
```

