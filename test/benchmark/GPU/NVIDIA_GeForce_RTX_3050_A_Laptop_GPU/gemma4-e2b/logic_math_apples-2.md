# 🚀 cluaiz Hardware Benchmark Report

## 🤖 Model: gemma4:e2b
### 🛠️ Hardware & Environment
- **Compute Node**: "C:\\Users\\Aryan\\my\\cluaiz-workspace\\cluaiz\\test\\benchmark\\GPU\\NVIDIA_GeForce_RTX_3050_A_Laptop_GPU"
- **Approx. Parameters**: ~0B
- **Context Window**: 56832 (Dynamic Limit)
- **VRAM Used**: 2.00 GB

## 📊 Benchmark Results

### 🧪 [logic_math_apples] - Basic Logic (Easy)
**Prompt**: `If I have 5 apples, I eat 2, and then I buy 3 more, how many apples do I have left? Explain your reasoning.`

#### 🧠 Thinking Mode: OFF
- **Speed**: 27.75 TPS
- **TTFT**: 0.06 s
- **Tokens**: 145
- **Time**: 5.22 s

**Output:**
Here is the step-by-turn explanation:

1.  **Starting amount:** You begin with 5 apples.
2.  **Eating apples:** You eat 2 apples, so you calculate $5 - 2 = 3$ apples remaining.
3.  **Buying more:** You buy 3 more apples, so you calculate $3 + 3 = 6$ apples total.

**Answer:** You have 6 apples left.

***

**Reasoning Explanation:**
The process involves three steps: subtraction (eating), followed by addition (buying). Starting with 5, subtracting 2 leaves 3. Then, adding 3 to that amount results in 6.

#### 🧠 Thinking Mode: ON
- **Speed**: 27.24 TPS
- **TTFT**: 0.08 s
- **Tokens**: 262
- **Time**: 9.62 s

**Output:**
Thinking Process:
1.  **Analyze the initial state:** The user starts with 5 apples.
2.  **Analyze the first action (eating):** The user eats 2 apples. Calculation: $5 - 2 = 3$ apples remaining.
3.  **Analyze the second action (buying):** The user buys 3 more apples. Calculation: $3 + 3 = 6$ apples total.
4.  **Formulate the explanation:** Detail each step clearly to show how the final number was reached.

**Calculation Check:**
Start: 5
Eat 2: $5 - 2 = 3$
Buy 3: $3 + 3 = 6$

**Final Answer Formulation:** The user has 6 apples left.<channel|>You have **6** apples left.

Here is the reasoning:

1.  **Start:** You begin with 5 apples.
2.  **Eating:** You eat 2 apples, so you subtract those from your total: $5 - 2 = 3$ apples remaining.
3.  **Buying:** You then buy 3 more apples, so you add them to what you have: $3 + 3 = 6$ apples.

