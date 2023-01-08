mod rust_jni_app{

    use std::{collections::HashMap, marker::PhantomData, path::Path};
    #[cfg(not(test))]
    use jni::{JNIEnv, sys::{jobject, JNINativeInterface_}, objects::{JObject}};

    #[cfg(not(test))]
    pub struct RustAppInsideJNI<'a>{
        jni_nativerunnable_obj:JObject<'a>,
        jni_environment: JNIEnv<'a>,
        phantom: PhantomData<&'a String>,
        packages_map:HashMap<String, HashMap<String,String> >,
    }
    #[cfg(test)]
    pub struct RustAppInsideJNI<'a>{
        packages_map:HashMap<String, HashMap<String,String> >,
        phantom: PhantomData<&'a String>,
    }


    impl<'a> RustAppInsideJNI<'a> {
        #[cfg(not(test))]
        pub fn new( jni: *mut *const JNINativeInterface_, thiz: jobject ) -> RustAppInsideJNI<'a> {
            RustAppInsideJNI{
                jni_nativerunnable_obj:unsafe{jni::objects::JObject::from_raw(thiz)},
                jni_environment:unsafe{JNIEnv::from_raw(jni).unwrap()},
                phantom:PhantomData,
                packages_map: HashMap::new(),
            }
        }


        #[cfg(test)]
        pub fn new() -> RustAppInsideJNI<'a> {
                RustAppInsideJNI{
                phantom:PhantomData,
                packages_map: HashMap::new(),
            }
        }

        #[cfg(not(test))]
        fn get_android_files_path(&self) -> anyhow::Result<String> {
            use jni::objects::JString;

            let res:JString=self.jni_environment.call_method (
                self.jni_nativerunnable_obj,
                "callbackGetFilesDir",
                "()Ljava/lang/String;",
                &[]
            )?.l()?.into();

            let res_java_str=self.jni_environment.get_string(res)?;
            let rust_str=res_java_str.to_str()?;

            Ok(  rust_str.to_string() )
//            getFilesDir()
        }

        #[cfg(not(test))]
        pub fn call_native_with_string(&self, data: &str) -> anyhow::Result<jni::objects::JValue>{
            let retres=self.jni_environment.call_method (
                self.jni_nativerunnable_obj,
                "callbackFromNative",
                "(Ljava/lang/String;)V",
                &[  self.jni_environment.new_string(data)?.into()  ]
            )?;
            Ok(retres)
        }


        async fn install_deb_with_url(&mut self,deb_url: String) -> anyhow::Result<()>{
            self.log("loading ar binary")?;

            let packages_body = reqwest::get(deb_url)
            .await?
            .bytes()
            .await?;
            self.log("done. trying unarchive")?;

            let content =  std::io::Cursor::new(packages_body);
            //    std::io::copy(&mut content, &mut file)?;
            let mut ar_archive = ar::Archive::new(content);

            while let Some(entry_result) = ar_archive.next_entry() {
                let entry = entry_result?;
                // Create a new file with the same name as the archive entry:
                let pth=String::from_utf8(entry.header().identifier().to_vec())?;
                self.log(format!("unpacking archive to {}",pth.clone()).as_str())?;

//                let mut file = std::fs::File::create(pth)?;
                // The Entry object also acts as an io::Read, so we can easily copy the
                // contents of the archive entry into the file:

                if pth.eq("data.tar.xz") {
                    //unpacking xz
//                    let enc = GzEncoder::new(tar_gz, Compression::default());
                    let xz_decoder=xz::read::XzDecoder::new(entry);
                    let mut tar_decoder=tar::Archive::new(xz_decoder);

                    println!("Extracted the following files:");
                    let tar_prefix="./data/data/com.termux/files";
                    tar_decoder
                        .entries()?
                        .filter_map(|e| e.ok())
                        .map(|mut entry| -> anyhow::Result<std::path::PathBuf> {
                            let path = entry.path()?.strip_prefix(tar_prefix)?.to_owned();
                            entry.unpack(&path)?;
                            Ok(path)
                        })
                        .filter_map(|e| e.ok())
                        .for_each(|x| self.log(format!("> {}", x.display()).as_str()).unwrap() );





                }

//                std::io::copy(&mut entry, &mut file)?;
            }


//            packages_body.

//            let bytes_reader=


            Ok(())
        }

        async fn termux_install_app(&mut self,app_name: String) -> anyhow::Result<()>{

            if let Some(app_info) = self.packages_map.get(app_name.as_str()) {
                    let deps_none=String::from("");
                    self.log(format!("app {} found in database. installing...",app_name ).as_str())?;
                    let deps=app_info.get("depends").unwrap_or( &deps_none );

                    let filename=app_info.get("filename").unwrap();
                    let url=format!("https://packages.termux.dev/apt/termux-main/{}",filename);
    
                    self.log(format!("DEPS={} url={}",deps,url).as_str())?;
    
                    self.install_deb_with_url(url).await?;
    

                }else{
                    self.log(format!("app {} not found in database. can't install ",app_name).as_str())?;
                    anyhow::bail!("app {} not found in database. can't install", app_name);
                }

                Ok(())
        }

        pub fn log(&self,txt: &str)-> anyhow::Result<()>{
            #[cfg(not(test))]
            self.call_native_with_string(  txt  )?;
            #[cfg(test)]
            println!("{}",txt);
            Ok(())
        }

        fn get_binary_arch(&self)->String{
            #[cfg(target_arch = "x86")]
            let termux_arch="i686";

            #[cfg(target_arch = "x86_64")]
            let termux_arch="x86_64";

            #[cfg(target_arch = "arm")]
            let termux_arch="arm";

            #[cfg(target_arch = "aarch64")]
            let termux_arch="aarch64";

            termux_arch.to_string()
        }

        async fn termux_load_database(&mut self)-> anyhow::Result<()>{
            //        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]

            let packages_url=format!("https://packages.termux.dev/apt/termux-main/dists/stable/main/binary-{}/Packages",self.get_binary_arch());

            self.log(  format!("ATTEMPT TO DOWNLOAD mod1").as_str()  )?;

            let packages_body = reqwest::get(packages_url)
            .await?
            .text()
            .await?;

            self.log(  format!("DOWNLOADED").as_str()  )?;

            let packages_body_splitted=packages_body.split("\n\n");
            self.packages_map=HashMap::new();

            for packages_row in packages_body_splitted {
                let mut row_package_map:HashMap<String,String> = HashMap::new();
                let mut name_of_package="unnamed".to_string();
                let packages_row_splitted:Vec<&str>=packages_row.lines().collect();
                for package_row in packages_row_splitted {
                    let package_line_splitted:Vec<&str>=package_row.split(": ").collect();
                    let package_line_key=(*package_line_splitted.get(0).unwrap()).to_lowercase();
                    let package_line_value=(*package_line_splitted.get(1).unwrap()).to_lowercase();
                    row_package_map.insert(package_line_key.clone(), package_line_value.clone() );
                    if package_line_key.eq("package") {
                        name_of_package=package_line_value.to_string();
                    }
                }
                self.packages_map.insert(name_of_package.to_lowercase(), row_package_map);   
            }
            self.log(  format!("packages len = {}",self.packages_map.keys().len()).as_str()  )?;

            Ok(())

        }


        async fn  termux_prepare_proot(&mut self)->anyhow::Result<()>{
            #[cfg(test)]
            let base_path_for_app="./".to_string();
            #[cfg(not(test))]
            let base_path_for_app=self.get_android_files_path()?;

            std::env::set_current_dir(Path::new(base_path_for_app.as_str()))?;
            if Path::new("usr/bin/proot").exists() {return Ok(());}
            
            self.termux_load_database().await?;
            self.termux_install_app("proot".to_string()).await?;
            self.termux_install_app("libtalloc".to_string()).await?;

            Ok(())
        }

        async fn run_proot(){
            //LD_LIBRARY_PATH=`pwd`/usr/lib ./usr/bin/proot 
            /*
            Usage:
            proot [option] ... [command]

            Regular options:
            -r *path*	Use *path* as the new guest root file-system, default is /.
            -b *path*	Make the content of *path* accessible in the guest rootfs.
            -q *command*	Execute guest programs through QEMU as specified by *command*.
            -w *path*	Set the initial working directory to *path*.
            --kill-on-exit		Kill all processes on command exit.
            -v *value*	Set the level of debug information to *value*.
            -V		Print version, copyright, license and contact, then exit.
            -h		Print the version and the command-line usage, then exit.

            Extension options:
            -k *string*	Make current kernel appear as kernel release *string*.
            -0		Make current user appear as "root" and fake its privileges.
            -i *string*	Make current user and group appear as *string* "uid:gid".
            --link2symlink		Replace hard links with symlinks, pretending they are really hardlinks
            --sysvipc		Handle System V IPC syscalls in proot
            -H		Hide files and directories starting with '.proot.' .
            -p		Modify bindings to protected ports to use a higher port number.
            -L		Correct the size returned from lstat for symbolic links.

            Alias options:
            -R *path*	Alias: -r *path* + a couple of recommended -b.
            -S *path*	Alias: -0 -r *path* + a couple of recommended -b.

            */
        }

        #[cfg(not(test))]
        async fn app_loop_async_prod(&mut self) -> anyhow::Result<()>{
            self.termux_prepare_proot().await?;
            Ok(())
        }

        #[cfg(test)]
        async fn app_loop_async_test(&mut self) -> anyhow::Result<()>{
            self.termux_prepare_proot().await?;
            Ok(())
        }


        #[tokio::main(flavor = "current_thread")]
        async fn app_loop_async(&mut self) -> anyhow::Result<()>{
            #[cfg(test)]
            self.app_loop_async_test().await?;

            #[cfg(not(test))]
            self.app_loop_async_prod().await?;

            Ok(())
        }

        pub fn app_loop(&mut self) -> anyhow::Result<()> {
            let _future = self.app_loop_async();
//            futures::executor::block_on(future)?;
            Ok(())
        }

    }
}

#[cfg(not(test))]
use jni::sys::{jobject, JNINativeInterface_};

#[cfg(not(test))]
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

#[cfg(test)]
mod tests {
    use crate::rust_jni_app::RustAppInsideJNI;
    #[test]
    fn  it_works() {
        let mut app = RustAppInsideJNI::new();
        app.app_loop().unwrap();
    }
}

