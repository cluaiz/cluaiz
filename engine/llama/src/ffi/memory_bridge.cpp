// C ABI only: what a model + context will take per device, measured by llama.cpp itself with a
// no-alloc load (the dry run behind llama.cpp's --fit). Nothing is allocated and nothing is
// estimated here; the planner corrects its own byte estimate with these numbers.
#include "fit.h"
#include "llama.h"

#include <cstdint>
#include <exception>
#include <vector>

// out[0..3]: model, context, compute bytes on the first GPU; out[3..6]: the same in host memory.
// Returns 0 on success, -1 when llama.cpp could not load the model or create the context.
extern "C" int cluaiz_projected_memory(
        const char * path_model,
        const llama_model_params * mparams,
        const llama_context_params * cparams,
        uint64_t * out) {
    for (int i = 0; i < 6; i++) {
        out[i] = 0;
    }
    try {
        std::vector<ggml_backend_dev_t> devs;
        uint32_t n_layer = 0;
        uint32_t n_layer_skip = 0; // NextN layers llama.cpp does not offload
        uint32_t n_ctx_train = 0;
        uint32_t n_expert = 0;
        const common_device_memory_data_vec data = common_get_device_memory_data(
            std::vector<std::string>{ path_model }, mparams, cparams, devs, n_layer, n_layer_skip, n_ctx_train, n_expert, GGML_LOG_LEVEL_ERROR);
        if (data.empty()) {
            return -1;
        }
        const common_device_memory_data & host = data.back();
        out[3] = host.model;
        out[4] = host.context;
        out[5] = host.compute;
        if (data.size() >= 2) {
            out[0] = data[0].model;
            out[1] = data[0].context;
            out[2] = data[0].compute;
        }
        return 0;
    } catch (const std::exception &) {
        return -1;
    }
}

// Layout of every struct Rust mirrors by hand (ffi/llama_cpp.rs, ffi/mtmd.rs): sizes and the
// offsets of fields after parts llama.cpp has changed before. A test compares them with Rust's
// layout, so a header change after an upstream merge fails the tests instead of shifting fields.
#include "../../llama.cpp/tools/mtmd/mtmd-helper.h"

#include <cstddef>

extern "C" void cluaiz_param_layout(uint64_t * out) {
    out[0]  = sizeof(llama_context_params);
    out[1]  = offsetof(llama_context_params, type_v);
    out[2]  = offsetof(llama_context_params, abort_callback);
    out[3]  = offsetof(llama_context_params, swa_full);
    out[4]  = offsetof(llama_context_params, ctx_other);
    out[5]  = sizeof(llama_model_params);
    out[6]  = offsetof(llama_model_params, n_gpu_layers);
    out[7]  = offsetof(llama_model_params, kv_overrides);
    out[8]  = offsetof(llama_model_params, load_mtp);
    out[9]  = sizeof(llama_perf_context_data);
    out[10] = sizeof(llama_chat_message);
    out[11] = sizeof(llama_token_data);
    out[12] = sizeof(llama_token_data_array);
    out[13] = sizeof(llama_sampler_chain_params);
    out[14] = sizeof(llama_batch);
    out[15] = sizeof(llama_model_tensor_buft_override);
    out[16] = sizeof(mtmd_helper_gen_audio_inp);
    out[17] = offsetof(mtmd_helper_gen_audio_inp, speaker_ref);
    out[18] = offsetof(mtmd_helper_gen_audio_inp, out_type);
    out[19] = sizeof(mtmd_gen_audio_info);
}
