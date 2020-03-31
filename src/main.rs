/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */

#![allow(unused)]

use std::collections::HashMap;
use std::str::FromStr;
use anyhow::{Result, Context, bail};
use reqwest::blocking::{Client, Response};
use reqwest::header::{USER_AGENT, HeaderMap};
use reqwest::Method;
use std::{thread, time};
use structopt::StructOpt;
use chrono::{Date, DateTime, Local, Utc, NaiveDate};
use serde_json::Value;
use serde_derive::Deserialize;
use reqwest::header;

static RIB_AGENT: &'static str = "ribbot (Rust-in-Blockchain bot; Aimeedeer/ribbot; aimeedeer@gmail.com)";
static CONFIG: &'static str = include_str!("rib-config.toml");
static DELAY_MS: u64 = 100;
static MAX_PAGES: usize = 10;

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    Pulls(PullCmdOpts),
}

#[derive(StructOpt, Clone)]
struct PullCmdOpts {
    #[structopt(long, parse(try_from_str = parse_naive_date))]
    begin: NaiveDate,
    #[structopt(long, parse(try_from_str = parse_naive_date))]
    end: NaiveDate,
    #[structopt(long)]
    no_comments: bool,
    #[structopt(long)]
    only_project: Option<String>,
}


#[derive(Deserialize)]
struct Config {
    projects: Vec<Project>,
}

#[derive(Deserialize)]
struct Project {
    name: String,
    url: String,
    repos: Vec<String>,
}

fn main() -> Result<()> {
    let options = Options::from_args();
    let ref config = toml::from_str::<Config>(CONFIG)
        .context("parsing configuration")?;

    match options.cmd {
        Command::Pulls(opts) => {
            fetch_pulls(config, &opts)?;
        }
    }

    Ok(())
}

fn fetch_pulls(config: &Config, opts: &PullCmdOpts) -> Result<()> {
    
    let mut client = GhClient {
        client: Client::new(),
    };

    for project in &config.projects {
        if let Some(ref only_project) = opts.only_project {
            if project.name != *only_project {
                continue;
            }
        }
        let pulls = if !opts.no_comments {
            get_sorted_merged_pulls_with_comments(&mut client, project, opts)?
        } else {
            get_sorted_merged_pulls_without_comments(&mut client, project, opts)?
        };
        let stats = make_pull_stats(project, &pulls)?;
        print_pull_candidates(project, &pulls, stats, opts);
    }

    return Ok(());
}

#[derive(Deserialize, Debug)]
struct GhPull {
    html_url: String,
    state: String,
    title: String,
    user: GhUser,
    merged_at: Option<DateTime<Utc>>,
    review_comments_url: String,
    base: GhPullBase,
}

#[derive(Deserialize, Debug)]
struct GhUser {
    login: String,
}

#[derive(Deserialize, Debug)]
struct GhPullBase {
    repo: GhRepo,
}

#[derive(Deserialize, Debug)]
struct GhRepo {
    html_url: String,
}

#[derive(Deserialize, Debug)]
struct GhPullWithComments {
    pull: GhPull,
    comments: usize,
}

#[derive(Deserialize, Debug)]
struct GhComments {
}

fn get_sorted_merged_pulls_with_comments(client: &mut GhClient, project: &Project, opts: &PullCmdOpts) -> Result<Vec<GhPullWithComments>> {
    let mut pulls = get_merged_pulls_with_comments(client, project, opts)?;
    pulls.sort_by_key(|pull| {
        usize::max_value() - pull.comments
    });
    Ok(pulls)
}

fn get_sorted_merged_pulls_without_comments(client: &mut GhClient, project: &Project, opts: &PullCmdOpts) -> Result<Vec<GhPullWithComments>> {
    let mut pulls = get_merged_pulls_without_comments(client, project, opts)?;
    pulls.sort_by_key(|pull| {
        usize::max_value() - pull.comments
    });
    Ok(pulls)
}

fn get_merged_pulls_with_comments(client: &mut GhClient, project: &Project, opts: &PullCmdOpts) -> Result<Vec<GhPullWithComments>> {
    get_merged_pulls(client, project, opts)?.into_iter().map(|pull| {
        let comments = get_comment_count(client, &pull)?;
        Ok(GhPullWithComments {
            pull,
            comments,
        })
    }).collect()
}

fn get_merged_pulls_without_comments(client: &mut GhClient, project: &Project, opts: &PullCmdOpts) -> Result<Vec<GhPullWithComments>> {
    get_merged_pulls(client, project, opts)?.into_iter().map(|pull| {
        let comments = 0;
        Ok(GhPullWithComments {
            pull,
            comments,
        })
    }).collect()
}

fn get_merged_pulls(client: &mut GhClient, project: &Project, opts: &PullCmdOpts) -> Result<Vec<GhPull>> {
    let begin = opts.begin.and_hms(0, 0, 0);
    let begin = DateTime::<Utc>::from_utc(begin, Utc);
    let end = opts.end.and_hms(0, 0, 0);
    let end = DateTime::<Utc>::from_utc(end, Utc);

    let mut all_pulls = vec![];
    
    println!("<!-- fetching pulls for project {} -->", project.name);
    for repo in &project.repos {
        println!("<!-- fetching pulls for repo {} -->", repo);

        let url = format!("https://api.github.com/repos/{}/pulls?state=closed&sort=updated&direction=desc", repo);

        let new_pulls = do_gh_api_paged_request(client, &url, |body| {
            let pulls: Vec<GhPull> = serde_json::from_str(&body)?;
            //println!("{:#?}", pulls);

            let mut any_outdated = false;
            let pulls = pulls.into_iter().filter(|pr| {
                if let Some(merged_at) = pr.merged_at.clone() {
                    if merged_at < begin {
                        any_outdated = true;
                        false
                    } else if merged_at >= end {
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            }).collect();

            let keep_going = if any_outdated {
                false
            } else {
                true
            };

            Ok((pulls, keep_going))
        })?;

        all_pulls.extend(new_pulls);
    }

    Ok(all_pulls)
}

fn get_comment_count(client: &mut GhClient, pull: &GhPull) -> Result<usize> {
    println!("<!-- fetching comments for {} -->", pull.html_url);

    let comments = do_gh_api_paged_request(client, &pull.review_comments_url, |body| {
        let comments: Vec<GhComments> = serde_json::from_str(&body)?;
        Ok((comments, true))
    })?;

    Ok(comments.len())
}

fn do_gh_api_paged_request<T>(client: &mut GhClient, url: &str,
                              f: impl Fn(String) -> Result<(Vec<T>, bool)>) -> Result<Vec<T>> {
    let mut url = url.to_string();

    let mut all_results = vec![];

    for page in 1.. {
        println!("<!-- fetching page {}: {} -->", page, url);

        let (body, headers) = do_gh_api_request(client, &url)?;

        let (new_results, keep_going) = f(body)?;

        all_results.extend(new_results);

        if !keep_going {
            break;
        }

        let next = parse_next(&headers)?.map(ToString::to_string);

        if let Some(next) = next {
            url = next;
        } else {
            break;
        }

        if page >= MAX_PAGES {
            println!("<!-- reached max pages -->");
            break;
        }
    }

    Ok(all_results)
}

struct GhClient {
    client: Client,
}

fn do_gh_api_request(client: &mut GhClient, url: &str) -> Result<(String, HeaderMap)> {
    let builder = client.client.request(Method::GET, url);
    let builder = builder.header(USER_AGENT, RIB_AGENT);
    let resp = builder.send()?;
    let headers = resp.headers().clone();
    let body = resp.text()?;
    //let json_body = Value::from_str(&body)?;
    //println!("{}", serde_json::to_string_pretty(&json_body[0])?);

    delay();

    Ok((body, headers))
}

fn print_pull_candidates(project: &Project, pulls: &[GhPullWithComments],
                         stats: PullStats, opts: &PullCmdOpts) -> Result<()> {
    let stubname = make_stubname(project);
    let begin = opts.begin.format("%Y-%m-%d").to_string();
    let end = opts.end.format("%Y-%m-%d").to_string();

    println!();
    println!("#### [**{}**]({})", project.name, project.url);
    println!();

    let total_merged_prs = stats.stats.iter().fold(0, |a, s| a + s.count);
    print!("{} merged PRs (", total_merged_prs);
    for (i, stat) in stats.stats.iter().enumerate() {
        print!("[{}][{}-merged-pr-{}]", i + 1, stubname, i + 1);
        if i < stats.stats.len() - 1 {
            print!(", ");
        }
    }
    println!(")");
    println!();
    for (i, stat) in stats.stats.iter().enumerate() {
        let human_query=format!("{}/pulls?q=is%3Apr+is%3Aclosed+merged%3A{}..{}",
                                stat.repo, begin, end);
        println!("[{}-merged-pr-{}]: {}", stubname, i + 1, human_query);
    }
    println!();
    
    for pull in pulls {
        let comments = pull.comments;
        let pull = &pull.pull;
        println!("- PR: [{}]({}) by [@{}](https://github.com/{})",
                 pull.title, pull.html_url,
                 pull.user.login, pull.user.login);
        //println!("<!-- ^ comments: {}, merged_at: {:?} -->", comments, pull.merged_at);
    }
    println!();

    Ok(())
}

fn parse_next(headers: &HeaderMap) -> Result<Option<&str>> {
    if let Some(link_header) = headers.get(header::LINK) {
        let link_header = link_header.to_str()?;
        for entry in link_header.split(",") {
            if let Some((url, maybe_rel)) = split_2_trim(&entry, ';') {
                if let Some((rel_word, rel_value)) = split_2_trim(maybe_rel, '=') {
                    if rel_word == "rel" {
                        if rel_value == "\"next\"" {
                            return Ok(Some(parse_link_url(url)?));
                        }
                    } else {
                        bail!("unexpected link rel word");
                    }
                } else {
                    bail!("unexpected link rel pair");
                }
            } else {
                bail!("unexpected link header");
            }
        }

        Ok(None)
    } else {
        Ok(None)
    }
}

fn parse_link_url(s: &str) -> Result<&str> {
    trim_ends(s, b'<', b'>')
}

fn split_2_trim(s: &str, at: char) -> Option<(&str, &str)> {
    let mut elts = s.splitn(2, at);
    let one = elts.next();
    let two = elts.next();
    if let (Some(one), Some(two)) = (one, two) {
        Some((one.trim(), two.trim()))
    } else {
        None
    }
}

fn trim_ends(s: &str, front: u8, back: u8) -> Result<&str> {
    let s = s.trim();
    if s.len() < 2 || s.as_bytes()[0] != front || s.as_bytes()[s.len() - 1] != back {
        bail!("bad trim");
    }
    Ok(&s[1 .. s.len() - 1 ])
}

fn parse_naive_date(s: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

fn delay() {
    let one_second = time::Duration::from_millis(DELAY_MS);
    thread::sleep(one_second);
}

struct PullStats {
    stats: Vec<PullStat>,
}

struct PullStat {
    repo: String,
    count: usize,
}

fn make_pull_stats(project: &Project, pulls: &[GhPullWithComments]) -> Result<PullStats> {
    let mut map = HashMap::new();

    for pull in pulls {
        let repo = &pull.pull.base.repo.html_url;
        let counter = map.entry(repo).or_insert(0);
        *counter += 1;
    }

    let mut stats = vec![];
    for repo in &project.repos {
        let repo = repo_name_to_url(repo);
        let count = map.remove(&repo).unwrap_or(0);
        if count != 0 {
            stats.push(PullStat {
                repo: repo.to_string(),
                count,
            });
        }
    }

    for k in map.keys() {
        println!("repo mismatch during stats: {}", k);
    }

    if !map.is_empty() {
        bail!("repo mismatch during stats for {}", project.name);
    }

    Ok(PullStats { stats })
}

fn repo_name_to_url(repo: &str) -> String {
    format!("https://github.com/{}", repo)
}

fn make_stubname(project: &Project) -> String {
    let lower = project.name.to_ascii_lowercase();
    lower.replace(" ", "_")
}
