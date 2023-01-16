#!/bin/sh

#NEED VFS_FILE IN CONTAINER for exmpl

#cargo test -- --nocapture --color always
#cargo test --lib -- --nocapture --color always docker_hub

#adb shell mount -o remount,rw /
#adb shell mount -o remount,rw /vendor

cargo build --target armv7-linux-androideabi --release --bin virtual_engine_bin --features kmsg_debug
#adb push ./target/armv7-linux-androideabi/release/virtual_engine_bin /data/local/tmp/virt/

sleep 1
adb push ./target/armv7-linux-androideabi/release/virtual_engine_bin /system/bin/
adb shell chcon "u:object_r:adbd_exec:s0" /system/bin/virtual_engine_bin 


return true
exit 0

adb shell mkdir /mnt/sdcard/zigbee2mqtt/ 
adb push /Users/alex/Downloads/zigbee2mqtt-1.29.0/data/configuration.yaml /mnt/sdcard/zigbee2mqtt/ 


echo  << 'EOF'
service initAsic /system/bin/virtual_engine_bin {\"files_dir\":\"/data/data/xao_virtual_engine/\"}
    seclabel u:r:su:s0
    class main
    user root
    group system root
    disabled

on property:asic=5
    start initAsic
    write /proc/bootevent "ASIC set to 5"

on property:sys.boot_completed=1
    start initAsic
    write /proc/bootevent "ASIC boot completed"
EOF 

adb shell mount -o remount,ro /
adb shell mount -o remount,ro /vendor


return 0
exit 0


#{"docker_hub":{"image":"koenkk/zigbee2mqtt:latest","save_to":"","arch":"arm/v7"},"vm_path":"%FILES%/vms/zigbee2mqtt","chroot_mode":"chroot","volumes":{"/mnt/sdcard/zigbee2mqtt":"/app/data"},"envs":null,"entrypoint":null,"cmd":null,"start_on_boot":true,"workdir":null}
#{"docker_hub":{"image":"nodered/node-red:latest-minimal","save_to":"","arch":"arm/v7"},"vm_path":"%FILES%/vms/nodered","chroot_mode":"chroot","volumes":{"/mnt/sdcard/nodered":"/data"},"envs":null,"entrypoint":null,"cmd":null,"start_on_boot":true,"workdir":null}



#service initAsic /system/bin/virtual_engine_bin {\"files_dir\":\"/data/local/tmp/virt/\"}
#    seclabel u:r:su:s0
#    class main
#    user root
#    group system root
#    disabled
#
#on property:asic=5
#    start initAsic
#    write /proc/bootevent "ASIC set to 5"
#
#on property:sys.boot_completed=1
#    start initAsic
#    write /proc/bootevent "ASIC boot completed"


#/system/etc/selinux/plat_service_contexts
# adb push /Users/alex/Downloads/zigbee2mqtt-1.29.0/data/configuration.yaml /sdcard/zigbee2mqtt/configuration.yaml


#pm disable com.google.android.katniss  
#pm disable com.google.tv.remote.service
#pm disable com.google.android.gms
#pm disable com.google.android.tvrecommendations
#pm disable com.google.android.leanbacklauncher.recommendations
#pm disable com.android.vending
#pm disable com.android.gallery3d
