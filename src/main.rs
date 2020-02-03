/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */
extern crate postgres;

use postgres::{Client as PostgresClient, NoTls};

use reqwest::blocking::Client;
use reqwest::header::USER_AGENT;
use reqwest::Method;
use serde_json::value::Value;
use std::{thread, time};

static RIB_AGENT: &'static str = "ribbot";

struct RepoPulls {
    id: i32,
    name: String,
    data: Option<Vec<u8>>,
}

fn main() -> Result<(), reqwest::Error> {
    let client = Client::new();
    let repos = include_str!("github-repos.txt");

    let mut clientrepo = PostgresClient::connect(
        "host=localhost user=postgres password=p1!cvxGRftGM#lcM50*Ydr2l7@dkB*HwUr",
        NoTls,
    )
    .unwrap();

    let temp = clientrepo
        .query_one(
            "
       SELECT EXISTS 
        (
            SELECT 1 
            FROM pg_tables
            WHERE tablename = 'repodata'
        );
    ",
            &[],
        )
        .unwrap();

    let temp2: bool = temp.get(0);
    //println!("{:#?}", temp2);
    //panic!();
    if temp2 == false {
        clientrepo
            .batch_execute(
                "    
            CREATE TABLE repodata (
                id      SERIAL PRIMARY KEY,
                name    TEXT NOT NULL,
                data    BYTEA
            )
            ",
            )
            .unwrap();
    }
    for repo in repos.lines() {
        println!("{}", repo);
        let repourl = format!("https://api.github.com/repos/{}/pulls", repo);
        let builder = client.request(Method::GET, &repourl);
        let builder = builder.header(USER_AGENT, RIB_AGENT);
        let body = builder.send()?.text()?;
        //   println!("body = {:#?}", body);

        let one_second = time::Duration::from_millis(1000);
        thread::sleep(one_second);

        clientrepo
            .execute(
                "INSERT INTO repodata (name, data) VALUES ($1, $2)",
                &[&repo, &body],
            )
            .unwrap();
    }

    return Ok(());
}
