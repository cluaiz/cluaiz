"""
    cluaiz DATA FACTORY v1.0
    Role: Synthetic Scenario Generator for Atma (278M)
    Uses: Gemini 1.5 Pro via Google AI SDK
"""

import os
import json
import random
from typing import List, Dict

# ── SCHEMATIC CONSTANTS ──────────────────────────────────────────────────────
# Based on our research in docs/master_landscape.md & NEURAL_PATHWAYS.yaml
PILLARS = ["Workforce", "Cognition", "Essence"]
INTENTS = ["Search", "Summarize", "Execute", "Route", "Analyze"]

SYSTEM_PROMPT = """
You are the Lead Data Architect for the cluaiz Neural OS.
Your task is to generate one "Instruction-Response" training pair for the Atma Supervisor (278M).

Atma's primary job is INTENT ROUTING and CONTEXT COMPRESSION. 
It converts raw user requests into structural neural paths.

3-PIVOT PINS (Target Branches):
1. WORKFORCE: Agents, Skills, Roles (e.g., 'SummaryAgent', 'WhatsAppSkill').
2. COGNITION: Documents, Chunks, Web Pages (e.g., 'Manual.pdf', 'Pricing_Page').
3. ESSENCE: Goals, Mission, Departments (e.g., 'Increase Sales', 'HR Dept').

OUTPUT FORMAT (STRICT JSON ONLY):
{
    "instruction": "A realistic user prompt for a Windows-based AI OS.",
    "thought_process": "Atma's step-by-step mathematical reasoning.",
    "prediction": {
        "intent": "Search|Summarize|Execute|Route|Analyze",
        "primary_pillar": "Workforce|Cognition|Essence",
        "target_neurons": ["NeuronA", "NeuronB"],
        "mces_score": "Confidence score 0.0 - 1.0"
    },
    "neural_path": "Neuron1 -> [:LINK] -> Neuron2"
}
"""

def generate_scenario(count: int = 1):
    """
    Simulates the Data Factory loop.
    In reality, this calls the Gemini API to distill 10,000 samples.
    """
    scenarios = []
    
    # Example Sample based on "Sovereign OS" context
    sample = {
        "instruction": "Summarize the VRAM Guardian policy from our technical manual.",
        "thought_process": "User is asking for cognitive processing of a document. Requires fetching manual.pdf from Cognition and routing to SummaryAgent in Workforce.",
        "prediction": {
            "intent": "Summarize",
            "primary_pillar": "Cognition",
            "target_neurons": ["Technical_Manual_Doc", "VRAM_Guardian_Chunk", "SummarySkill"],
            "mces_score": 0.98
        },
        "neural_path": "Technical_Manual_Doc -> [:HAS_CONTENT] -> VRAM_Guardian_Chunk -> [:PROCESSED_BY] -> SummarySkill"
    }
    
    scenarios.append(sample)
    
    filename = f"data/synthetic_train_{count}.jsonl"
    with open(filename, 'w') as f:
        for s in scenarios:
            f.write(json.dumps(s) + "\n")
            
    print(f" [DataFactory] Successfully generated {len(scenarios)} samples in {filename}")

if __name__ == "__main__":
    # Create data dir if not exists
    if not os.path.exists("data"):
        os.makedirs("data")
    generate_scenario(10)
