/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */

#![allow(unused)]

use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use reqwest::Method;
use std::{thread, time};
use structopt::StructOpt;
use chrono::{Date, DateTime, Local, Utc};

static RIB_AGENT: &'static str = "ribbot (Rust-in-Blockchain bot; Aimeedeer/ribbot; aimeedeer@gmail.com)";

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    Pulls {
        #[structopt(parse(try_from_str))]
        date: DateTime<Utc>,
    },
}

fn main() -> Result<()> {
    let options = Options::from_args();
    
    let client = Client::new();
    let repos = include_str!("github-repos.txt");

    for repo in repos.lines() {
        println!("{}", repo);
        let repourl = format!("https://api.github.com/repos/{}/pulls", repo);
        let builder = client.request(Method::GET, &repourl);
        let builder = builder.header(USER_AGENT, RIB_AGENT);
        let body = builder.send()?.json()?;
        println!("body = {:#?}", body);

        let one_second = time::Duration::from_millis(1000);
        thread::sleep(one_second);
    }

    return Ok(());
}
