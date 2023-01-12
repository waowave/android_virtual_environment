pub mod vm;
pub mod docker_hub;

mod rust_jni_app{

    type HGenericError = Box<dyn std::error::Error + Send + Sync>;
    type HResult<T> = std::result::Result<T, HGenericError>;
    type HBoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
    type ArcMutex<T> = Arc<tokio::sync::Mutex<T>>;

    use anyhow::bail;
    use hyper::{body::Incoming as HIncomingBody, StatusCode as HStatusCode, Response as HResponse};
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use tokio::io::{AsyncWriteExt, AsyncReadExt};
    use std::{io::prelude::*};

    use bytes::{Bytes, Buf};
    use hyper::Method;
    use std::{collections::HashMap, marker::PhantomData, path::Path, sync::Arc};
    use crate::{vm::VM, docker_hub::JConfigConfig};

    #[cfg(not(test))]
    use jni::{JNIEnv, sys::{jobject, JNINativeInterface_}, objects::{JObject}};

    pub struct RustAppInsideJNI<'a>{
        #[cfg(not(test))]
        jni_nativerunnable_obj: JObject<'a>,
        #[cfg(not(test))]
        jni_environment: JNIEnv<'a>,
        phantom: PhantomData<&'a String>,
        packages_map:tokio::sync::Mutex<HashMap<String, HashMap<String,String> >>,
        vms:  ArcMutex<HashMap<String,ArcMutex<VM>>>,
        #[cfg(not(test))]
        files_dir:std::sync::Mutex<String>,
    }

    impl<'a> RustAppInsideJNI<'a> {
        #[cfg(not(test))]
        pub fn new( jni: *mut *const JNINativeInterface_, thiz: jobject ) -> RustAppInsideJNI<'a> {
            RustAppInsideJNI{
                jni_nativerunnable_obj:unsafe{jni::objects::JObject::from_raw(thiz)},
                jni_environment:unsafe{JNIEnv::from_raw(jni).unwrap()},
                phantom:PhantomData,
                packages_map: tokio::sync::Mutex::new(HashMap::new()),
                vms: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                files_dir:std::sync::Mutex::new(String::new()),
            }
        }


        #[cfg(test)]
        pub fn new() -> RustAppInsideJNI<'a> {
                RustAppInsideJNI{
                phantom:PhantomData,
                packages_map: tokio::sync::Mutex::new(HashMap::new()),
                vms: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            }
        }

        pub fn log(&self,txt: &str)-> anyhow::Result<()>{
            #[cfg(not(test))]
            self.call_native_with_string(  txt  )?;
            #[cfg(test)]
            println!("{}",txt);
            Ok(())
        }


        fn get_files_path (&self) -> anyhow::Result<String>{
            #[cfg(not(test))]
            return self.get_android_files_path();
            #[cfg(test)]
            {
                let mut current_exe = std::env::current_exe()?;
                current_exe.pop();
                current_exe.push("files");
                let pa=current_exe.as_path();
                if !pa.exists() {
                    std::fs::create_dir(pa.clone())?;
                }
                return Ok(pa.to_str().unwrap().to_string()  );
            }
        }

        #[cfg(not(test))]
        fn get_android_files_path(&self) -> anyhow::Result<String> {
            let mut files_dir_clone=self.files_dir.lock().unwrap();

            if !files_dir_clone.eq(""){
                return Ok(files_dir_clone.clone() );
            }

            use jni::objects::JString;

            let res:JString=self.jni_environment.call_method (
                self.jni_nativerunnable_obj,
                "callbackGetFilesDir",
                "()Ljava/lang/String;",
                &[]
            )?.l()?.into();

            let res_java_str=self.jni_environment.get_string(res)?;
            let rust_str=res_java_str.to_str()?;

            files_dir_clone.clear();
            files_dir_clone.push_str(rust_str);

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

        //special for this app


        async fn load_binary_from_url(&self, deb_url: String) -> anyhow::Result<bytes::Bytes>{
            let packages_body = reqwest::get(deb_url)
            .await?
            .bytes()
            .await?;
            self.log("done. trying unarchive")?;

            Ok(packages_body)
        }

        async fn download_url_to_file(&self, download_url: String,save_to_filename:&str,is_executable:bool ) -> anyhow::Result<()>{
            let bin=self.load_binary_from_url(download_url).await?;
            let mut file = tokio::fs::File::create(save_to_filename).await?;
            let mut buf:Vec<u8>=Vec::new();
            let _bytes=bin.reader().read_to_end(&mut buf)?;
            file.write_all ( &buf ).await?;

            use std::os::unix::fs::PermissionsExt;
  //          PermissionsExt::

            let mut perm=std::fs::Permissions::from_mode(0o777);
            if !is_executable{
                perm.set_mode(0o644);
//                perm.set_readonly(true);
            }

            file.set_permissions(perm).await?;


//            let mut perms = file.metadata().await?.permissions();
//            perms

            Ok(())
        }


        
        async fn unarchive_tar<R: Read> (&self, reader:R, ignore_prefix:&str)-> anyhow::Result<()>{ 
            self.log(format!("unarchiving tar...: ").as_str())?;
            let mut tar_decoder=tar::Archive::new(reader);
            tar_decoder
                .entries()?
                .filter_map(|e| e.ok())
                .map(|mut entry| -> anyhow::Result<std::path::PathBuf> {
                    let path = entry.path()?.strip_prefix(ignore_prefix)?.to_owned();
                    entry.unpack(&path)?;
                    Ok(path)
                })
                .filter_map(|e|
                    match e {
                        Err(er)=>{self.log(format!("unpack error > {}", er ).as_str()).unwrap(); None},
                        Ok(pt)=>{Some(pt)}
                    }
                )
                .for_each(|x| self.log(format!("> {}", x.display()).as_str()).unwrap() );
                self.log(format!("unarchiving tar done. returning OK ").as_str())?;
                Ok(())       
        }

        async fn unarchive_compressed_tar<R: Read> (&self, reader:R, format: &str,ignore_prefix:&str)-> anyhow::Result<()>{ 

            match format{
                "tar.xz"=>{
                    //let tar_decoded=XzDecoder::new(reader);
                    self.log(format!("trying unarchive as XZ: ").as_str())?;

                    let tar_decoded=xz::read::XzDecoder::new(reader);                    
                    self.unarchive_tar(tar_decoded,ignore_prefix).await?;
                },
                "tar.gz" | "tar.gzip"=>{
                    self.log(format!("trying unarchive as GZ: ").as_str())?;
                    let tar_decoded=flate2::read::GzDecoder::new(reader);
                    self.unarchive_tar(tar_decoded,ignore_prefix).await?;
                }

//                "tar.xz"=>{tar_decoded=xz::read::XzDecoder::new(reader);}
//                "tar.gz"=>{tar_decoded=async_compression::tokio::bufread::GzipDecoder::new(reader);}
                _ =>{}
            }
//            let xz_decoder=xz::read::XzDecoder::new(reader);


            println!("Extracted the following files:");


            Ok(())
        }


        async fn install_archive_with_url(&self, arc_url: String,ignore_prefix:&str) -> anyhow::Result<()>{
            let content =  self.load_binary_from_url(arc_url.clone()).await?;
            if arc_url.ends_with("tar.xz"){
                let endfmt="tar.xz";
                self.unarchive_compressed_tar(content.reader(),endfmt,ignore_prefix).await?;
            }else if arc_url.ends_with("tar.gz"){
                let endfmt="tar.gz";
                self.unarchive_compressed_tar(content.reader(),endfmt,ignore_prefix).await?;
            }else if arc_url.ends_with("tar"){
                self.unarchive_tar(content.reader(),ignore_prefix).await?;
            }

            Ok(())
        }

        async fn install_deb_with_url(&self, deb_url: String,ignore_prefix:&str) -> anyhow::Result<()>{
            self.log("loading ar binary")?;
            let content =  self.load_binary_from_url(deb_url).await?;
            let mut ar_archive = ar::Archive::new(content.reader() );

            while let Some(entry_result) = ar_archive.next_entry() {
                let  entry = entry_result?;
                // Create a new file with the same name as the archive entry:
                let pth=String::from_utf8(entry.header().identifier().to_vec())?;
                self.log(format!("finded file in deb: {}",pth.clone().as_str() ).as_str())?;

//                let mut file = std::fs::File::create(pth)?;
                // The Entry object also acts as an io::Read, so we can easily copy the
                // contents of the archive entry into the file:

//                let mut bf=vec![];
//                let bf_readed=(entry.read_to_end(&mut bf) )?;
//                let bfu8:&[u8]=&bf;
//                let buf_reader=tokio::io::BufReader::new(bfu8);

                if pth.contains("data.tar.xz") {
                    self.unarchive_compressed_tar(entry,"tar.xz",ignore_prefix).await?;
                }else if pth.contains("data.tar.gz") {
                    self.unarchive_compressed_tar(entry,"tar.gz",ignore_prefix).await?;
                }

//                std::io::copy(&mut entry, &mut file)?;
            }


//            packages_body.

//            let bytes_reader=


            Ok(())
        }

    
        async fn termux_install_app(&self,app_name: String) -> anyhow::Result<()>{

            self.termux_load_database_if_needed().await?;

            if let Some(app_info) = self.packages_map.lock().await.get(app_name.as_str()) {
                    let deps_none=String::from("");
                    self.log(format!("app {} found in database. installing...",app_name ).as_str())?;
                    let deps=app_info.get("depends").unwrap_or( &deps_none );

                    let filename=app_info.get("filename").unwrap();
                    let url=format!("https://packages.termux.dev/apt/termux-main/{}",filename);
    
                    self.log(format!("DEPS={} url={}",deps,url).as_str())?;
    
                    self.install_deb_with_url(url,"./data/data/com.termux/files").await?;

                }else{
                    self.log(format!("app {} not found in database. can't install ",app_name).as_str())?;
                    anyhow::bail!("app {} not found in database. can't install", app_name);
                }

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

        async fn termux_load_database_if_needed(&self)-> anyhow::Result<()>{
            //        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            if self.packages_map.lock().await.len()!=0 {return Ok(())}
            let packages_url=format!("https://packages.termux.dev/apt/termux-main/dists/stable/main/binary-{}/Packages",self.get_binary_arch());

            self.log(  format!("ATTEMPT TO DOWNLOAD termux database").as_str()  )?;

            let packages_body = reqwest::get(packages_url)
            .await?
            .text()
            .await?;

            self.log(  format!("DOWNLOADED").as_str()  )?;

            let packages_body_splitted=packages_body.split("\n\n");
            self.packages_map.lock().await.clear();

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
                self.packages_map.lock().await.insert(name_of_package.to_lowercase(), row_package_map);   
            }
            self.log(  format!("termux database packages len = {}",self.packages_map.lock().await.keys().len()).as_str()  )?;

            Ok(())

        }

 /* 
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
*/

        fn set_current_directory_to_files(&self)->anyhow::Result<()>{
            let base_path_for_app=self.get_files_path()?;
            self.log(format!("settings current directory to {}", base_path_for_app ).as_str())?;
            std::env::set_current_dir(Path::new(base_path_for_app.as_str()))?;
            Ok(())
        }

        async fn  download_release_proot_if_needed(&self)->anyhow::Result<()>{
            self.log("download_release_proot_if_needed executed")?;
            self.set_current_directory_to_files()?;
            if Path::new("bin/proot").exists() {return Ok(());}
            self.log("bin/proot not found")?;

//            #[cfg(target_arch = "x86")]
//            return Ok(());
            #[cfg(target_arch = "x86_64")]
            let proot_archive_arch="x86_64";
            #[cfg(target_arch = "arm")]
            let proot_archive_arch="armv7a";
            #[cfg(target_arch = "aarch64")]
            let proot_archive_arch="aarch64";

            #[cfg(not(target_arch = "x86"))]
            {
//                let version="5.3.0";
//                let down_url=format!("https://github.com/proot-me/proot/releases/download/v{}/proot-v{}-{}-static",version,version,proot_archive_arch).to_string();
                let down_url=format!("https://raw.githubusercontent.com/waowave/build-proot-android-fork/master/packages/proot-android-{}.tar.gz",proot_archive_arch).to_string();
                self.log(format!("install_archive_with_url ur={}",down_url).as_str())?;
                self.install_archive_with_url(down_url, "root").await?;
                //self.download_url_to_file(down_url, "proot",true).await?;
            }

            Ok(())
        }

/*
        async fn  download_release_proot_rs_if_needed(&mut self)->anyhow::Result<()>{            
            if Path::new("proot-rs").exists() {return Ok(());}

            #[cfg(target_arch = "x86")]
            let proot_rs_archive_arch="i686-linux-android";

            #[cfg(target_arch = "x86_64")]
            let proot_rs_archive_arch="x86_64-linux-android";

            #[cfg(target_arch = "arm")]
            let proot_rs_archive_arch="arm-linux-androideabi";

            #[cfg(target_arch = "aarch64")]
            let proot_rs_archive_arch="aarch64-linux-android";

            self.install_archive_with_url(format!("https://github.com/proot-me/proot-rs/releases/download/v0.1.0/proot-rs-v0.1.0-{}.tar.gz",proot_rs_archive_arch).to_string(), "").await?;
 
            Ok(())
        }
 */

        #[cfg(not(test))]
        async fn app_loop_async_prod(&mut self) -> anyhow::Result<()>{
            self.set_current_directory_to_files()?;
            self.download_release_proot_if_needed().await?;
            self.http_server_start().await?;
            Ok(())
        }

        #[cfg(test)]
        async fn app_loop_async_test(&mut self) -> anyhow::Result<()>{
            self.set_current_directory_to_files()?;
            self.download_release_proot_if_needed().await?;
            /*
            docket_hub.download_blobs(async move |bytes,fmt| -> anyhow::Result<()> {                
                arced_self_clone.clone().unarchive_compressed_tar(bytes.reader(), "tar.gz", "").await?;
                Ok(())
            }).await?;
            */
            self.http_server_start().await?;
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
            let _future = self.app_loop_async()?;
            Ok(())
        }

        pub fn replace_path_with_env(&self,str:String )  -> anyhow::Result<String>{
            Ok(str.replace("%FILES%", self.get_files_path()?.as_str()))
        }

        pub async fn vm_add_app(&self,vm_name:String, exe:String,args:Vec<String>, envs_p: HashMap<String,String>) -> anyhow::Result<()>{            
            let mut envs=envs_p.clone();
            let mut vm_mtx=self.vms.lock().await;

            if vm_mtx.contains_key(vm_name.as_str()) {
                std::mem::drop(vm_mtx);
                anyhow::bail!("vm with name {} already exists", vm_name)
            }
            let exe_replaced=self.replace_path_with_env(exe)?;
            let args_replaced=args.iter().map(move |s| self.replace_path_with_env(s.clone()).unwrap()).collect();

                if let Ok(mut container_info_f) = tokio::fs::File::open(".container_info.json").await{
                    let mut container_info_str=String::new();
                    container_info_f.read_to_string(&mut container_info_str).await?;
                    let conf:JConfigConfig = serde_json::from_str(container_info_str.as_str())?;


                    conf.env.iter().for_each(
                        |row|  {
                            let file_env_kv:Vec<&str> = row.split("=").collect();
                            if file_env_kv.len()==2{
                                let file_env_key=file_env_kv[0];
                                if envs.get(file_env_key).is_none() {
                                    envs.insert(file_env_key.to_string(), file_env_kv[1].to_string());
                                }
                            }
                        }
                      );                      
                }

            let vm=VM::new( exe_replaced,args_replaced,envs ).await?;
            vm_mtx.insert(vm_name.to_string(), Arc::new(tokio::sync::Mutex::new(vm)) );
            Ok(())
        }

        fn h_full<T: Into<Bytes>>(chunk: T) -> HBoxBody {
            http_body_util::Full::new(chunk.into())
                .map_err(|never| match never {})
                .boxed()
        }

        pub async fn destroy_vm_with_problem(&self,vm_name:String,err_text:String) -> anyhow::Result<()>{
            self.log(format!("VM {} problem: {}",vm_name ,err_text).as_str() )?;
            if let Some ( (finded_vm_name,finded_vm) ) = self.vms.lock().await.remove_entry(vm_name.as_str()){
                self.log(format!("sending stop signal to vm: {}",finded_vm_name).as_str())?;
                finded_vm.clone().lock().await.stop(err_text.clone().as_str()); 
            }
            Ok(())
        }

        pub async fn http_command_vms(&self,command: String, req_json: serde_json::Value )  -> anyhow::Result<(String,bool,bool,String,String)> { //vmname,success,should_drop,stdout,stderr
            #[derive(Deserialize, Debug)]
            struct HPGetSendStdIO{
                vm:String,
                data:Option<String>,
            }
            let params:HPGetSendStdIO = serde_json::from_value(req_json)?;
            let vms_hashmap=self.vms.lock().await;

            if let Some(finded_vm)=vms_hashmap.get(params.vm.as_str()){
                if command.eq("send_stdin") {
                    //send stdin
                    if let Some(stdin_data) = params.data {                        
                        if let Err(e) = finded_vm                                    
                        .clone()
                        .lock()
                        .await
                        .write_to_stdin(stdin_data)
                        .await {
                            Ok((params.vm,false,true,String::from(""),e.to_string().clone()))
                        }else{
                            Ok((params.vm,true,false,String::from(""),String::from("")))
                        }                        
                    }else{
                        bail!("stdin data not found")
                    }
                }else{
                    //get stdout
                    let stdout=finded_vm
                    .clone()
                    .lock()
                    .await
                    .read_from_stdout()
                    .await;
                    let stderr=finded_vm
                    .clone()
                    .lock()
                    .await
                    .read_from_stderr()
                    .await;
                    match stdout{
                        Err(stdout_e)=>{
                            return Ok((params.vm,false,true,String::new(),stdout_e.to_string() ));
                        },
                        Ok(stdout_o)=>{ 
                            match stderr{
                                Err(stderr_e)=>{
                                    return Ok((params.vm,false,true,String::new(),stderr_e.to_string()));
                                },
                                Ok(stderr_o)=>{ 
                                    return Ok((params.vm,true,false,stdout_o.clone(),stderr_o.clone()));
                                }
                            }
                        }
                    }
                }
                //end of vm found
            }else{
                bail!("VM not found")
            }
            
        }



        pub async fn http_server_fn_json_api(&self,req: hyper::Request<HIncomingBody>)  ->  HResult<hyper::Response<HBoxBody>> {
            let whole_body = req.collect().await?.aggregate();
            // Decode as JSON...
            let mut data: serde_json::Value = serde_json::from_reader(whole_body.reader())?;
//            let mut response_data:HashMap<String,String>=HashMap::new();

            let mut command_result=false;
            let mut command_result_err_text=String::new();


            if let Some(cmd) = data.get("cmd"){
                let cmd_s=cmd.as_str().unwrap().to_string();
                println!("CMD IS {}",cmd_s );

                match cmd_s.as_str() {
                    "send_stdin" | "get_stdout" => {
                        match self.http_command_vms(cmd_s.to_string(),data.clone()).await{
                            Err(e)=>{
                                command_result=false;
                                command_result_err_text=e.to_string();
                            },
                            Ok((vmname,success,should_drop,stdout,stderr))=>{
                                data["stdout"]=serde_json::Value::from(stdout);
                                data["stderr"]=serde_json::Value::from(stderr.clone());
                                command_result_err_text=stderr;
                                command_result=success;
                                if should_drop{
                                    self.destroy_vm_with_problem( vmname,command_result_err_text.clone() ).await?;
                                }
                            }
                        }
                    },

                    "create_vm"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPCreateVM{
                            vm:String,
                            exe:String,
                            params: Vec<String>,
                            envs: HashMap<String,String>,
                        }
                        let params:HPCreateVM = serde_json::from_value(data.clone())?;
                        match self.vm_add_app(
                                    params.vm,
                                    params.exe,
                                    params.params,
                                    params.envs
                                ).await{
                                    Ok(_o)=>{
                                        command_result=true;
                                    },
                                    Err(e)=>{
                                        command_result=false;
                                        command_result_err_text=e.to_string();
                                    },
                            }
                    },
                    "install_archive" | "install_deb" =>{
                        #[derive(Deserialize, Debug)]
                        struct HPInstallArchiveOrDeb{
                            url:String,
                            ignore_prefix:String,
                        }
                        let params:HPInstallArchiveOrDeb = serde_json::from_value(data.clone())?;

                        if cmd_s.as_str().eq("install_archive"){//tar(gz|xz)
                            self.install_archive_with_url(params.url,params.ignore_prefix.as_str() ).await?;
                            command_result=true;
                        }else{//deb
                            self.install_deb_with_url(params.url,params.ignore_prefix.as_str()).await?;            
                            command_result=true;
                        }                      
                    },
                    "chdir"=>{
                        if let Some(dir)=data.get("dir"){
                            if let Some(dir_s)=dir.as_str(){
                                std::env::set_current_dir(Path::new( self.replace_path_with_env(dir_s.to_string())?.as_str() ))?;
                                command_result=true;
                            }
                        }
                    },
                    "mkdir"=>{
                        if let Some(dir)=data.get("dir"){
                            if let Some(dir_s)=dir.as_str(){                       
                                std::fs::create_dir(Path::new( self.replace_path_with_env(dir_s.to_string())?.as_str() ))?;
                                command_result=true;
                            }
                        }
                    },
                    "termux_install_app"=>{
                        if let Some(app_name)=data.get("app"){
                            if let Some(app_name_s)=app_name.as_str(){        
                                self.termux_install_app(app_name_s.to_string()).await?;
                                command_result=true;
                            }
                        }
                    },
                    "docker_hub_pull"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPDockerHubPull{
                            container:String,
                            save_to: String,
                            arch:Option<String>,//exmpl: arm/v7 or arm
                        }
                        let params:HPDockerHubPull = serde_json::from_value(data.clone())?;

                        let replaced_path=self.replace_path_with_env(params.save_to)?;
                        let p=Path::new(replaced_path.as_str());
                        if !p.exists(){
                            tokio::fs::create_dir_all(p).await?;
                        }
                        
                        std::env::set_current_dir(p)?;

                        self.docker_hub_pull(params.container,params.arch).await?;
                        command_result=true;                        
                    },                   
                     "download_url_to_file"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPDownloadUrl{
                            url:String,
                            executable:bool,
                            save_to: String,
                        }
                        let params:HPDownloadUrl = serde_json::from_value(data.clone())?;
                        self.download_url_to_file(params.url,&params.save_to,params.executable).await?;
                        command_result=true;                  
                    }

                    _default=>{},
                }

            }

            data["command_result"]=if command_result{
                serde_json::Value::from("true")
            }else{
                serde_json::Value::from("false")
            };
            
            if !command_result{
                data["command_result_err"]=serde_json::Value::from(command_result_err_text);
            }


            let json = serde_json::to_string(&data)?;
            let response = hyper::Response::builder()
                .status(hyper::StatusCode::OK)
                .header(hyper::header::CONTENT_TYPE, "application/json")
                .body(Self::h_full(json))?;
            Ok(response)

        }

        pub async fn docker_hub_pull(&self,container_name:String,arch: Option<String>) ->anyhow::Result<()>{
            let container_name_spl:Vec<&str>=container_name.split(":").collect();
            if container_name_spl.len()!=2{
                bail!("docker_hub_pull container name should be name:version");
            }
            let mut cont_name=container_name_spl[0].to_string();
            let version=container_name_spl[1].to_string();
            if !cont_name.contains("/"){
                cont_name.insert_str(0,"library/" );
            }
            
            let mut docker_hub = crate::docker_hub::DockerHub::new(cont_name,version,arch);
            let (layers,container_conf_opt)=docker_hub.get_layers_urls(None).await?;

            for (layer_url,layer_format)  in layers{
                self.log(format!("unarchiving blob {} with type {}",layer_url,layer_format).as_str())?;
                let bin=docker_hub.download_blob(layer_url).await?;
                self.unarchive_compressed_tar(bin.reader(), "tar.gz", "").await?;
            }

            if let Some(cont_conf)=container_conf_opt{
                let json_str=serde_json::to_string(&cont_conf)?;
                tokio::fs::File::create(".container_info.json")
                    .await?
                    .write_all(
                        json_str.as_bytes()
                    ).await?;
            }
            Ok(())
        }

        pub async fn http_server_fn_hello(&self,req: hyper::Request<hyper::body::Incoming>)  ->  HResult<hyper::Response<HBoxBody>> {
            static NOTFOUND: &[u8] = b"Not Found";
            static INDEX: &[u8] = include_bytes!("index.html") ;
            match (req.method(), req.uri().path()) {
                (&Method::GET, "/") | (&Method::GET, "/index.html") => Ok(hyper::Response::new(Self::h_full(INDEX )  )   ),
//                (&Method::GET, "/test.html") => client_request_response().await,
                (&Method::POST, "/api.json") => {
                    match self.http_server_fn_json_api(req).await {
                        Ok(ok)=>{
                            return Ok(ok);
                        },
                        Err(er)=>{
                            return Ok(HResponse::builder()
                            .status(HStatusCode::INTERNAL_SERVER_ERROR)
                            .body(Self::h_full(er.to_string()))
                            .unwrap());
                        }
                    }
                }
                    ,
 //               (&Method::GET, "/json_api") => api_get_response().await,
                _ => {
                    // Return 404 not found response.
                    Ok(HResponse::builder()
                        .status(HStatusCode::NOT_FOUND)
                        .body(Self::h_full(NOTFOUND))
                        .unwrap())
                }
            }
//            Ok(hyper::Response::new(http_body_util::Full::new(Bytes::from("Hello World!"))))
        }

        async fn http_server_start(&self) -> anyhow::Result<()>{
//            use hyper
            use std::net::SocketAddr;
            use tokio::net::{TcpListener};
            use hyper::server::conn::{http1};
            use hyper::service::service_fn;

            let addr = SocketAddr::from(([0,0,0,0], 3000));
            let listener = TcpListener::bind(addr).await?;
            loop {
                let (stream, _) = listener.accept().await?;                
                    if let Err(err) = http1::Builder::new()
                        .serve_connection(stream,
                             service_fn(
                                move |req| self.http_server_fn_hello(req)
                             )
                            )
                        .await
                    {
                        println!("Error serving connection: {:?}", err);
                    }
            }            
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

