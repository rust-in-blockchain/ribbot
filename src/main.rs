/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */

#![allow(unused)]

use std::str::FromStr;
use anyhow::{Result, Context, bail};
use reqwest::blocking::{Client, Response};
use reqwest::header::USER_AGENT;
use reqwest::Method;
use std::{thread, time};
use structopt::StructOpt;
use chrono::{Date, DateTime, Local, Utc, NaiveDate};
use serde_json::Value;
use serde_derive::Deserialize;
use reqwest::header;

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
        #[structopt(long, parse(try_from_str = parse_naive_date))]
        since: NaiveDate,
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
        Command::Pulls { since } => {
            fetch_pulls(config, since)?;
        }
    }

    Ok(())
}

fn fetch_pulls(config: &Config, since: NaiveDate) -> Result<()> {
    
    let client = Client::new();

    for project in &config.projects {
        println!("<!-- fetching pulls for project {} -->", project.name);
        for repo in &project.pull_merged_repos {
            println!("<!-- fetching pulls for repo {} -->", repo);

            for page in 1.. {
                println!("<!-- fetching page {} -->", page);

                let repourl = format!("https://api.github.com/repos/{}/pulls?state=closed&sort=popularity&direction=desc", repo);
                let builder = client.request(Method::GET, &repourl);
                let builder = builder.header(USER_AGENT, RIB_AGENT);
                let resp = builder.send()?;
                let next = parse_next(&resp)?;
                let body = resp.text()?;
                let body = Value::from_str(&body)?;

                /*println!("---");
                println!("{:#?}", body);
                println!("---");*/

                return Ok(());
                delay();
            }
        }
    }

    return Ok(());
}

fn parse_next(resp: &Response) -> Result<Option<String>> {
    if let Some(link_header) = resp.headers().get(header::LINK) {
        let link_header = link_header.to_str()?;
        for entry in link_header.split(",") {
            if let Some((url, rel)) = split_2_trim(&entry) {

            } else {
                bail!("unexpected link header");
            }
        }

        Ok(None)
    } else {
        Ok(None)
    }
}

fn split_2_trim(s: &str) -> Option<(&str, &str)> {
    let mut elts = s.splitn(2, s);
    let one = elts.next();
    let two = elts.next();
    if let (Some(one), Some(two)) = (one, two) {
        Some((one.trim(), two.trim()))
    } else {
        None
    }
}

fn parse_naive_date(s: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

fn delay() {
    let one_second = time::Duration::from_millis(DELAY_MS);
    thread::sleep(one_second);
}
