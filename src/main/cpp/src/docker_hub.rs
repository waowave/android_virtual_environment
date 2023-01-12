

use bytes::{Bytes};
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json;


pub struct DockerHub{
    token:String,
    auth_base_url:String,
    registry_base_url:String,
    container_name:String,
    container_version:String,
    container_current_platform_digest:String,
    container_layers:Vec<(String,String)>,
    container_arch:Option<String>,
}

#[derive(Deserialize, Debug)]
struct JManifestsPlatform{
    architecture:String,
    os:String,
    variant:Option<String>,
}

#[derive(Deserialize, Debug)]
struct JManifestsRow {
    digest: String,
//    mediaType: String,
    platform: JManifestsPlatform,
//    size:u32,
}

#[derive(Deserialize, Debug)]
struct JManifests{
    manifests:Option<Vec<JManifestsRow>>,
    layers:Option<Vec<JManifestLayersRow>>,
    config: Option<JManifestConfig>,
}

#[derive(Deserialize, Debug)]
struct JManifestLayersRow{
#[serde(rename = "mediaType")]
    media_type: String,
//    size:u32,
    digest:String,
}


#[derive(Deserialize, Debug)]
struct JManifestConfig{
 //   #[serde(rename = "mediaType")]
 //   media_type: String,
 //   size:u32,
    digest:String,
}

#[derive(Deserialize,Serialize, Debug)]
pub struct JConfigConfig{
    #[serde(rename = "Cmd")]
    pub cmd: Vec<String>,
    #[serde(rename = "Env")]
    pub env: Vec<String>,
    #[serde(rename = "WorkingDir")]
    pub working_dir: String,       
    #[serde(rename = "Entrypoint")]
    pub entrypoint: Option<Vec<String>>,       
}

#[derive(Deserialize, Debug)]
pub struct JConfig{
    config: JConfigConfig,
}

/*

#[derive(Deserialize, Debug)]
struct JManifest{
//    schemaVersion: i32,
    layers:Vec<JManifestLayersRow>,
}
*/


impl DockerHub{

    pub fn new(container_name:String,container_version:String,arch: Option<String>) -> Self{
        DockerHub{
            token:String::new(),
            auth_base_url:String::from("https://auth.docker.io"),
            registry_base_url:String::from("https://registry-1.docker.io"),
            container_name:container_name,
            container_version:container_version,
            container_current_platform_digest:String::new(),
            container_layers:Vec::new(),
            container_arch:arch,
        }
    }

    async fn prepare_reqwest_and_send(&self,url:&str)->anyhow::Result<reqwest::Response>{
        let mut client = reqwest::Client::new()
        .get(url)
        .header("Accept","application/vnd.docker.distribution.manifest.v2+json")
        .header("Accept","application/vnd.docker.distribution.manifest.list.v2+json");

        if !self.token.is_empty(){
            client=client.header("Authorization", format!("Bearer {}",self.token) );
        }

        let resp=client
        .send()
        .await?;

        return Ok(resp);
    }

    async fn get_json_response_using_auth_if_possible(&self,url:&str) -> anyhow::Result<serde_json::Value>{
            let resp=self
            .prepare_reqwest_and_send(url)
            .await?
            .text()
            .await?;
            let jdata: serde_json::Value = serde_json::from_str(resp.as_str())?;
            #[cfg(test)]
            println!("get_json_response_using_auth_if_possible({}) => jdata={}",url,jdata.clone());
            Ok(jdata)
    }

    async fn get_binary_response_using_auth_if_possible(&self,url:&str) -> anyhow::Result<Bytes>{
        let resp=self
        .prepare_reqwest_and_send(url)
        .await?
        .bytes()
        .await?;
        Ok(resp)
    }

    async fn get_token(&mut self)-> anyhow::Result<()>{
        let j = self.get_json_response_using_auth_if_possible(format!("{}/token?service=registry.docker.io&scope=repository:{}:{}:pull",self.auth_base_url,self.container_name,self.container_version).as_str()).await?;
        let j_token=j.get("token");
        if j_token.is_none(){bail!("token not found in response")}
        let j_token_s=j_token.unwrap().as_str();
        if j_token_s.is_none(){bail!("cant convert token to string")}        
        self.token=j_token_s.unwrap().to_string();
        Ok(())
    }


    async fn manifests_in_response(&mut self,manifests: Vec<JManifestsRow>)-> anyhow::Result<String>{

        for row in manifests{
            let  need_arch:String;
            let mut need_sub_arch:Option<String>=None;
            
            #[cfg(target_arch = "x86")]
            let local_arch="x86".to_string(); // ???

            #[cfg(target_arch = "x86_64")]
            let local_arch="amd64".to_string();

            #[cfg(target_arch = "arm")]
            let local_arch="arm".to_string();

            #[cfg(target_arch = "aarch64")]
            let local_arch="arm64".to_string();                    


            if let Some(arch)=&self.container_arch{
                let exp_arch:Vec<String>=arch.split("/").map(|f|f.to_string()).collect();
                match exp_arch.len() {
                    1=>{ need_arch=exp_arch[0].to_string(); },
                    2=>{ need_arch=exp_arch[0].to_string(); need_sub_arch=Some( exp_arch[1].to_string() ); },
                    _=>{bail!("wrong arch. should be arch/subarch or arch")},
                }
            }else{
                need_arch=local_arch;
            }

            if row.platform.os.eq("linux") && row.platform.architecture.eq(need_arch.as_str()) {
                if let Some(sub_arch)=need_sub_arch{
                    if let Some(variant_s)=row.platform.variant{
                        if variant_s.eq(sub_arch.as_str()){
                            self.container_current_platform_digest=row.digest.clone();
                            return Ok(row.digest);     
                        }
                    }
                }else{
                    self.container_current_platform_digest=row.digest.clone();
                    return Ok(row.digest);    
                }
            }
        }
        bail!("can't find supported manifest")
    }

    async fn layers_in_response(&mut self,layers: Vec<JManifestLayersRow>)-> anyhow::Result<Vec<(String,String)>>{
//        println!("ST1");
        let mut ret_vector:Vec<(String,String)>=vec![];
        for row in layers{
            //need real filename (format)
            let url=format!("{}/v2/{}/blobs/{}",self.registry_base_url,self.container_name,row.digest ).to_string();
            ret_vector.push(  (url,row.media_type.clone())  );
        }
        self.container_layers=ret_vector.clone();
//        println!("ST2");
        return Ok(ret_vector);
    }

    async fn config_in_response(&mut self,config: JManifestConfig)-> anyhow::Result<JConfigConfig> {
//        println!("ST3 a={} b={} c={}", self.registry_base_url,self.container_name,config.digest.clone() );
        let config_url=format!("{}/v2/{}/blobs/{}",self.registry_base_url,self.container_name,config.digest.clone() ).to_string();
//        println!("ST4 u ={}",config_url.clone());
        let j_config = self.get_json_response_using_auth_if_possible(config_url.as_str()).await?;
//        println!("ST5");
        let jc:JConfig=serde_json::from_value(j_config)?;
//        println!("ST6");
        Ok(jc.config)
    }
/*
={"architecture":"arm","config":{"AttachStderr":false,"AttachStdin":false,"AttachStdout":false,"Cmd":["bash"],"Domainname":"","Entrypoint":null,"Env":["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],"Hostname":"","Image":"sha256:961bfdd014e15bd42cc41ef8f9783f127646ba4ff0b08534e00e39c3c5a99f35","Labels":null,"OnBuild":null,"OpenStdin":false,"StdinOnce":false,"Tty":false,"User":"","Volumes":null,"WorkingDir":""},"container":"df974404959ede4d61409119b6ae69a7391cac593b32b53b6cb2d02e6e5d6bd0","container_config":{"AttachStderr":false,"AttachStdin":false,"AttachStdout":false,"Cmd":["/bin/sh","-c","#(nop) ","CMD [\"bash\"]"],"Domainname":"","Entrypoint":null,"Env":["PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"],"Hostname":"df974404959e","Image":"sha256:961bfdd014e15bd42cc41ef8f9783f127646ba4ff0b08534e00e39c3c5a99f35","Labels":{},"OnBuild":null,"OpenStdin":false,"StdinOnce":false,"Tty":false,"User":"","Volumes":null,"WorkingDir":""},"created":"2022-12-09T01:36:30.97736334Z","docker_version":"20.10.17","history":[{"created":"2022-12-09T01:36:30.299436884Z","created_by":"/bin/sh -c #(nop) ADD file:ca82b3c78a23b75345429f192c4b1f88b4e49e12808c85fccc2db04823c17d4e in / "},{"created":"2022-12-09T01:36:30.97736334Z","created_by":"/bin/sh -c #(nop)  CMD [\"bash\"]","empty_layer":true}],"os":"linux","rootfs":{"diff_ids":["sha256:be54527d989a703cfe5f7eac4c127972997ebdb991f79ce71eee00b8b92de1dc"],"type":"layers"},"variant":"v7"}
*/

    //self.container_version or latest
    pub async fn get_layers_urls(&mut self,last_parameter:Option<String>)-> anyhow::Result<(Vec<(String,String)>,Option<JConfigConfig>)>{
        let mut current_last_parameter;
        if let Some (last_parameter_unw) = last_parameter{
            current_last_parameter=last_parameter_unw;
        }else{
            current_last_parameter=self.container_version.clone();
        }
        if self.token.is_empty(){self.get_token().await?}
        loop{
            let url=format!("{}/v2/{}/manifests/{}",self.registry_base_url,self.container_name,current_last_parameter);
//            println!("dbg url = {}",url.clone());
            let j_base_manifests = self.get_json_response_using_auth_if_possible(url.as_str()).await?;
//            println!("dbg={}",j_base_manifests.clone());
            let manifests_dbg=j_base_manifests.clone();
            let response:JManifests = serde_json::from_value(j_base_manifests)?;
            if let Some(manifests) = response.manifests {
                println!("manifests in response");
                let needed_manifest=self.manifests_in_response(manifests).await?;
                current_last_parameter=needed_manifest;
                //continue;
            }else if let Some(layers) = response.layers {
                println!("layers in response");
                let mut downloaded_cfg=None;
                if let Some(config) = response.config {
                    downloaded_cfg=Some(self.config_in_response(config).await?);
                }
                return Ok((self.layers_in_response(layers).await?,downloaded_cfg));
            }else{
                bail!("nothing in manifest response. dbg={}",manifests_dbg)
            }
        }
    }




    //execute unpacking func 
    pub async fn download_blob(&mut self, layer_url:String) -> anyhow::Result<Bytes>{
        let layer:bytes::Bytes=self.get_binary_response_using_auth_if_possible(layer_url.as_str()).await?;
        Ok(layer)
    }

    /* 
    pub async fn download_blobs<F,Fut>(&mut self, f:F  ) -> anyhow::Result<()>
    where 
    F: Fn(Bytes,String)->Fut,
    Fut: Future<Output = anyhow::Result<()> >,
     { 
            let layers=self.get_layers_urls(None).await?;
            for (layer_url,layer_format) in layers{
                let layer:bytes::Bytes=self.get_binary_response_using_auth_if_possible(layer_url.as_str()).await?;
                f(layer,layer_format).await?;
            }
            Ok(())
    }
    */


}


#[cfg(test)]
mod tests {
    use crate::docker_hub::DockerHub;
//    #[test]
    #[tokio::test]
    async fn it_works() {
        let mut app = DockerHub::new( "library/node".to_string(),"slim".to_string(),Some("arm/v7".to_string()));
//        let mut app = DockerHub::new( "library/ubuntu".to_string(),"latest".to_string(),Some("arm/v7".to_string()));
        let layers=app.get_layers_urls(None).await.unwrap();
        println!("layers={:?}",layers);
    }
}
 



/*
adb shell am start -a android.settings.SETTINGS

To bring up developer settings (in Gingerbread at least):

adb shell am start -a com.android.settings.APPLICATION_DEVELOPMENT_SETTINGS


*/


/*
ref="${1:-library/ubuntu:latest}"
sha="${ref#*@}"
if [ "$sha" = "$ref" ]; then
  sha=""
fi
wosha="${ref%%@*}"
repo="${wosha%:*}"
tag="${wosha##*:}"
if [ "$tag" = "$wosha" ]; then
  tag="latest"
fi
api="application/vnd.docker.distribution.manifest.v2+json"
apil="application/vnd.docker.distribution.manifest.list.v2+json"
token=$(curl -s "https://auth.docker.io/token?service=registry.docker.io&scope=repository:${repo}:pull" \
        | jq -r '.token')
curl --verbose -H "Accept: ${api}" -H "Accept: ${apil}" \
     -H "Authorization: Bearer $token" \
     -s "https://registry-1.docker.io/v2/${repo}/blobs/sha256:c38006c9acc492149d706593acba951110798e57a7ad05103ae7a2d5969c14b6"

#     -s "https://registry-1.docker.io/v2/${repo}/manifests/sha256:ea8f467d512068a1e52494d5b2d959a9307a35682633d0b5d481e79c914c627f" | jq .
#     -s "https://registry-1.docker.io/v2/${repo}/manifests/${sha:-$tag}" | jq .
*/