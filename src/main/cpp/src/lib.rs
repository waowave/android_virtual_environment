#[cfg(feature="jni")]
use jni::sys::{jobject, JNINativeInterface_};
pub mod rust_jni_app;

#[cfg(feature="jni")]
#[no_mangle]
pub extern "C" fn Rust_Java_io_xao_myapplication_NativeRunnable_onServiceStart(jni: *mut *const JNINativeInterface_, thiz: jobject) {
    use rust_jni_app::RustAppInsideJNI;
    let mut app = RustAppInsideJNI::new(jni, thiz);
    match app.app_loop() {
        Ok(()) => {

        },
        Err(e)=>{
            println!("got error while run loop {:?}",e);
        },
    }
}
