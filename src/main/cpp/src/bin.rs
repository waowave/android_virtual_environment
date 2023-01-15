pub mod rust_jni_app;
use rust_jni_app::RustAppInsideJNI;

pub fn main() {
    let mut app = RustAppInsideJNI::new();
    app.app_loop().unwrap();
}
