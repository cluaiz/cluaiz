# cluaiz-context

Pure context planning for the Engine. It gets segments with real token counts and returns which
ones go into the next prompt, with a reason for each. It reads no files, calls no model, holds no
KV cache and has no database. The API (`api/src/handlers/chat/`) and the runtime do the I/O
around it.

Only `serde` is a dependency, so `cargo test --release -p cluaiz-context` builds in about 15 s
and never rebuilds llama.cpp.

## Planner (`src/planner.rs`)

```text
room      = n_ctx − reserve_answer − reserve_thinking − margin
available = room − protected
```

| Order | Segments | Rule |
| --- | --- | --- |
| 0 | `protected`: system prompt, tool protocol, current message, unfinished tool step | must fit, else `PlanError::ProtectedDoesNotFit` |
| 1 | `history` of the newest `recent_turns_min` turns | newest first, whole turns |
| 2 | `tools_in_use` | relevance, then recency |
| 3 | `evidence` | at most `evidence_cap_pct` of `available` |
| 4 | `suggestions` | at most `suggestions_cap_pct` of `available` |
| 5 | older `history` | relevance, then recency, whole turns |
| 6 | `evictable` | last; leaving it out does not ask for compaction |

Segments with the same `group` (a tool call and its result) are kept or left out together. The
same input gives the same plan, whatever the input order. `needs_compaction` is set when the
plan uses `compact_at_pct` of `available` or more, or leaves out anything that is not evictable.

## Compaction (`src/compaction.rs`)

When a chat fills its room, `compact` decides what to give back, cheapest loss first: tool hints
of turns outside the newest `recent_turns_min`, then results of older tool steps (the newest
`keep_recent_steps` stay whole) as placeholders. It returns nothing unless at least
`clear_at_least` tokens are freed, because every compaction re-processes the prompt from the
first change. The API applies the decision to the chat's ledger
(`api/src/handlers/chat/context_plan.rs`); a cleared tool load also unbinds that tool.

## Policy (`src/policy.rs`)

| Field | Default |
| --- | --- |
| `recent_turns_min` | 2 |
| `evidence_cap_pct` | 15 |
| `suggestions_cap_pct` | 5 |
| `margin_pct` | 2 |
| `compact_at_pct` | 85 |

## Tests

Unit tests for the budget, caps, groups, errors and compaction, and property tests over 2,000 random inputs:
never over `available`, never drops a protected segment, keeps the recent turns when they fit,
same plan for the same input in any order.
