use anyhow::bail;
//use http_body_util::BodyExt;
use tokio::{process::{Command,Child, ChildStdin}, io::{ BufWriter, AsyncWriteExt, BufReader, AsyncBufReadExt}, sync::Mutex, task::JoinHandle};
//use std::process::Stdio;
use std::{collections::HashMap, process::Stdio, sync::Arc};
//use std::io::prelude::*;

pub struct VM{    
    pub proc:Arc<Mutex<Child>>,
    stdout:Arc<Mutex<Vec<String>>>,
    stderr:Arc<Mutex<Vec<String>>>,
    writer: BufWriter<ChildStdin>,
    stop:Arc<Mutex<(bool,String)>>,
    join_handle_stdout_loop:Option<JoinHandle<()>>,
    join_handle_stderr_loop:Option<JoinHandle<()>>,
}

impl VM{
    pub async fn new(exe:String,args:Vec<String>, envs: HashMap<String,String>)->anyhow::Result<VM> {
        //.current_dir("/bin")
        let mut proc=Command::new(exe)
        .args(args)
        .envs(envs)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()? ;


        let child_stdin=proc.stdin.take().unwrap();

        let  writer  = BufWriter::new(child_stdin);

        let mut vm = VM{
            proc:Arc::new(Mutex::new(proc)),
            stdout: Arc::new(Mutex::new(Vec::new())),
            stderr: Arc::new(Mutex::new(Vec::new())),
            stop:Arc::new(Mutex::new( (false, String::new() ) )),
            writer:writer,
            join_handle_stdout_loop:None,
            join_handle_stderr_loop:None,
        };

        vm.run_background_stdout().await;
        vm.run_background_stderr().await;

        println!("returning VM...\n");

        Ok( vm )
    }

    pub async fn stop(&mut self,reason:&str){
        println!("stopping vm with reason {}",reason);
        let mut stop_mtx=self.stop.lock().await;
        *stop_mtx=(true,reason.to_string());

        if let Some(s)=&self.join_handle_stderr_loop{
            s.abort();
        }
        if let Some(s)=&self.join_handle_stdout_loop{
            s.abort();
        }
        self.join_handle_stderr_loop=None;
        self.join_handle_stdout_loop=None;
    }

    pub async fn is_stop(&self)->bool{
        self.stop.lock().await.0
    }

    pub async fn run_background_stdout(&mut self){
        let stdout_copy=self.stdout.clone();
        let self_proc_clone=self.proc.clone();
//        let self_stop_clone=self.stop.clone();
        self.join_handle_stdout_loop=Some(tokio::spawn(async move {
            let child_stdout = self_proc_clone.lock().await.stdout.take().unwrap();
            let mut reader = BufReader::new(child_stdout).lines();
                while let Some(line) = reader.next_line().await.unwrap() {                    
                    println!("Line: {} ", line);
                    stdout_copy.lock().await.push(line);
//                    if !self_stop_clone.lock().await.0{ break;}
                }
                //self.stop("run_background_stdout").await;
            println!("ending run_background_stdout")
        }));
    }
    

    pub async fn run_background_stderr(&mut self){
        let stderr_copy=self.stderr.clone();
        let self_proc_clone=self.proc.clone();
//        let self_stop_clone=self.stop.clone();
        self.join_handle_stderr_loop=Some(tokio::spawn(async move {
            let child_stderr = self_proc_clone.lock().await.stderr.take().unwrap();
            let mut reader = BufReader::new(child_stderr).lines(); 
                while let Some(line) = reader.next_line().await.unwrap() {                    
                    println!("Line: {} ", line);
                    stderr_copy.lock().await.push(line);
//                    if !self_stop_clone.lock().await.0{ break;}
                }
                println!("ending run_background_stderr")
                //self.stop("run_background_stdout");            
        }));
    }

    //paucci / пауччи

    pub async fn read_from_stdout(&mut self) -> anyhow::Result<String> {
        let mut stdout_array=self.stdout.lock().await;
        if  stdout_array.is_empty() && self.is_stop().await { bail!("read_from_stdout::empty and stopped") }
        let outgoing_string = stdout_array.join("\n");
        stdout_array.clear();
        Ok(outgoing_string)
    }

    pub async fn read_from_stderr(&mut self) -> anyhow::Result<String> {
        let mut stderr_array=self.stderr.lock().await;
        if  stderr_array.is_empty() && self.is_stop().await { bail!("read_from_stderr::empty and stopped") }
        let outgoing_string = stderr_array.join("\n");
        stderr_array.clear();
        Ok(outgoing_string)
    }


    pub async fn write_to_stdin(&mut self,s:String) -> anyhow::Result<bool> {
        if self.is_stop().await { bail!("write_to_stdin::vm stopped") }
        self.writer.write(s.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(true)
    }
    
}
