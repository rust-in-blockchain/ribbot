/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */

#![allow(unused)]

use std::str::FromStr;
use anyhow::{Result, Context};
use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use reqwest::Method;
use std::{thread, time};
use structopt::StructOpt;
use chrono::{Date, DateTime, Local, Utc, NaiveDate};
use serde_json::Value;
use serde_derive::Deserialize;

static RIB_AGENT: &'static str = "ribbot (Rust-in-Blockchain bot; Aimeedeer/ribbot; aimeedeer@gmail.com)";
static CONFIG: &'static str = include_str!("rib-config.toml");
static DELAY_MS: u64 = 1000;

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

#[derive(Deserialize)]
struct Config {
    projects: Vec<Project>,
}

#[derive(Deserialize)]
struct Project {
    name: String,
    url: String,
    /// Repos for which we care about merged PRs
    pull_merged_repos: Vec<String>,
}

fn main() -> Result<()> {
    let options = Options::from_args();
    let ref config = toml::from_str::<Config>(CONFIG)
        .context("parsing configuration")?;

    match options.cmd {
        Command::Pulls { date } => {
            fetch_pulls(config, date)?;
        }
    }

    Ok(())
}

fn fetch_pulls(config: &Config, date: NaiveDate) -> Result<()> {
    
    let client = Client::new();

    for project in &config.projects {
        println!("### [**{}**]({})", project.name, project.url);
        println!();
        for repo in &project.pull_merged_repos {
            println!("<!-- fetching pulls for {} -->", repo);
            let repourl = format!("https://api.github.com/repos/{}/pulls", repo);
            let builder = client.request(Method::GET, &repourl);
            let builder = builder.header(USER_AGENT, RIB_AGENT);
            let body = builder.send()?.text()?;
            let body = Value::from_str(&body)?;
            println!("---");
            println!("{:#?}", body);
            println!("---");

            return Ok(());
        }
    }

    return Ok(());
}

fn parse_naive_date(s: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

fn delay() {
    let one_second = time::Duration::from_millis(DELAY_MS);
    thread::sleep(one_second);
}
