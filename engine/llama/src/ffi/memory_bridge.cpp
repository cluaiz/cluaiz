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
        uint32_t n_ctx_train = 0;
        uint32_t n_expert = 0;
        const common_device_memory_data_vec data = common_get_device_memory_data(
            path_model, mparams, cparams, devs, n_layer, n_ctx_train, n_expert, GGML_LOG_LEVEL_ERROR);
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
