/*
 *   Copyright (c) 2020 
 *   All rights reserved.
 */
use reqwest::blocking::Client;
use reqwest::Method;
use reqwest::header::USER_AGENT;

static RIB_AGENT:&'static str = "";

fn main() -> Result<(), reqwest::Error>{

    let client = Client::new();
    let builder = client.request(Method::GET, "https://api.github.com/repos/nervosnetwork/ckb/pulls");
    let builder = builder.header(USER_AGENT, RIB_AGENT);
    let body = builder.send()?.text()?;



   // let body = reqwest::blocking::get("https://api.github.com/repos/nervosnetwork/ckb/pulls")?.text()?;
    println!("body = {:?}", body);
    
    //println!("test");
    return Ok(());

}



