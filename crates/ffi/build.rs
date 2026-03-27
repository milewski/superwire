fn main() {
    if std::env::var_os("CARGO_FEATURE_JS").is_some() {
        napi_build::setup();
    }
}
