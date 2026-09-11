use cluaiz_llama::ffi::llama_cpp::*;
use std::ffi::CString;
use std::os::raw::c_char;

fn test_inference(gpu_layers: i32) {
    println!("\n==========================================");
    println!("TESTING INFERENCE WITH n_gpu_layers = {}", gpu_layers);
    println!("==========================================");

    unsafe {
        llama_backend_init();
        let path = CString::new("C:\\Users\\Aryan\\.cluaiz\\models\\chat\\qwen3-4b-gguf-Q4_K_M\\Qwen3-4B-Q4_K_M.gguf").unwrap();
        let mut model_params = llama_model_default_params();
        model_params.n_gpu_layers = gpu_layers;
        let model = llama_model_load_from_file(path.as_ptr(), model_params);
        if model.is_null() {
            println!("FAILED TO LOAD MODEL");
            return;
        }

        let mut ctx_params = llama_context_default_params();
        ctx_params.n_ctx = 2048;
        let ctx = llama_init_from_model(model, ctx_params);
        if ctx.is_null() {
            println!("FAILED TO INIT CONTEXT");
            llama_model_free(model);
            return;
        }

        let vocab = llama_model_get_vocab(model);

        // Render template for "hi"
        let rendered = "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n";
        println!("Prompt: {:?}", rendered);

        let c_prompt = CString::new(rendered).unwrap();
        let mut tokens = vec![0i32; 128];
        let n_tokens = llama_tokenize(vocab, c_prompt.as_ptr(), rendered.len() as i32, tokens.as_mut_ptr(), tokens.len() as i32, false, true);
        tokens.truncate(n_tokens as usize);
        println!("Prompt tokens ({}): {:?}", tokens.len(), tokens);

        // Build batch
        let mut batch = llama_batch_init(512, 0, 1);
        for (i, &t) in tokens.iter().enumerate() {
            *batch.token.add(i) = t;
            *batch.pos.add(i) = i as i32;
            *batch.n_seq_id.add(i) = 1;
            *(*batch.seq_id.add(i)).add(0) = 0;
            *batch.logits.add(i) = if i == tokens.len() - 1 { 1 } else { 0 };
        }
        batch.n_tokens = tokens.len() as i32;

        println!("Decoding prompt...");
        let dec_res = llama_decode(ctx, batch);
        println!("llama_decode returned: {}", dec_res);

        // Sampler with CORRECT FFI signature (n_vocab included)
        let sparams = LlamaSamplerChainParams { no_perf: true };
        let smpl = llama_sampler_chain_init(sparams);
        let n_vocab = llama_vocab_n_tokens(vocab);
        println!("Model n_vocab = {}", n_vocab);
        llama_sampler_chain_add(smpl, llama_sampler_init_penalties(n_vocab, 64, 1.1, 0.0, 0.0));
        llama_sampler_chain_add(smpl, llama_sampler_init_top_k(40));
        llama_sampler_chain_add(smpl, llama_sampler_init_top_p(0.95, 1));
        llama_sampler_chain_add(smpl, llama_sampler_init_min_p(0.05, 1));
        llama_sampler_chain_add(smpl, llama_sampler_init_temp(0.7));
        llama_sampler_chain_add(smpl, llama_sampler_init_dist(1234));

        for &t in &tokens {
            llama_sampler_accept(smpl, t);
        }

        let mut next_token = llama_sampler_sample(smpl, ctx, -1);
        println!("Initial sampled token: {}", next_token);

        let mut generated = String::new();
        let mut n_cur = tokens.len() as i32;
        for step in 0..25 {
            if llama_vocab_is_eog(vocab, next_token) {
                println!("\nHit EOG token: {}", next_token);
                break;
            }
            llama_sampler_accept(smpl, next_token);

            let mut buf = [0u8; 128];
            let n = llama_token_to_piece(vocab, next_token, buf.as_mut_ptr() as *mut c_char, buf.len() as i32, 0, true);
            if n > 0 {
                let piece = String::from_utf8_lossy(&buf[..n as usize]).to_string();
                print!("{}", piece);
                generated.push_str(&piece);
            }

            // Decode 1 token
            *batch.token.add(0) = next_token;
            *batch.pos.add(0) = n_cur;
            *batch.n_seq_id.add(0) = 1;
            *(*batch.seq_id.add(0)).add(0) = 0;
            *batch.logits.add(0) = 1;
            batch.n_tokens = 1;

            let ret = llama_decode(ctx, batch);
            if ret != 0 {
                println!("\nDecode failed at step {} with code {}", step, ret);
                break;
            }
            n_cur += 1;
            next_token = llama_sampler_sample(smpl, ctx, -1);
        }

        println!("\nTotal generated: {:?}", generated);

        llama_batch_free(batch);
        llama_sampler_free(smpl);
        llama_free(ctx);
        llama_model_free(model);
    }
}

fn main() {
    // 1. First test with CPU (gpu_layers = 0)
    test_inference(0);

    // 2. Then test with GPU (gpu_layers = -1)
    test_inference(-1);
}
