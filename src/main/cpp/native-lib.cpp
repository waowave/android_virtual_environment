#include <jni.h>
#include <string>

extern "C" {

    void Rust_Java_io_xao_myapplication_NativeRunnable_onServiceStart(JNIEnv *env, jobject thiz);
    JNIEXPORT void JNICALL Java_io_xao_myapplication_NativeRunnable_onServiceStart(JNIEnv *env, jobject thiz) {
        Rust_Java_io_xao_myapplication_NativeRunnable_onServiceStart(env,thiz);
    }

}
