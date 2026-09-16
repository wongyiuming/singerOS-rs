#[cfg(target_arch = "wasm32")]
pub fn compile_probe() {
    let _host = cpal::default_host();
}

#[cfg(not(target_arch = "wasm32"))]
pub fn compile_probe() {}
