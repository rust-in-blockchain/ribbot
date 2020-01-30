/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */
use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use reqwest::Method;
use serde_json::value::Value;
use std::{thread, time};

static RIB_AGENT: &'static str = "ribbot";

fn main() -> Result<(), reqwest::Error> {
    let client = Client::new();
    let repos = include_str!("github-repos.txt");

    for repo in repos.lines() {
        println!("{}", repo);
        let repourl = format!("https://api.github.com/repos/{}/pulls", repo);
        let builder = client.request(Method::GET, &repourl);
        let builder = builder.header(USER_AGENT, RIB_AGENT);
        let body: Value = builder.send()?.json()?;
        println!("body = {:#?}", body);

        let one_second = time::Duration::from_millis(1000);
        thread::sleep(one_second);
    }

    return Ok(());
}
