// C ABI over llama.cpp's own speculative decoding (common/speculative.h), MTP only: the drafter
// that reads the model's own multi-token-prediction (NextN) layers. The context and the drafter
// are made the way llama-server makes them (`common_speculative_init_result`,
// `common_speculative_init`); the target decode and the verification stay in Rust.
#include "common.h"
#include "llama.h"
#include "speculative.h"

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <exception>
#include <vector>

struct cluaiz_mtp {
    llama_context      * ctx_dft = nullptr;
    common_speculative * spec    = nullptr;
    common_params        params;
    llama_tokens         draft;
};

static void cluaiz_mtp_error(char * err, size_t err_len, const char * text) {
    if (err != nullptr && err_len > 0) {
        snprintf(err, err_len, "%s", text);
    }
}

// The MTP draft context over the target's NextN layers (the model was loaded with `load_mtp`), and
// llama.cpp's MTP drafter on it. `n_max`: the most tokens drafted per step (llama-server: 3).
// Returns null with a reason in `err` when the model has no NextN layers or a context cannot be made.
extern "C" cluaiz_mtp * cluaiz_mtp_init(
        llama_model   * model_tgt,
        llama_context * ctx_tgt,
        int32_t         n_max,
        char          * err,
        size_t          err_len) {
    if (llama_model_n_layer_nextn(model_tgt) <= 0) {
        cluaiz_mtp_error(err, err_len, "the model has no multi-token-prediction layers");
        return nullptr;
    }
    auto * mtp = new cluaiz_mtp();
    try {
        mtp->params.speculative.types       = { COMMON_SPECULATIVE_TYPE_DRAFT_MTP };
        mtp->params.speculative.draft.n_max = n_max;

        // common_speculative_init_result + common_base_params_to_speculative: an MTP context type,
        // the target's context size, no recurrent snapshots of its own, one output
        llama_context_params cparams = llama_context_default_params();
        cparams.ctx_type        = LLAMA_CONTEXT_TYPE_MTP;
        cparams.n_ctx           = llama_n_ctx(ctx_tgt);
        cparams.n_batch         = llama_n_batch(ctx_tgt);
        cparams.n_ubatch        = llama_n_ubatch(ctx_tgt);
        cparams.n_seq_max       = 1;
        cparams.n_rs_seq        = 0;
        // One drafted token is read per step (common_base_params_to_speculative): without this the
        // context keeps logits for a whole batch (512 x the vocabulary, 0.5 GB for Qwen3.5's 248k)
        cparams.n_outputs_max         = 1;
        cparams.n_outputs_max_per_seq = 1;
        cparams.n_threads       = llama_n_threads(ctx_tgt);
        cparams.n_threads_batch = llama_n_threads_batch(ctx_tgt);
        cparams.ctx_other       = ctx_tgt;

        mtp->ctx_dft = llama_init_from_model(model_tgt, cparams);
        if (mtp->ctx_dft == nullptr) {
            cluaiz_mtp_error(err, err_len, "llama.cpp could not create the MTP draft context");
            delete mtp;
            return nullptr;
        }

        mtp->params.speculative.draft.ctx_tgt = ctx_tgt;
        mtp->params.speculative.draft.ctx_dft = mtp->ctx_dft;
        mtp->spec = common_speculative_init(mtp->params.speculative, 1);
        if (mtp->spec == nullptr) {
            cluaiz_mtp_error(err, err_len, "llama.cpp could not start the MTP drafter");
            llama_free(mtp->ctx_dft);
            delete mtp;
            return nullptr;
        }
        return mtp;
    } catch (const std::exception & e) {
        cluaiz_mtp_error(err, err_len, e.what());
        if (mtp->spec != nullptr) {
            common_speculative_free(mtp->spec);
        }
        if (mtp->ctx_dft != nullptr) {
            llama_free(mtp->ctx_dft);
        }
        delete mtp;
        return nullptr;
    }
}

extern "C" void cluaiz_mtp_free(cluaiz_mtp * mtp) {
    if (mtp == nullptr) {
        return;
    }
    if (mtp->spec != nullptr) {
        common_speculative_free(mtp->spec);
    }
    if (mtp->ctx_dft != nullptr) {
        llama_free(mtp->ctx_dft);
    }
    delete mtp;
}

// The most tokens one draft can hold (after llama.cpp's own limits for the model)
extern "C" int32_t cluaiz_mtp_n_max(const cluaiz_mtp * mtp) {
    return mtp == nullptr ? 0 : common_speculative_n_max(mtp->spec);
}

// Tells the drafter about tokens the target just decoded (sequence 0): every target decode,
// prompt chunks included, as llama-server does after each `llama_decode`. `output[i]` is the
// batch's logits flag of token i.
extern "C" bool cluaiz_mtp_process(
        cluaiz_mtp        * mtp,
        const llama_token * tokens,
        const llama_pos   * pos,
        const int8_t      * output,
        int32_t             n_tokens) {
    if (mtp == nullptr || n_tokens <= 0) {
        return true;
    }
    try {
        common_batch batch(mtp->ctx_dft);
        for (int32_t i = 0; i < n_tokens; i++) {
            batch.add(tokens[i], pos[i], 0, output[i] != 0);
        }
        return common_speculative_process(mtp->spec, batch);
    } catch (const std::exception &) {
        return false;
    }
}

// A new generation over `prompt` (the tokens in the target context)
extern "C" void cluaiz_mtp_begin(cluaiz_mtp * mtp, const llama_token * prompt, int32_t n_prompt) {
    if (mtp == nullptr) {
        return;
    }
    llama_tokens tokens(prompt, prompt + (n_prompt > 0 ? n_prompt : 0));
    common_speculative_begin(mtp->spec, 0, tokens);
}

// Drafts up to `n_max` tokens after `id_last`, which goes into the target at `pos0`; `prompt` holds
// the tokens before it. Writes them to `out` (room for `cap`) and returns how many.
extern "C" int32_t cluaiz_mtp_draft(
        cluaiz_mtp        * mtp,
        llama_pos           pos0,
        llama_token         id_last,
        const llama_token * prompt,
        int32_t             n_prompt,
        int32_t             n_max,
        llama_token       * out,
        int32_t             cap) {
    if (mtp == nullptr || n_max <= 0 || cap <= 0) {
        return 0;
    }
    try {
        llama_tokens before(prompt, prompt + (n_prompt > 0 ? n_prompt : 0));
        mtp->draft.clear();
        auto & dp = common_speculative_get_draft_params(mtp->spec, 0);
        dp.drafting = true;
        dp.n_max    = n_max;
        dp.pos0     = pos0;
        dp.id_last  = id_last;
        dp.prompt   = &before;
        dp.result   = &mtp->draft;
        dp.result_q = nullptr;
        common_speculative_draft(mtp->spec);
        const int32_t n = (int32_t) std::min<size_t>(mtp->draft.size(), (size_t) cap);
        for (int32_t i = 0; i < n; i++) {
            out[i] = mtp->draft[i];
        }
        return n;
    } catch (const std::exception &) {
        return 0;
    }
}

// How many tokens of the last draft the target accepted
extern "C" void cluaiz_mtp_accept(cluaiz_mtp * mtp, uint16_t n_accepted) {
    if (mtp != nullptr) {
        common_speculative_accept(mtp->spec, 0, n_accepted);
    }
}

// Removes the draft context's tokens from `p0` on, as the target's are removed
extern "C" void cluaiz_mtp_seq_rm(cluaiz_mtp * mtp, llama_pos p0) {
    if (mtp != nullptr) {
        llama_memory_seq_rm(llama_get_memory(mtp->ctx_dft), 0, p0, -1);
    }
}
