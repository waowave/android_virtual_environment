#!/bin/sh

#cargo test -- --nocapture --color always
#cargo test --lib -- --nocapture --color always docker_hub

cargo build --target armv7-linux-androideabi --release --bin virtual_engine_bin
adb push ./target/armv7-linux-androideabi/release/virtual_engine_bin /data/local/tmp/virt/

# adb push /Users/alex/Downloads/zigbee2mqtt-1.29.0/data/configuration.yaml /sdcard/zigbee2mqtt/configuration.yaml


#pm disable com.google.android.katniss  
#pm disable com.google.tv.remote.service
#pm disable com.google.android.gms
#pm disable com.google.android.tvrecommendations
#pm disable com.google.android.leanbacklauncher.recommendations
#pm disable com.android.vending
#pm disable com.android.gallery3d
