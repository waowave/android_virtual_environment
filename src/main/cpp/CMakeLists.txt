# For more information about using CMake with Android Studio, read the
# documentation: https://d.android.com/studio/projects/add-native-code.html

# Sets the minimum version of CMake required to build the native library.

cmake_minimum_required(VERSION 3.18.1)
include(ExternalProject)

# Declares and names the project.

project("myapplication")

### from https://stackoverflow.com/questions/31162438/how-can-i-build-rust-code-with-a-c-qt-cmake-project
# Set default ExternalProject root directory
#set_directory_properties(PROPERTIES EP_PREFIX ${CMAKE_BINARY_DIR}/rust)

#set(RUST_TARGET "nothing")

if ( ${ANDROID_ABI} STREQUAL "armeabi-v7a" )
    set(RUST_TARGET "armv7-linux-androideabi")
    set(CLANG_APPNAME_PREFIX "armv7a-linux-androideabi")
elseif( ${ANDROID_ABI} STREQUAL "armeabi-v7a with NEON" )
    set(RUST_TARGET "thumbv7neon-linux-androideabi")
    set(CLANG_APPNAME_PREFIX "armv7a-linux-androideabi")
elseif( ${ANDROID_ABI} STREQUAL "arm64-v8a" )
    set(RUST_TARGET "aarch64-linux-android")
    set(CLANG_APPNAME_PREFIX "aarch64-linux-android")
elseif( "${ANDROID_ABI}" STREQUAL "x86" )
    set(RUST_TARGET "i686-linux-android")
    set(CLANG_APPNAME_PREFIX "i686-linux-android")
elseif( ${ANDROID_ABI} STREQUAL "x86_64" )
    set(RUST_TARGET "x86_64-linux-android")
    set(CLANG_APPNAME_PREFIX "x86_64-linux-android")
endif()

#message(AUTHOR_WARNING "Hello CMake, hello Android")
message(AUTHOR_WARNING "android abi is ${ANDROID_ABI} and target is ${RUST_TARGET}")

add_library( # Sets the name of the library.
        myapplication
        # Sets the library as a shared library.
        SHARED
        # Provides a relative path to your source file(s).
        native-lib.cpp
        )


#${ANDROID_ABI}
# Add rust_example as a CMake target
find_program(CARGO_EXE NAMES cargo)


#ANDROID_NDK
#ANDROID_TOOLCHAIN_PREFIX
#ANDROID_TOOLCHAIN
#ANDROID_ABI
#ANDROID_PLATFORM
#ANDROID_STL
#ANDROID_PIE
#ANDROID_CPP_FEATURES
#ANDROID_ALLOW_UNDEFINED_SYMBOLS
#ANDROID_ARM_MODE
#ANDROID_ARM_NEON
#ANDROID_DISABLE_NO_EXECUTE
#ANDROID_DISABLE_RELRO
#ANDROID_DISABLE_FORMAT_STRING_CHECKS
#ANDROID_CCACHE

# ~/Library/Android/sdk/cmake/cmake_version_xxx/android.toolchain.cmake

#set (RUST_SOURCE_DIR "./")
message(AUTHOR_WARNING "
cargo exe is ${CARGO_EXE}
NDK TOOLCHAIN PREFIX=${ANDROID_TOOLCHAIN_PREFIX}
ANDROID_TOOLCHAIN_ROOT=${ANDROID_TOOLCHAIN_ROOT}
ANDROID_TOOLCHAIN_NAME=${ANDROID_TOOLCHAIN_NAME}
ANDROID_ABI=${ANDROID_ABI}
ANDROID_PLATFORM_LEVEL=${ANDROID_PLATFORM_LEVEL}
")
#  && cp ${CMAKE_BINARY_DIR}/cargo_targets/${RUST_TARGET}/
#set_directory_properties(PROPERTIES EP_PREFIX ${CMAKE_BINARY_DIR}/Rust)
#--out-dir ${CMAKE_BINARY_DIR}/CMakeFiles/myapplication.dir/

set("NDK_TOOLCHAIN_REAL_PREFIX" ${ANDROID_TOOLCHAIN_ROOT}/bin/${CLANG_APPNAME_PREFIX}${ANDROID_PLATFORM_LEVEL}-)
set("NDK_TOOLCHAIN_REAL_PREFIX_AR" ${ANDROID_TOOLCHAIN_ROOT}/bin/llvm-ar)

ExternalProject_Add(project_rust_main_lib
        DOWNLOAD_COMMAND ""
        CONFIGURE_COMMAND ""
        INSTALL_COMMAND ""
        BUILD_COMMAND env AR=${NDK_TOOLCHAIN_REAL_PREFIX_AR} CC=${NDK_TOOLCHAIN_REAL_PREFIX}clang CXX=${NDK_TOOLCHAIN_REAL_PREFIX}clang++ ${CARGO_EXE} build --target ${RUST_TARGET} --release --lib --target-dir ${CMAKE_LIBRARY_OUTPUT_DIRECTORY}/cargo_targets --features jni
#        COMMAND ${CARGO_EXE} build --target ${RUST_TARGET} --release --target-dir ${CMAKE_LIBRARY_OUTPUT_DIRECTORY}/cargo_targets
        BUILD_ALWAYS ON
        #${CMAKE_SOURCE_DIR}/
        # ${CMAKE_BINARY_DIR}
        SOURCE_DIR ${CMAKE_SOURCE_DIR}
#        WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/common-rust
#        BUILD_IN_SOURCE true
        BINARY_DIR ${CMAKE_SOURCE_DIR}
        BUILD_BYPRODUCTS ${CMAKE_LIBRARY_OUTPUT_DIRECTORY}/cargo_targets/${RUST_TARGET}/release/libmain_lib.a
        LOG_BUILD true
        )



add_library(main_lib STATIC IMPORTED)
add_dependencies(main_lib project_rust_main_lib)
set_target_properties(main_lib PROPERTIES IMPORTED_LOCATION ${CMAKE_LIBRARY_OUTPUT_DIRECTORY}/cargo_targets/${RUST_TARGET}/release/libmain_lib.a )

# Create dependency of Example on rust_example
add_dependencies(myapplication main_lib)


# Creates and names a library, sets it as either STATIC
# or SHARED, and provides the relative paths to its source code.
# You can define multiple libraries, and CMake builds them for you.
# Gradle automatically packages shared libraries with your APK.

#add_library( imported-lib
 #       SHARED
 #       IMPORTED )
#

#add_library(...)
#set_target_properties( # Specifies the target library.
#        imported-lib

        # Specifies the parameter you want to define.
#        PROPERTIES IMPORTED_LOCATION

        # Provides the path to the library you want to import.
#        imported-lib/src/${ANDROID_ABI}/libimported-lib.so )


# Searches for a specified prebuilt library and stores the path as a
# variable. Because CMake includes system libraries in the search path by
# default, you only need to specify the name of the public NDK library
# you want to add. CMake verifies that the library exists before
# completing its build.

find_library( # Sets the name of the path variable.
        log-lib
        # Specifies the name of the NDK library that
        # you want CMake to locate.
        log)

# Specifies libraries CMake should link to your target library. You
# can link multiple libraries, such as libraries you define in this
# build script, prebuilt third-party libraries, or system libraries.

#"${CMAKE_SOURCE_DIR}/common-rust/target/release/librust_example.a"

#SET(CMAKE_EXE_LINKER_FLAGS
#        "${CMAKE_EXE_LINKER_FLAGS} -Wl,-rpath -Wl,${CMAKE_BINARY_DIR}/cargo_targets/${RUST_TARGET}/debug/")
#message(AUTHOR_WARNING "ld path = ${CMAKE_EXE_LINKER_FLAGS}")

#link_directories(${CMAKE_BINARY_DIR}/cargo_targets/${RUST_TARGET}/debug/)

#add_library(main_lib STATIC IMPORTED) # or STATIC instead of SHARED
#set_target_properties(main_lib PROPERTIES
#        IMPORTED_LOCATION "${CMAKE_BINARY_DIR}/CMakeFiles/myapplication.dir/libmain_lib.a"
#        )


target_link_libraries( # Specifies the target library.
        myapplication
        # Links the target library to the log library
        # included in the NDK.
#        debug "main_lib"
#        optimized "cargo_targets/${RUST_TARGET}/release/libmain_lib.a"
        main_lib
        ${log-lib}
)
#${CMAKE_SOURCE_DIR}