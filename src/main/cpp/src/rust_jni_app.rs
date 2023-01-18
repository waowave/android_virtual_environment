pub mod vm;
pub mod docker_hub;

//pub mod rust_jni_app{

    type HGenericError = Box<dyn std::error::Error + Send + Sync>;
    type HResult<T> = std::result::Result<T, HGenericError>;
    type HBoxBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;
    type ArcMutex<T> = Arc<tokio::sync::Mutex<T>>;

    use anyhow::bail;
    use hyper::{body::Incoming as HIncomingBody, StatusCode as HStatusCode, Response as HResponse};
    use http_body_util::BodyExt;
    use serde::{Deserialize, Serialize};
    use tokio::io::{AsyncWriteExt, AsyncReadExt};
    use std::{io::prelude::*, time::Duration, str::FromStr, collections::HashSet};

    use bytes::{Bytes, Buf};
    use hyper::Method;
    use std::{collections::HashMap, marker::PhantomData, path::Path, sync::Arc};
    use self::{vm::VM,  docker_hub::{JConfigConfig, DockerHub}};


    #[cfg(not(target_os = "macos"))]
    use nix::mount::{mount, umount, MsFlags};


    #[cfg(feature = "jni")]
    use jni::{JNIEnv, sys::{jobject, JNINativeInterface_}, objects::{JObject}};

//    use self::docker_hub::DockerHub;

    pub struct RustAppInsideJNI<'a>{
        #[cfg(feature = "jni")]
        jni_nativerunnable_obj: JObject<'a>,
        #[cfg(feature = "jni")]
        jni_environment: JNIEnv<'a>,
        phantom: PhantomData<&'a String>,
        packages_map:tokio::sync::Mutex<HashMap<String, HashMap<String,String> >>,
        vms:  ArcMutex<HashMap<String,ArcMutex<VM>>>,
        files_dir:std::sync::Mutex<Option<String>>,
    }

    #[derive(Debug)]
    pub struct UnarchiveTarResult{
        pub dirs_mode_uid_gid: HashMap<String,(u32,u64,u64)>,//dir mode, uid, gid
        pub files: HashSet<String>,
        pub unpacked_entries:HashSet<String>,
        pub errors:Vec<String>,
    }




    #[derive(Deserialize,Serialize, Debug)]
    struct ContainerConfigJSON{ //%FILES%/containers/container_name.json
        vm_path:String,
        chroot_mode:String,
        volumes:Option<HashMap<String,String>>,
        envs:Option<HashMap<String,String>>,
        entrypoint:Option<Vec<String>>,
        cmd:Option<Vec<String>>,
        start_on_boot:Option<bool>,
        workdir:Option<String>,
        docker_hub:Option<HPDockerHubPull>,
    }

    #[derive(Deserialize,Serialize, Debug)]
    struct HPRunContainer{
        container:String, // in %FILES%/containers/cont.json
    }                        

    #[derive(Deserialize, Serialize, Debug)]
    struct HPDockerHubPull{
        image:String,
        save_to: String,
        arch:Option<String>,//exmpl: arm/v7 or arm
    }



    impl<'a> RustAppInsideJNI<'a> {
        #[cfg(feature = "jni")]
        pub fn new( jni: *mut *const JNINativeInterface_, thiz: jobject ) -> RustAppInsideJNI<'a> {
            RustAppInsideJNI{
                jni_nativerunnable_obj:unsafe{jni::objects::JObject::from_raw(thiz)},
                jni_environment:unsafe{JNIEnv::from_raw(jni).unwrap()},
                phantom:PhantomData,
                packages_map: tokio::sync::Mutex::new(HashMap::new()),
                vms: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                files_dir:std::sync::Mutex::new(None),
            }
        }

        #[cfg(not(feature = "jni"))]
        pub fn new() -> RustAppInsideJNI<'a> {
                RustAppInsideJNI{
                phantom:PhantomData,
                packages_map: tokio::sync::Mutex::new(HashMap::new()),
                vms: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                files_dir: std::sync::Mutex::new(None),
            }
        }


        pub fn log_string(&self,txt: String){
            self.log(txt.as_str());
        }

        pub fn log(&self,txt: &str){
            #[cfg(feature = "jni")]
            self.call_native_with_string(  txt  )?;
            #[cfg(not(feature = "jni"))]
            {
                #[cfg(feature = "kmsg_debug")]
                {
                    std::fs::write("/dev/kmsg_debug", txt).unwrap_or_default();
                    std::fs::write("/proc/bootevent", txt).unwrap_or_default();                
                }
                println!("{}",txt);

            }
           
        }


        fn get_files_path (&self) -> anyhow::Result<String>{
           if let Ok(files_dir_mtx)=self.files_dir.lock(){
                if let Some::<String>(files_dir)=&*files_dir_mtx {
                    return Ok(files_dir.clone());
                }else{
                    bail!("files_dir is empty?")
                }
            }else{
                bail!("files_dir lock err")
            }
            

//            Ok(self.files_dir.lock().unwrap().unwrap().clone())
            /*
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
             */
        }

        #[cfg(feature = "jni")]
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

        #[cfg(feature = "jni")]
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
            self.log("done. trying unarchive");

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



        async fn unarchive_tar<R: Read> (&self, reader:R, ignore_prefix:&str,test_archive: bool, unarchive_only: Option<&HashSet<String>>  )-> anyhow::Result<UnarchiveTarResult>{ 
            self.log_string(format!("unarchiving tar...: "));
            let mut ret = UnarchiveTarResult{
                dirs_mode_uid_gid: HashMap::new(),
                files: HashSet::new(),
                unpacked_entries: HashSet::new(),
                errors: Vec::new(),
            };

//            fn get_entry_info( e: &tar: ) -> (bool,(u32,u64,u64)){
//            }


            let mut tar_decoder=tar::Archive::new(reader);
//            tar_decoder
//                .entries()?
//                .filter_map(|e| e.ok()).collect();

                for entry_r in tar_decoder.entries()? {                    
                    if let Ok(mut entry)=entry_r{
                        let orig_path=entry.path();//.unwrap().to_owned();
                        if orig_path.is_err() {continue ;}
                        let orig_path=orig_path.unwrap();
                        let orig_path_string=orig_path.to_str().unwrap().to_string();
    
                        let path = orig_path.strip_prefix(ignore_prefix).unwrap_or(&orig_path).to_owned();
    
                        let allow_unarchive_this_file;
                        if let Some(hs)= unarchive_only{
                            if hs.contains(&orig_path_string){
                                allow_unarchive_this_file=true;
                            }else{
                                allow_unarchive_this_file=false;
                            }
                        }else{
                            allow_unarchive_this_file=true;
                        }
    
                        if allow_unarchive_this_file {
    
                            let entrh=entry.header();
                            if entrh.entry_type().is_dir(){
                                let mut dir_mode=0o755;
                                let mut dir_gid=0;
                                let mut dir_uid=0;
                                if let Ok(dir_mode_r)=entrh.mode(){
                                    dir_mode=dir_mode_r;
                                }
                                if let Ok(dir_uid_r)=entrh.uid(){
                                    dir_uid=dir_uid_r;
                                }
                                if let Ok(dir_gid_r)=entrh.gid(){
                                    dir_gid=dir_gid_r;
                                }
                                
                                ret.dirs_mode_uid_gid.insert(orig_path_string.clone(),  (dir_mode, dir_uid, dir_gid) );
                            }else{
                                ret.files.insert(orig_path_string.clone());
                            }
    
    
                            if !test_archive{
                            //}else{//test archive
    //                            println!("path={:?} size={}",&path,entry.size());
                                if entrh.entry_type().is_file(){
                                    let mut read_buf:Vec<u8>=Vec::new();
                                    let unpack_res=entry.read_to_end(&mut read_buf);
                                    match unpack_res{
                                        Err(e)=>{
                                            ret.errors.push( format!("read_to_end = {:?} filename={}",&e, path.display() ) );
                                        },
                                        Ok(_o_)=>{
                                            let tw_res=tokio::fs::write(&path, read_buf).await;
                                            if let Err(e)=tw_res{
                                                ret.errors.push( format!("write(async) = {:?} filename={}",&e, &path.display() ) );
                                            }
                                        }
                                    }
                                }else{//is file
                                    entry.set_preserve_mtime(true);
                                    let unpack_res=entry.unpack(&path);
                                    match unpack_res{
                                        Err(e)=>{  
                                            ret.errors.push( format!("unpack = {:?} filename={}",&e, path.display() ) );
                                        },
                                        Ok(_o_)=>{                                 
                                            ret.unpacked_entries.insert(orig_path_string);
                                        },
                                    }
                                }//is file
                            }//not test archive                       
                        }//allow unarchive this file
    
                    }
                }


//                .for_each(|mut entry| { //-> Option<std::path::PathBuf> 
//                });
                self.log_string(format!("unarchiving tar done. "));
                Ok(ret)
        }

        async fn unarchive_compressed_tar<R: Read> (&self, reader:R, format: &str,ignore_prefix:&str,test_archive: bool, unarchive_only: Option<&HashSet<String>>)-> anyhow::Result<UnarchiveTarResult>{ 
            let ret;
            match format{
                "tar.xz"=>{
                    //let tar_decoded=XzDecoder::new(reader);
                    self.log_string(format!("trying unarchive as XZ: "));

                    let tar_decoded=xz::read::XzDecoder::new(reader);                    
                    ret=self.unarchive_tar(tar_decoded,ignore_prefix,test_archive,unarchive_only).await?;
                },
                "tar.gz" | "tar.gzip"=>{
                    self.log_string(format!("trying unarchive as GZ: "));
                    let tar_decoded=flate2::read::GzDecoder::new(reader);
                    ret=self.unarchive_tar(tar_decoded,ignore_prefix,test_archive,unarchive_only).await?;
                }

//                "tar.xz"=>{tar_decoded=xz::read::XzDecoder::new(reader);}
//                "tar.gz"=>{tar_decoded=async_compression::tokio::bufread::GzipDecoder::new(reader);}
                _ =>{
                    bail!("unknown format for unarchive_compressed_tar ({})",format)
                }
            }
//            let xz_decoder=xz::read::XzDecoder::new(reader);


            self.log_string(format!("Extracted the following files:"));


            Ok(ret)
        }


        async fn install_archive_with_url(&self, arc_url: String,ignore_prefix:&str,test_archive: bool, unarchive_only: Option<&HashSet<String>>) -> anyhow::Result<UnarchiveTarResult>{
            self.log_string(format!("install_archive_with_url ur={}",&arc_url));
            
            let content =  self.load_binary_from_url(arc_url.clone()).await?;
            let ret;
            if arc_url.ends_with("tar.xz"){
                let endfmt="tar.xz";
                ret=self.unarchive_compressed_tar(content.reader(),endfmt,ignore_prefix,test_archive,unarchive_only).await?;
            }else if arc_url.ends_with("tar.gz"){
                let endfmt="tar.gz";
                ret=self.unarchive_compressed_tar(content.reader(),endfmt,ignore_prefix,test_archive,unarchive_only).await?;
            }else if arc_url.ends_with("tar"){
                ret=self.unarchive_tar(content.reader(),ignore_prefix,test_archive,unarchive_only).await?;
            }else{
                bail!("unknown format for install_archive_with_url ({})",arc_url)
            }

            Ok(ret)
        }

        async fn install_deb_with_url(&self, deb_url: String,ignore_prefix:&str) -> anyhow::Result<()>{
            self.log("loading ar binary");
            let content =  self.load_binary_from_url(deb_url).await?;
            let mut ar_archive = ar::Archive::new(content.reader() );

            while let Some(entry_result) = ar_archive.next_entry() {
                let  entry = entry_result?;
                // Create a new file with the same name as the archive entry:
                let pth=String::from_utf8(entry.header().identifier().to_vec())?;
                self.log_string(format!("finded file in deb: {}",pth.clone().as_str() ));

//                let mut file = std::fs::File::create(pth)?;
                // The Entry object also acts as an io::Read, so we can easily copy the
                // contents of the archive entry into the file:

//                let mut bf=vec![];
//                let bf_readed=(entry.read_to_end(&mut bf) )?;
//                let bfu8:&[u8]=&bf;
//                let buf_reader=tokio::io::BufReader::new(bfu8);

                if pth.contains("data.tar.xz") {
                    self.unarchive_compressed_tar(entry,"tar.xz",ignore_prefix,false,None).await?;
                }else if pth.contains("data.tar.gz") {
                    self.unarchive_compressed_tar(entry,"tar.gz",ignore_prefix,false,None).await?;
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
                    self.log_string(format!("app {} found in database. installing...",app_name ));
                    let deps=app_info.get("depends").unwrap_or( &deps_none );

                    let filename=app_info.get("filename").unwrap();
                    let url=format!("https://packages.termux.dev/apt/termux-main/{}",filename);
    
                    self.log_string(format!("DEPS={} url={}",deps,url));
    
                    self.install_deb_with_url(url,"./data/data/com.termux/files").await?;

                }else{
                    self.log_string(format!("app {} not found in database. can't install ",app_name));
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

            self.log_string(  format!("ATTEMPT TO DOWNLOAD termux database"));

            let packages_body = reqwest::get(packages_url)
            .await?
            .text()
            .await?;

            self.log_string(  format!("DOWNLOADED"));

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
            self.log_string(  format!("termux database packages len = {}",self.packages_map.lock().await.keys().len()));

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
            self.log_string(format!("settings current directory to {}", base_path_for_app ));

            let p =Path::new(&base_path_for_app);
            if !p.exists(){
                self.log_string(format!("{} not exists. attemp to create", base_path_for_app ));
                if let Err(e) = std::fs::create_dir_all(p) {
                    bail!("set_current_directory_to_files:: couldn't create base_path_for_app. reason={}",e)
                }
            }

            if let Err(e) = std::env::set_current_dir(Path::new(base_path_for_app.as_str())){
                bail!("set_current_directory_to_files:: can't set current directory. reason={}",e)
            }

            Ok(())
        }

        async fn  download_release_proot_if_needed(&self)->anyhow::Result<()>{
            self.log("download_release_proot_if_needed executed");

            let proot_tmp_dir=self.replace_path_with_env("%TEMP%/proot_tmp".to_string()) ?;
            std::env::set_var("PROOT_TMP_DIR",  proot_tmp_dir.clone() );

            let proot_tmp_path=Path::new(proot_tmp_dir.as_str());
            if !Path::new(proot_tmp_path).exists() {
                if let Err(e) = tokio::fs::create_dir_all(proot_tmp_path).await{
                     bail!("couldn't create proot_tmp_path. reason = {}", e ) 
                }
            }
            
            let containers_dir=self.get_containers_directory() ?;
            let containers_dir_path=Path::new( &containers_dir ) ;
            if !containers_dir_path.exists() {
                if let Err(e) = tokio::fs::create_dir_all(containers_dir_path).await{
                     bail!("couldn't create containers_dir_path. reason = {}", e ) 
                }
            }

            if let Err(e) = self.set_current_directory_to_files(){
                bail!("couldn't set current directory to files. reason={}",e);
            }

            if Path::new("bin/proot").exists() {return Ok(());}
            self.log("bin/proot not found");

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
                self.install_archive_with_url(down_url, "root",false,None).await?;
                //self.download_url_to_file(down_url, "proot",true).await?;
            }

            Ok(())
        }

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

            self.install_archive_with_url(format!("https://github.com/proot-me/proot-rs/releases/download/v0.1.0/proot-rs-v0.1.0-{}.tar.gz",proot_rs_archive_arch).to_string(), "",false,None).await?;
 
            Ok(())
        }

        pub fn set_files_directory_env_if_needed(&self){
            if self.files_dir.lock().unwrap().is_none() {
               self.set_files_directory_env(None);
            }
        }

        pub fn set_files_directory_env(&self,files_dir:Option<String>){
            #[cfg(feature = "jni")]
            let files_path_calculated=self.get_android_files_path().unwrap();
            #[cfg(not(feature = "jni"))]
            let files_path_calculated = std::env::current_dir().unwrap().to_str().unwrap().to_string();

            let files_path;
            if let Some(fp)=files_dir{
                files_path=fp;
            }else{
                files_path=files_path_calculated;
            }
            let mut locked_str=self.files_dir.lock().unwrap();
            *locked_str=Some(files_path.clone());

            if !Path::new(files_path.clone().as_str()).exists(){
                std::fs::create_dir_all(&files_path.clone()).unwrap();
            }
    }


        async fn wait_internet_connection(&self)->anyhow::Result<()>{
      //      pub const ADDRS: [&str; 2] = [
                // - http://clients3.google.com/generate_204
//                "clients3.google.com:80",
                // - http://detectportal.firefox.com/success.txt
  //              "detectportal.firefox.com:80",
    //        ];

        let client = reqwest::Client::builder()
            .gzip(true)
            .timeout(Duration::from_secs(10))
            .build()?;

            loop{
                self.log_string(format!("wait_internet_connection to firefox"));
                let res_res = 
                    client
                    .get("https://detectportal.firefox.com/success.txt")
                    .send()
                    .await;
                if let Ok(res)= res_res{
                    let res_tr=res.text().await;
                    if let Ok(res_t)= res_tr{
                        self.log_string(format!("wait_internet_connection to firefox return {}",res_t));
                        break;
                    }else{
                        self.log_string(format!("bad response?..."));
                    }
                }else{
                    self.log_string(format!("no internet?..."));
                }
                tokio::time::sleep(Duration::from_millis(30000)).await;
            }

            Ok(())
        }


        async fn app_loop_async_main(&mut self) -> anyhow::Result<()>{
            if let Err(e) = self.set_current_directory_to_files(){
                bail!("app_loop_async_main::set_current_directory_to_files exec err. reason={}",e);
            }
            if let Err(e) = self.wait_internet_connection().await{
                bail!("app_loop_async_main::wait_internet_connection exec err. reason={}",e);
            }
            if let Err(e) = self.download_release_proot_if_needed().await{
                bail!("app_loop_async_main::download_release_proot_if_needed exec err. reason={}",e);
            }
            if let Err(e) = self.download_release_proot_rs_if_needed().await{
                bail!("app_loop_async_main::download_release_proot_rs_if_needed exec err. reason={}",e);
            }
            if let Err(e) = self.run_needed_containers().await{
                bail!("app_loop_async_main::run_needed_containers exec err. reason={}",e);
            }
            if let Err(e) = self.http_server_start().await{
                bail!("app_loop_async_main::http_server_start exec err. reason={}",e);
            }
            Ok(())
        }

        /* 
        #[cfg(all(not(feature = "jni"),not(test)))]
        
        async fn app_loop_async_bin(&mut self) -> anyhow::Result<()>{
            self.set_current_directory_to_files()?;
            self.download_release_proot_if_needed().await?;
            self.http_server_start().await?;
            Ok(())
        }
        */

        
        #[cfg(test)]
        async fn app_loop_async_test(&mut self) -> anyhow::Result<()>{
            use std::time::Duration;

            use serde_json::json;
            use tokio::time::sleep;

            self.set_current_directory_to_files()?;
            self.download_release_proot_if_needed().await?;
            self.download_release_proot_rs_if_needed().await?;

//            docket_hub.download_blobs(async move |bytes,fmt| -> anyhow::Result<()> {                
//                arced_self_clone.clone().unarchive_compressed_tar(bytes.reader(), "tar.gz", "").await?;
//                Ok(())
//            }).await?;

            tokio::spawn(async move {
                sleep(Duration::from_millis(1000)).await;
                let client = reqwest::Client::new();
                let mut res;
                
                res = client.post("http://localhost:3001/api.json")
                .body(r#"{"cmd":"docker_hub_pull","image":"nodered/node-red:latest", "save_to":"%FILES%/vms/nodered", "arch":"arm/v7"} "#)
//                .body(r#"{"cmd":"docker_hub_pull","image":"koenkk/zigbee2mqtt:latest", "save_to":"%FILES%/vms/test_vm", "arch":"arm/v7"} "#)
//                .body(r#"{"cmd":"docker_hub_pull","image":"zigbee2mqtt/zigbee2mqtt-armv7:latest", "save_to":"%FILES%/vms/test_vm", "arch":"arm/v7"} "#)
                    .send()
                    .await
                    .unwrap()
                    .text()
                    .await
                    .unwrap();
                    println!("res={}",res);
                
                /* 
                let ccfg:ContainerConfigJSON = ContainerConfigJSON{
                    vm_path:"%FILES%/vms/test_vm".to_string(),
                    volumes:Some(HashMap::from([
                            ("%FILES%/data".to_string(),"/app/data".to_string())
                        ]                        
                    )),
                    docker_hub:None,
                    chroot_mode:"proot_cpp".to_string(),
                    start_on_boot:Some(true),
                    cmd:None,
                    entrypoint:None,
                    workdir:None,
                    envs:None,
                };            

                let save_container_json=json!({
                    "cmd":"save_container_json",
                    "container":"ctest",
                    "data":serde_json::to_string(&ccfg).unwrap() ,
                });
*/
                let save_container_json=r#"{"docker_hub":{"image":"nodered/node-red:latest-minimal","save_to":"","arch":"arm/v7"},"vm_path":"%FILES%/vms/nodered","chroot_mode":"chroot","volumes":{"/mnt/sdcard/nodered":"/data"},"envs":null,"entrypoint":null,"cmd":null,"start_on_boot":true,"workdir":null}"#;


                
                res = client.post("http://localhost:3001/api.json")
                .body(save_container_json.to_string() )
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
                println!("res={}",res);
                
                let run_container_json=json!({
                    "cmd":"run_container",
                    "container":"nodered",
                });
                


                res = client.post("http://localhost:3001/api.json")
                .body(run_container_json.to_string() )
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap();
                println!("res={}",res);

             

            });


            /*
            {
                    vm_path:"%FILES%/zigbee2mqtt2",
                    volumes:[],
                    envs[
                        test_env:"HELLO"
                    ],
                    start_on_boot:true,
            }

            */

            self.http_server_start().await?;
            Ok(())
        }



        #[tokio::main(flavor = "current_thread")]
        async fn app_loop_async(&mut self) -> anyhow::Result<()>{

            #[cfg(not(test))]
            self.app_loop_async_main().await?;
    
            #[cfg(test)]
            self.app_loop_async_test().await?;

            Ok(())
        }



        pub fn app_loop(&mut self) -> anyhow::Result<()> {
            self.set_files_directory_env_if_needed();
            let _future = self.app_loop_async()?;
            Ok(())
        }

        pub fn replace_path_with_env(&self,str:String )  -> anyhow::Result<String>{
            if str.is_empty() {
                bail!("path could'nt be empty");
            }

            let files_path=self.get_files_path()?;
            let mut outpath=str.replace("%FILES%", files_path.as_str() );

            let cache_path;
            let containers_path;
            let tmp_path;

            if Path::new("/mnt/sdcard/").exists(){
                tmp_path=format!("/mnt/sdcard/virtual_environment/tmp/");
                cache_path=format!("/mnt/sdcard/virtual_environment/cache/");
                containers_path=format!("/mnt/sdcard/virtual_environment/containers/");
            }else{
                tmp_path=format!("{}/temp/",&files_path);
                cache_path=format!("{}/cache/",&files_path);
                containers_path=format!("{}/containers/",&files_path);
            }

            outpath=outpath.replace("%CACHE%", cache_path.as_str()  );
            outpath=outpath.replace("%TMP%", tmp_path.as_str()  );
            outpath=outpath.replace("%TEMP%", tmp_path.as_str()  );
            outpath=outpath.replace("%CONTAINERS%", containers_path.as_str()  );

//            #[cfg(target_os = "android")]

            Ok(
                outpath
            )
        }

        pub async fn vm_add_app(&self,vm_name:String, exe:String,args:Vec<String>, envs_p: HashMap<String,String>) -> anyhow::Result<()>{            
            if vm_name.is_empty() || exe.is_empty(){
                bail!("vm_name and executable could'nt be empty");
            }
            
            let mut envs=envs_p.clone();
            let mut vm_mtx=self.vms.lock().await;

            if vm_mtx.contains_key(vm_name.as_str()) {
                std::mem::drop(vm_mtx);
                anyhow::bail!("vm with name {} already exists", vm_name)
            }
            let exe_replaced=self.replace_path_with_env(exe)?;
            let args_replaced:Vec<String>=args.iter().map(move |s| {self.log_string(format!("arg=[{}]",s)); return  if s.is_empty() { "".to_string() } else { self.replace_path_with_env(s.clone()).unwrap() }} ).collect();

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

                envs.values_mut().for_each(|f| { let new_f=self.replace_path_with_env(f.clone()).unwrap(); f.clear();f.push_str(new_f.as_str());   } );

//            let args_replaced:Vec<String>=args.iter().map(move |s| self.replace_path_with_env(s.clone()).unwrap()).collect();


            self.log_string( format!("starting vm exe={} args={:?} envs={:?}",exe_replaced.clone(),args_replaced.clone(),envs.clone()));

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
            if vm_name.is_empty() {
                bail!("vm_name could'nt be empty");
            }
            self.log_string(format!("VM {} problem: {}",vm_name ,err_text));
            if let Some ( (finded_vm_name,finded_vm) ) = self.vms.lock().await.remove_entry(vm_name.as_str()){
                self.log_string(format!("sending stop signal to vm: {}",finded_vm_name));
                finded_vm.clone().lock().await.stop(err_text.clone().as_str()).await; 
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



        /// Copy files from source to destination recursively.
        /// thx: https://nick.groenen.me/notes/recursively-copy-files-in-rust/
        /// thx: https://stackoverflow.com/questions/26958489/how-to-copy-a-folder-recursively-in-rust
        /// 
        fn copy_recursively(&self,source: impl AsRef<Path>, destination: impl AsRef<Path>) -> std::io::Result<(u64,u64,u64)> {
            let mut files_copied:u64=0;
            let mut dirs_copied:u64=0;
            let mut bytes_copied:u64=0;
            std::fs::create_dir_all(&destination)?;
            for entry in std::fs::read_dir(source)? {
                let entry = entry?;
                let filetype = entry.file_type()?;
                if filetype.is_dir() {
                    self.copy_recursively(entry.path(), destination.as_ref().join(entry.file_name()))?;
                    dirs_copied=dirs_copied+1;
                } else {
                    files_copied=files_copied+1;
                    bytes_copied=bytes_copied+entry.metadata()?.len();
                    std::fs::copy(entry.path(), destination.as_ref().join(entry.file_name()))?;
                }
            }
            Ok((files_copied,dirs_copied,bytes_copied))
        }

        fn get_containers_directory(&self)->anyhow::Result<String>{
            Ok(self.replace_path_with_env("%CONTAINERS%/".to_string())?)
        }

        fn get_container_config_filename(&self,container:&str)->anyhow::Result<String>{
            if container.is_empty(){
                bail!("container name could'nt be empty")
            }
            Ok(format!("{}/{}.json", self.get_containers_directory()? ,container).to_string())
        }


        async fn run_needed_containers(&self)->anyhow::Result<()>{
        let  rdr= tokio::fs::read_dir( self.get_containers_directory()? ).await;
        if rdr.is_err(){ return Ok(());}
        let mut rd=rdr.unwrap();
        while let Ok(dir_entry) = rd.next_entry().await {
                if dir_entry.is_none(){break;}
                let dir_entry=dir_entry.unwrap();
                let dir_path=dir_entry.path();
                let stem=dir_path.file_stem();
                if stem.is_none(){continue;}
                let stem=stem.unwrap().to_str();
                if stem.is_none(){continue;}
                let stem=stem.unwrap();
                let jfile=tokio::fs::read(&dir_path).await;
                if jfile.is_err(){continue;}
                let container_conf = serde_json::from_slice::<ContainerConfigJSON>(&jfile.unwrap());
                if container_conf.is_err(){continue;}
                let container_conf=container_conf.unwrap();
                if container_conf.start_on_boot.is_none() {continue;}
                if container_conf.start_on_boot.unwrap(){
                    self.log_string(format!("running container {}",stem));
                    let hpr=HPRunContainer{
                        container: String::from_str(stem)?,
                    };
                    let run_cont_res = self.httpfunc_run_container(hpr).await;
                    match run_cont_res{
                        Ok(o)=>{ self.log_string(format!("running container {} result = {}", stem,o  )) },
                        Err(e)=>{ self.log_string(format!("running container {} error = {}", stem,e  )) },
                    }        
                }
            }
            self.log("run_needed_containers OK");
            Ok(())
        }



        async fn httpfunc_docker_hub_pull(&self,params:HPDockerHubPull)-> anyhow::Result<bool>{
            let replaced_path=self.replace_path_with_env(params.save_to)?;
            let p=Path::new(replaced_path.as_str());
            if !p.exists(){
                tokio::fs::create_dir_all(p).await?;
            }
            
            std::env::set_current_dir(p)?;

            self.docker_hub_pull(params.image,params.arch).await?;

            Ok(true)
        }
        
        async fn httpfunc_run_container(&self,params:HPRunContainer) -> anyhow::Result<bool>{
            let json_filename= self.get_container_config_filename(&params.container)?;
            let json_file=tokio::fs::read(json_filename).await?;
            let container_conf:ContainerConfigJSON=serde_json::from_slice(&json_file)?;

            let fs_vm_path=self.replace_path_with_env(container_conf.vm_path.clone())?;
            if ! Path::new(&fs_vm_path).exists(){
                self.log_string(format!("run_container... path: {} not found", &fs_vm_path ));                
                if let Some(mut docker_hub_cfg) = container_conf.docker_hub{
                    docker_hub_cfg.save_to=fs_vm_path.clone();
                    self.httpfunc_docker_hub_pull( docker_hub_cfg ).await?;
                }
            }
            
            let mut envs:HashMap<String,String> = HashMap::new();
            let mut create_vm_params:Vec<String>=Vec::new();

            envs.insert("SHELL".to_string(), "/bin/sh".to_string() );
            envs.insert("HOME".to_string(), "/root".to_string() );
            envs.insert("USER".to_string(), "root".to_string() );
            envs.insert("TMPDIR".to_string(), "/var/tmp".to_string() );
            envs.insert("PATH".to_string(), "/usr/bin:/bin:/sbin:/usr/bin:/usr/sbin:/usr/local/bin:/usr/local/sbin".to_string() );
            

            enum VmRunMode{
                ProotRS,
                ProotCPP,
                Chroot,
            }

            let vm_mode;
            
            vm_mode=match container_conf.chroot_mode.as_str(){
                "proot_rs" => VmRunMode::ProotRS,
                "proot_cpp" => VmRunMode::ProotCPP,
                "chroot" => VmRunMode::Chroot,
                _=> bail!("incorrect chroot mode") //VmRunMode::Chroot,
            };


            let create_vm_exe;
            let mut mounting_points=HashMap::new();
        
            ["/dev","/proc","/sys","/mnt"]
                .iter()
                .for_each(|&f| {
                    mounting_points.insert(f.clone().to_string(),f.clone().to_string());
                }
            );


            if let Some(vols)=container_conf.volumes {
                for (vol_k,vol_v) in vols {
                    let vol_path_str=self.replace_path_with_env(vol_k)?;
                    //let vol_path=Path::new(&vol_path_str);
                    mounting_points.insert(vol_path_str.to_string(),vol_v.clone());                                                                
                }
            }

            let mut should_umount_when_err: Vec<String>=Vec::new();

#[cfg(not(test))]
            for (vol_from,vol_to) in mounting_points {

                let vol_from_path=Path::new(vol_from.as_str());
                if !vol_from_path.exists(){
                    tokio::fs::create_dir_all(vol_from_path).await?;
                }

                let vol_to_in_real_fs=vec![ fs_vm_path.as_str(),  vol_to.as_str() ].join("/");
                let vol_to_path_in_real_fs=Path::new(vol_to_in_real_fs.as_str());
                if !vol_to_path_in_real_fs.exists(){
                    tokio::fs::create_dir_all(vol_to_path_in_real_fs).await?;
                }

                self.log_string(format!("mounting {} to {}",&vol_from,&vol_to_in_real_fs));

                match vm_mode{
                    VmRunMode::ProotRS | VmRunMode::ProotCPP => {
                        create_vm_params.push("-b".to_string());
                        create_vm_params.push(format!("{}:{}",vol_from,vol_to).to_string());            
                    },
                    #[cfg(target_os = "macos")]
                    VmRunMode::Chroot => {}

                    #[cfg(not(target_os = "macos"))]
                    VmRunMode::Chroot => {
                        //try umount if mounted
                        
                        let ures=umount(vol_to_path_in_real_fs);
                        if let Err(uer)=ures{
                            self.log_string(format!("umount(if mounted) => ({}) err = {}", &vol_to_in_real_fs ,uer ));
                        }

                        //mounting                                    
                        let mount_res = mount(
                            Some( vol_from_path ),
                            vol_to_path_in_real_fs,
                            None::<&Path>,
                            MsFlags::MS_BIND,
                            None::<&Path>,
                        );

                        match mount_res{
                            Ok(_o)=>{
                                should_umount_when_err.push(vol_to_in_real_fs);
                            },
                            Err(e)=>{
                                self.log_string(format!("mount err={}",e));
                                for umount_path in should_umount_when_err {
                                    let p=Path::new(&umount_path);
                                    let ures=umount(p);
                                    if let Err(uer)=ures{
                                        self.log_string(format!("umount({}) err = {}", &umount_path ,uer ));
                                    }
                                }
                                break;
                            },
                        }
                    }                                    
                }
            }

            //pass path with vm to parameters

            match vm_mode{
                VmRunMode::ProotRS | VmRunMode::ProotCPP => {
                    create_vm_params.push("-r".to_string());
                    create_vm_params.push( fs_vm_path  );       
                },
                VmRunMode::Chroot => {
                    create_vm_params.push( fs_vm_path  );
                },
            }


            let mut create_vm_params_for_bash=Vec::<String>::new();
            
            if let VmRunMode::Chroot = vm_mode {
                create_vm_params.push( "sh".to_string() );
                create_vm_params.push( "-c".to_string() );
                create_vm_params_for_bash.push( "echo 'chroot from xao'".to_string() );
            }



            let workdir;

            let mut dockerhub_config=None;

            let dockerhub_config_file=self.replace_path_with_env( format!("{}/.container_info.json",container_conf.vm_path.clone()).to_string() )?;
            if let Ok(dockerfile_config_file_data)=tokio::fs::read(&dockerhub_config_file).await {
                let dockerhub_config_d:JConfigConfig=serde_json::from_slice(  &dockerfile_config_file_data )?;
                dockerhub_config=Some(dockerhub_config_d);
            }
            //workdir
            let default_workdir="/".to_string();

            if let Some(workdir_c)=container_conf.workdir {
                workdir=workdir_c.clone();
            }else{
                if let Some(dcfg)=&dockerhub_config {
                    if ! dcfg.working_dir.is_empty(){
                        workdir=dcfg.working_dir.clone();
                    }else{
                        workdir=default_workdir;
                    }
                }else{
                    workdir=default_workdir;
                }
            }

            //setting workdir as parameter
            match vm_mode{
                VmRunMode::ProotRS | VmRunMode::ProotCPP => {
                    create_vm_params.push("-w".to_string());
                    create_vm_params.push( workdir );
                },
                VmRunMode::Chroot => {
                    create_vm_params_for_bash.push("&& cd".to_string());
                    create_vm_params_for_bash.push( workdir );
                },
            }




            //envs 
            //from dockerhub_conf
            if let Some(dcfg)=&dockerhub_config {
                dcfg.env.iter().for_each(|f|{
                        let dcfg_env_spl:Vec<&str> = f.split("=") .collect();
                        if dcfg_env_spl.len()==2{
                            envs.insert(dcfg_env_spl[0].to_string(), dcfg_env_spl[1].to_string());
                        }                                    
                    }
                );
            }

            //from container conf
            if let Some(envs_c)=container_conf.envs {
                for (env_k,env_v) in envs_c{
                    envs.insert(env_k.clone(), env_v.clone());
                }                            
            }

            //entrypoints and cmd 
            let mut entrypoint=vec!["/bin/sh".to_string()];
            let mut cmd=vec![];
            //getting entrypoints and cmd from dockerhub conf
            if let Some(dcfg)=&dockerhub_config{
                if let Some(dcfg_ep)=&dcfg.entrypoint{
                    entrypoint=dcfg_ep.clone();
                }
                if let Some(dcfg_cmd)=&dcfg.cmd{
                    cmd=dcfg_cmd.clone();
                }
            }
            //replacing entrypoint and if they exists in container conf

            if let Some(ccfg_ep)=&container_conf.entrypoint{
                entrypoint=ccfg_ep.clone();
            }
            if let Some(ccfg_cmd)=&container_conf.cmd{
                cmd=ccfg_cmd.clone();
            }

            match vm_mode{
                VmRunMode::ProotRS=>{
                    create_vm_params.push("--".to_string());
                    create_vm_exe=self.replace_path_with_env("%FILES%/proot-rs".to_string())?;
                },
                VmRunMode::ProotCPP=>{
                    create_vm_params.push("--kill-on-exit".to_string());
                    create_vm_params.push("-0".to_string());
                    create_vm_exe=self.replace_path_with_env("%FILES%/bin/proot".to_string())?;
                },
                VmRunMode::Chroot=>{
                    create_vm_exe="chroot".to_string();

                }

            }

            //cmd and entrypoint

            match vm_mode{
                VmRunMode::ProotRS | VmRunMode::ProotCPP => {
                    create_vm_params.extend(entrypoint);
                    create_vm_params.extend(cmd);
                },
                VmRunMode::Chroot => {
                    create_vm_params_for_bash.push("&& ".to_string());
                    create_vm_params_for_bash.extend(entrypoint);
                    create_vm_params_for_bash.extend(cmd);

                    create_vm_params.push(
                        create_vm_params_for_bash.join(" ")
                    )
                },
            }


            self.vm_add_app(
                format!("container_{}",params.container).to_string(),
                create_vm_exe,
                create_vm_params,
                envs
            ).await?;


            Ok(true)
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

                    "get_vms_list"=>{
                        let vms :Vec<String>  = self.vms.lock().await.clone().into_keys().collect();
                        data["vms"]=serde_json::Value::from(vms);                        
                    },

                    "dir_copy"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPDirCopy{
                            src_dir:String,
                            dest_dir:String,
                        }
                        let params:HPDirCopy = serde_json::from_value(data.clone())?;
                        let real_path_src=self.replace_path_with_env(params.src_dir)?;
                        let real_path_dest=self.replace_path_with_env(params.dest_dir)?;

                        let (files_copied,dirs_copied,bytes_copied) = self.copy_recursively(real_path_src.as_str(), real_path_dest.as_str())?;
                        
                        data["files_copied"]=serde_json::Value::from(files_copied);                        
                        data["dirs_copied"]=serde_json::Value::from(dirs_copied);                        
                        data["bytes_copied"]=serde_json::Value::from(bytes_copied);   
                        command_result=true;                     
                    },


                    "remove_fs_path"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPRmDirOrFile{
                            path:String,
                        }
                        let params:HPRmDirOrFile = serde_json::from_value(data.clone())?;
                        let real_path=self.replace_path_with_env(params.path)?;

                        let rpath=Path::new(real_path.as_str());
                        if rpath.exists(){
                            if rpath.is_dir(){
                                std::fs::remove_dir_all(rpath)?;
                            }else{
                                std::fs::remove_file(rpath)?;                                
                            }
                        }                
                        command_result=true;                     
                    
                    },

                    "save_container_json"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPSaveContainerJSON{
                            container:String,
                            data:String,
                        }
                        let params:HPSaveContainerJSON = serde_json::from_value(data.clone())?;
                        let container_filename=self.get_container_config_filename(&params.container)?;
                        tokio::fs::write(container_filename, params.data.as_bytes()  ).await?;
                        command_result=true;                     
                    
                    },
                    
                    "save_text_file"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPSaveTextFile{
                            save_to:String,
                            data:String,
                        }
                        let params:HPSaveTextFile = serde_json::from_value(data.clone())?;
                        let container_filename=self.replace_path_with_env(params.save_to)?;
                        tokio::fs::write(container_filename, params.data.as_bytes()  ).await?;
                        command_result=true;
                    },



                    "dir_list"=>{
                        #[derive(Deserialize, Debug)]
                        struct HPDirList{
                            dir:String,
                        }
                        let params:HPDirList = serde_json::from_value(data.clone())?;
                        let real_path=self.replace_path_with_env(params.dir)?;

                        let dirs:Vec<String>=std::fs::read_dir(  real_path.clone() )?.map(
                            |f|  
                            f
                            .unwrap()
                            .path()
                            .to_str()
                            .unwrap()
                            .strip_prefix(real_path.as_str())
                            .unwrap()
                            .to_string()
                        
                        ).collect();
                        data["dirs"]=serde_json::Value::from(dirs);           
                        command_result=true;                     
                    },

                    "run_container"=>{
                        let params:HPRunContainer = serde_json::from_value(data.clone())?;
                        let r = self.httpfunc_run_container(params).await?;
                        command_result=r;
                    }

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
                            self.install_archive_with_url(params.url,params.ignore_prefix.as_str(),false,None ).await?;
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
                        let params:HPDockerHubPull = serde_json::from_value(data.clone())?;
                        command_result=self.httpfunc_docker_hub_pull(params).await?;
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
            
            let mut docker_hub = DockerHub::new(
                cont_name,
                version,
                self.replace_path_with_env("%CACHE%/docker_hub".to_string())?,
                arch
                )?;
 
            let (layers,container_conf_opt)=docker_hub.get_layers_urls(None).await?;

            let mut start;
            
            

            //exception doubles
            let mut all_files_in_archives=0;
            let mut unarchive_file_list_with_archive_url:HashMap<String,String>=HashMap::new(); //filename / archive_url
            let mut unarchive_directories:HashMap<String,(u32,u64,u64)> = HashMap::new(); //dir_name, chmod,uid,gid
            for (layer_url,layer_format)  in &layers{
                start = std::time::Instant::now();

                self.log_string(format!("unarchiving blob {} with type {}",layer_url,layer_format));
                let bin=docker_hub.download_blob(layer_url.clone()).await?;
                let unarch_res = self.unarchive_compressed_tar(bin.reader(), "tar.gz", "",true,None).await?;
                unarch_res.files.iter().for_each(|file_name|{
                    unarchive_file_list_with_archive_url.insert(file_name.clone(), layer_url.clone() );
                });
                unarch_res.dirs_mode_uid_gid.iter().for_each(|(file_name,chmod_uid_gid)|{
                    unarchive_directories.insert(file_name.clone(), chmod_uid_gid.clone() );
                });
                all_files_in_archives=all_files_in_archives+unarch_res.files.len();
                println!("Time elapsed in list files in tar is: {:?}", start.elapsed() );
            }

            //making directories
            for (dir_name,(mode,uid,gid)) in unarchive_directories{
                if dir_name.contains("../"){continue;}

                let dir_path=Path::new(&dir_name);
                if !dir_path.exists(){
                    if let Err(e)= tokio::fs::create_dir_all(dir_path).await{
                        self.log_string(format!( "docker_hub_pull:: error creating directory {}. reason = {}",dir_path.display(),e) );
                    }else{
                        let perm = std::os::unix::prelude::PermissionsExt::from_mode(mode);
                        tokio::fs::set_permissions(dir_path, perm).await.unwrap_or_default();
                        nix::unistd::chown(dir_path,
                             Some(nix::unistd::Uid::from_raw(uid.try_into().unwrap_or(0))),
                             Some(nix::unistd::Gid::from_raw(gid.try_into().unwrap_or(0)))
                            )
                            .unwrap_or_default();
                    }
                }
            }

            //making hashmap with url -> set of files  (uniq)
            let mut unarchived_files_count=0;
            for (layer_url,layer_format)  in &layers{
                self.log_string(format!("unarchiving blob {} with type {}",layer_url,layer_format));
                start = std::time::Instant::now();
                let bin=docker_hub.download_blob(layer_url.clone()).await?;
                println!("Time elapsed in download current blob is {:?}", start.elapsed() );

                start = std::time::Instant::now();
                let unarchive_only:HashSet<String>=unarchive_file_list_with_archive_url.iter().filter_map(|(k,v)|{
                    return   if v.eq(layer_url.as_str()) {Some(k.clone())}else{None}
                } ).collect();
                println!("Time elapsed in hash set collection {:?}", start.elapsed() );
 
                start = std::time::Instant::now();
                let unarchived_list = self.unarchive_compressed_tar(bin.reader(), "tar.gz", "",false,Some(&unarchive_only)).await?;
                unarchived_files_count=unarchived_files_count+unarchive_only.len();
                println!("Time elapsed in unarchiving {:?} errors count={}", start.elapsed(),unarchived_list.errors.len());
                for errtxt in unarchived_list.errors{
                    println!("err={}",errtxt);
                }
            }




            self.log_string(format!("LAYERS files: all count={} unarchived count={}",all_files_in_archives, unarchived_files_count ));

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

            #[cfg(feature = "jni")]
            let port=3000;
            #[cfg(not(feature = "jni"))]
            let port=3001;

            let addr = SocketAddr::from(([0,0,0,0], port));
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
                        self.log_string(format!("Error serving connection: {:?}", err));
                    }
            }            
        }
    }
//}








#[cfg(test)]
mod tests {
    use crate::rust_jni_app::RustAppInsideJNI;
    #[test]
    #[cfg(not(feature = "jni"))]
    fn  it_works() {
        let mut app = RustAppInsideJNI::new();
        app.set_files_directory_env(Some("/Users/alex/AndroidStudioProjects/MyApplication/app/src/main/cpp/TESTING_TMP_DIR/".to_string()));
        app.app_loop().unwrap();
    }
}
 