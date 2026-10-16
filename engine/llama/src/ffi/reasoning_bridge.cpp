// C ABI only: all reasoning state and forcing belong to llama.cpp's sampler.
#include "reasoning-budget.h"
#include <memory>

extern "C" llama_sampler * cluaiz_reasoning_wrap(
        llama_sampler * chain, const llama_vocab * vocab, const char * start,
        const char * end, const char * forced, int32_t budget, bool inside,
        llama_sampler ** reasoning) noexcept {
    *reasoning = nullptr;
    if (!end[0]) { return chain; }
    try {
        const auto start_tokens = common_tokenize(vocab, start, false, true);
        const auto end_tokens = common_tokenize(vocab, end, false, true);
        const auto forced_tokens = common_tokenize(vocab, forced, false, true);
        if (end_tokens.empty() || forced_tokens.empty()) { return nullptr; }
        using owner = std::unique_ptr<llama_sampler, decltype(&llama_sampler_free)>;
        owner gate(common_reasoning_budget_init(vocab, {start_tokens}, {end_tokens},
            forced_tokens, budget, inside ? REASONING_BUDGET_COUNTING : REASONING_BUDGET_IDLE),
            llama_sampler_free);
        auto params = llama_sampler_chain_default_params();
        params.no_perf = true;
        owner root(llama_sampler_chain_init(params), llama_sampler_free);
        if (!gate || !root) { return nullptr; }
        auto * borrowed = gate.get();
        llama_sampler_chain_add(root.get(), gate.get());
        gate.release();
        // The old chain already accepted prompt history. The new gate starts at
        // the rendered prompt's state and sees only generated tokens.
        llama_sampler_chain_add(root.get(), chain);
        *reasoning = borrowed;
        return root.release();
    } catch (...) {
        // On failure before attachment, the caller still owns the original chain.
        return nullptr;
    }
}

extern "C" bool cluaiz_reasoning_force(llama_sampler * sampler) noexcept {
    return common_reasoning_budget_force(sampler);
}

extern "C" bool cluaiz_reasoning_is_forcing(const llama_sampler * sampler) noexcept {
    return sampler && common_reasoning_budget_get_state(sampler) == REASONING_BUDGET_FORCING;
}
