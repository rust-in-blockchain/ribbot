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
use chrono::{Date, DateTime, Local, Utc, NaiveDate};

static RIB_AGENT: &'static str = "ribbot (Rust-in-Blockchain bot; Aimeedeer/ribbot; aimeedeer@gmail.com)";

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    Pulls {
        #[structopt(parse(try_from_str = parse_naive_date))]
        date: NaiveDate,
    },
}

fn main() -> Result<()> {
    let options = Options::from_args();

    match options.cmd {
        Command::Pulls { date } => {
            fetch_pulls(date)?;
        }
    }

    Ok(())
}

fn fetch_pulls(date: NaiveDate) -> Result<()> {
    
    let client = Client::new();
    let repos = include_str!("github-repos.txt");

    for repo in repos.lines() {
        println!("<!-- fetching pulls for {} -->", repo);
        let repourl = format!("https://api.github.com/repos/{}/pulls", repo);
        let builder = client.request(Method::GET, &repourl);
        let builder = builder.header(USER_AGENT, RIB_AGENT);
        let body = builder.send()?.text()?;
        println!("---");
        println!("{}", body);
        println!("---");

        return Ok(());
        let one_second = time::Duration::from_millis(1000);
        thread::sleep(one_second);
    }

    return Ok(());
}

fn parse_naive_date(s: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

