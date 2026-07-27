fn main() {
    let builder = ort::Session::builder().unwrap();
    let cuda_ep = ort::execution_providers::CUDAExecutionProvider::default();
    match cuda_ep.build().register(&builder) {
        Ok(_) => println!("CUDA Registered!"),
        Err(e) => println!("CUDA Failed: {:?}", e),
    }
}
