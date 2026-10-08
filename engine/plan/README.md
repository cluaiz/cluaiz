# 🎯 Cluaiz Execution Planner (`engine/plan`)

Pure mathematical negotiation and placement engine for Cluaiz.

## 📐 Architecture

```mermaid
graph TD
    A[ModelGeometry] --> D[Planner::create_plan]
    B[SystemInfo] --> D
    C[ActiveUserSettings] --> D
    D -->|Pure Function Evaluation| E[ExecutionPlan]
    E -->|Candidate Evaluation| F[GPU-Only / Hybrid / CPU-Only]
```

## 📦 Components

| Module | Responsibility |
|---|---|
| [`types.rs`](src/types.rs) | Typed data structures for Candidates, Tiers, and ExecutionPlans. |
| [`planner.rs`](src/planner.rs) | Pure mathematical calculation of memory budgets and candidate selection. |
