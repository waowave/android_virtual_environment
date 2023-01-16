pub mod rust_jni_app;
use std::env;

use rust_jni_app::RustAppInsideJNI;
use serde::Deserialize;

pub fn main() ->anyhow::Result<()> {
    let mut app = RustAppInsideJNI::new();    
    std::fs::write("/proc/bootevent", "virtual env LOADED! (main executed)").unwrap_or_default();

    #[derive(Deserialize, Debug)]
    struct AppParamsJSON{
        files_dir:Option<String>,
    }

    let args: Vec<String> = env::args().collect();
    std::fs::write("/proc/bootevent", format!("{:?}",&args ) ).unwrap_or_default();

    if args.len()==2{
        let args_first=args.get(1).unwrap();
        std::fs::write("/proc/bootevent", "args len 2").unwrap_or_default();
        let params:AppParamsJSON = serde_json::from_str( args_first )?;
        std::fs::write("/proc/bootevent", "params decompilied").unwrap_or_default();
        if let Some(directory)=params.files_dir{
            std::fs::write("/proc/bootevent", format!("{:?}",&directory ) ).unwrap_or_default();
            app.set_files_directory_env(Some(directory));
            std::fs::write("/proc/bootevent", "set files ok").unwrap_or_default();
        }
    }    
    let app_loop_res=app.app_loop();
    if let Err(e)=app_loop_res{
        std::fs::write("/proc/bootevent", e.to_string()).unwrap();
    }
    Ok(())
}
