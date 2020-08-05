/*
 *   Copyright (c) 2020
 *   All rights reserved.
 */

#![allow(unused)]

use anyhow::{bail, Context, Result};
use chrono::{Date, DateTime, Local, NaiveDate, SecondsFormat, TimeZone, Utc};
use reqwest::blocking::{Client, Response};
use reqwest::header;
use reqwest::header::{HeaderMap, USER_AGENT};
use reqwest::{Method, StatusCode};
use serde_derive::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::str::FromStr;
use std::{thread, time};
use structopt::StructOpt;

static RIB_AGENT: &'static str =
    "ribbot (Rust-in-Blockchain; Aimeedeer/ribbot; aimeedeer@gmail.com)";
static CONFIG: &'static str = include_str!("rib-config.toml");
static DELAY_MS: u64 = 10;
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
    #[structopt(long)]
    oauth_token: Option<String>,
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
    let ref config = toml::from_str::<Config>(CONFIG).context("parsing configuration")?;

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
        limits: None,
        calls: 0,
    };

    let mut calls = 0;
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
        let issues = get_closed_issues(&mut client, project, opts)?;
        let open_issues = get_open_issues(&mut client, project, opts)?;
        let pull_stats = make_pull_stats(project, &pulls)?;
        let issue_stats = make_issue_stats(project, &issues)?;
        let open_issue_stats = make_issue_stats(project, &open_issues)?;
        print_project(
            project,
            &pulls,
            pull_stats,
            issue_stats,
            open_issue_stats,
            opts,
        );

        let new_calls = client.calls - calls;
        calls = client.calls;
        println!(
            "<!-- total GitHub calls: {}, new GitHub calls: {} -->",
            calls, new_calls
        );
    }

    return Ok(());
}

#[derive(Deserialize, Debug)]
struct GhPull {
    html_url: String,
    state: String,
    title: String,
    user: GhUser,
    updated_at: DateTime<Utc>,
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
struct GhComments {}

fn get_sorted_merged_pulls_with_comments(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhPullWithComments>> {
    let mut pulls = get_merged_pulls_with_comments(client, project, opts)?;
    pulls.sort_by_key(|pull| usize::max_value() - pull.comments);
    Ok(pulls)
}

fn get_sorted_merged_pulls_without_comments(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhPullWithComments>> {
    let mut pulls = get_merged_pulls_without_comments(client, project, opts)?;
    pulls.sort_by_key(|pull| usize::max_value() - pull.comments);
    Ok(pulls)
}

fn get_merged_pulls_with_comments(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhPullWithComments>> {
    get_merged_pulls(client, project, opts)?
        .into_iter()
        .map(|pull| {
            let comments = get_comment_count(client, &pull, opts)?;
            Ok(GhPullWithComments { pull, comments })
        })
        .collect()
}

fn get_merged_pulls_without_comments(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhPullWithComments>> {
    get_merged_pulls(client, project, opts)?
        .into_iter()
        .map(|pull| {
            let comments = 0;
            Ok(GhPullWithComments { pull, comments })
        })
        .collect()
}

#[derive(Deserialize, Debug)]
struct GhIssue {
    html_url: String,
    state: String,
    title: String,
    user: GhUser,
    updated_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    created_at: Option<DateTime<Utc>>,
    pull_request: Option<GhIssuePull>,
}

#[derive(Deserialize, Debug)]
struct GhIssuePull {}

fn begin_and_end(opts: &PullCmdOpts) -> (DateTime<Utc>, DateTime<Utc>) {
    let begin = opts.begin.and_hms(0, 0, 0);
    let begin = DateTime::<Utc>::from_utc(begin, Utc);
    let end = opts.end.and_hms(0, 0, 0);
    let end = DateTime::<Utc>::from_utc(end, Utc);
    (begin, end)
}

fn get_closed_issues(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhIssue>> {
    let (begin, end) = begin_and_end(opts);

    let mut all_issues = vec![];

    println!("<!-- fetching issues for project {} -->", project.name);
    for repo in &project.repos {
        println!("<!-- fetching issues for repo {} -->", repo);

        let since = begin.to_rfc3339_opts(SecondsFormat::Millis, true);
        let url = format!("https://api.github.com/repos/{}/issues?state=closed&sort=updated&direction=desc&since={}", repo, since);
        let new_issues = do_gh_api_paged_request(client, &url, &opts.oauth_token, |body| {
            let issues: Vec<GhIssue> = serde_json::from_str(&body)?;
            //println!("{:#?}", pulls);

            let mut any_outdated = false;
            let issues = issues
                .into_iter()
                .filter(|issue| {
                    if issue.updated_at < begin && !any_outdated {
                        println!(
                            "<!-- found old issue {}, {:?}; last page -->",
                            issue.html_url, issue.updated_at
                        );
                        any_outdated = true;
                    }
                    if let Some(closed_at) = issue.closed_at.clone() {
                        if closed_at < begin {
                            println!("<!-- discard too old: {} -->", issue.html_url);
                            false
                        } else if closed_at >= end {
                            println!("<!-- discard too new: {} -->", issue.html_url);
                            false
                        } else if issue.pull_request.is_some() {
                            println!("<!-- discard issue is pull: {} -->", issue.html_url);
                            false
                        } else {
                            true
                        }
                    } else {
                        println!("<!-- discard unclosed: {} -->", issue.html_url);
                        false
                    }
                })
                .collect();

            let keep_going = if any_outdated { false } else { true };

            Ok((issues, keep_going))
        })?;

        all_issues.extend(new_issues);
    }

    Ok(all_issues)
}

fn get_open_issues(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhIssue>> {
    let (begin, end) = begin_and_end(opts);

    let mut all_open_issues = vec![];

    println!("<!-- fetching open issues for project {} -->", project.name);
    for repo in &project.repos {
        println!("<!-- fetching open issues for repo {} -->", repo);

        let since = begin.to_rfc3339_opts(SecondsFormat::Millis, true);
        let url = format!("https://api.github.com/repos/{}/issues?state=open&sort=updated&direction=desc&since={}", repo, since);
        let new_issues = do_gh_api_paged_request(client, &url, &opts.oauth_token, |body| {
            let issues: Vec<GhIssue> = serde_json::from_str(&body)?;
            //println!("{:#?}", issues);

            let mut any_outdated = false;
            let issues = issues
                .into_iter()
                .filter(|issue| {
                    if issue.updated_at < begin && !any_outdated {
                        println!(
                            "<!-- found old issue {}, {:?}; last page -->",
                            issue.html_url, issue.updated_at
                        );
                        any_outdated = true;
                    }
                    if let Some(created_at) = issue.created_at.clone() {
                        if created_at < begin {
                            println!("<!-- discard too old: {} -->", issue.html_url);
                            false
                        } else if created_at >= end {
                            println!("<!-- discard too new: {} -->", issue.html_url);
                            false
                        } else if issue.pull_request.is_some() {
                            println!("<!-- discard issue is pull: {} -->", issue.html_url);
                            false
                        } else {
                            true
                        }
                    } else {
                        println!("<!-- discard unclosed: {} -->", issue.html_url);
                        false
                    }
                })
                .collect();

            let keep_going = if any_outdated { false } else { true };

            Ok((issues, keep_going))
        })?;

        all_open_issues.extend(new_issues);
    }

    Ok(all_open_issues)
}

fn get_merged_pulls(
    client: &mut GhClient,
    project: &Project,
    opts: &PullCmdOpts,
) -> Result<Vec<GhPull>> {
    let (begin, end) = begin_and_end(opts);

    let mut all_pulls = vec![];
    println!("<!-- fetching pulls for project {} -->", project.name);
    for repo in &project.repos {
        println!("<!-- fetching pulls for repo {} -->", repo);

        let url = format!(
            "https://api.github.com/repos/{}/pulls?state=closed&sort=updated&direction=desc",
            repo
        );

        let new_pulls = do_gh_api_paged_request(client, &url, &opts.oauth_token, |body| {
            let pulls: Vec<GhPull> = serde_json::from_str(&body)?;
            //println!("{:#?}", pulls);

            let mut any_outdated = false;
            let pulls = pulls
                .into_iter()
                .filter(|pr| {
                    if pr.updated_at < begin && !any_outdated {
                        println!(
                            "<!-- found old pull {}, {:?}; last page -->",
                            pr.html_url, pr.updated_at
                        );
                        any_outdated = true;
                    }
                    if let Some(merged_at) = pr.merged_at.clone() {
                        if merged_at < begin {
                            println!("<!-- discard too old: {} -->", pr.html_url);
                            false
                        } else if merged_at >= end {
                            println!("<!-- discard too new: {} -->", pr.html_url);
                            false
                        } else {
                            true
                        }
                    } else {
                        println!("<!-- discard unmerged: {} -->", pr.html_url);
                        false
                    }
                })
                .collect();

            let keep_going = if any_outdated { false } else { true };

            Ok((pulls, keep_going))
        })?;

        all_pulls.extend(new_pulls);
    }

    Ok(all_pulls)
}

fn get_comment_count(client: &mut GhClient, pull: &GhPull, opts: &PullCmdOpts) -> Result<usize> {
    println!("<!-- fetching comments for {} -->", pull.html_url);

    let comments = do_gh_api_paged_request(
        client,
        &pull.review_comments_url,
        &opts.oauth_token,
        |body| {
            let comments: Vec<GhComments> = serde_json::from_str(&body)?;
            Ok((comments, true))
        },
    )?;

    Ok(comments.len())
}

fn do_gh_api_paged_request<T>(
    client: &mut GhClient,
    url: &str,
    oauth_token: &Option<String>,
    f: impl Fn(String) -> Result<(Vec<T>, bool)>,
) -> Result<Vec<T>> {
    let mut url = url.to_string();

    let mut all_results = vec![];

    for page in 1.. {
        println!("<!-- fetching page {}: {} -->", page, url);

        let (body, headers) = do_gh_api_request(client, &url, oauth_token)?;

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
    limits: Option<RateLimitValues>,
    calls: u64,
}

fn do_gh_api_request(
    client: &mut GhClient,
    url: &str,
    oauth_token: &Option<String>,
) -> Result<(String, HeaderMap)> {
    do_gh_rate_limit(client)?;

    loop {
        let builder = client.client.request(Method::GET, url);
        let builder = builder.header(USER_AGENT, RIB_AGENT);
        let builder = if let Some(ref oauth_token) = *oauth_token {
            builder.header("Authorization", format!("token {}", oauth_token))
        } else {
            builder
        };
        let resp = builder.send()?;
        let headers = resp.headers().clone();
        let status = resp.status();
        let limits = get_rate_limit_values(&headers)?;

        println!("<!-- {:?} -->", limits);

        //println!("<!-- headers -->");
        for (k, v) in &headers {
            //println!("<!-- {}: {:?} -->", k, v);
        }

        client.calls += 1;
        do_gh_rate_limit_bookkeeping(client, &headers)?;

        match status {
            StatusCode::OK => {
                let body = resp.text()?;

                //let json_body = Value::from_str(&body)?;
                //println!("{}", serde_json::to_string_pretty(&json_body[0])?);

                return Ok((body, headers));
            }
            StatusCode::FORBIDDEN => {
                // Probably rate limited
                let rate_limited = limits.remaining == 0;
                if rate_limited {
                    do_gh_rate_limit_delay(&limits);
                    continue;
                } else {
                    println!("{:#?}", resp);
                    bail!("unexpected forbidden status");
                }
            }
            _ => {
                println!("{:#?}", resp);
                bail!("unexpected response");
            }
        }
    }

    unreachable!()
}

#[derive(Debug)]
struct RateLimitValues {
    limit: u64,
    remaining: u64,
    reset: DateTime<Utc>,
    reset_local: DateTime<Local>,
}

fn get_rate_limit_values(headers: &HeaderMap) -> Result<RateLimitValues> {
    let limit: u64 = headers
        .get("X-RateLimit-Limit")
        .expect("X-RateLimit-Limit")
        .to_str()?
        .parse()?;
    let remaining: u64 = headers
        .get("X-RateLimit-Remaining")
        .expect("X-RateLimit-Remaining")
        .to_str()?
        .parse()?;
    let reset: u64 = headers
        .get("X-RateLimit-Reset")
        .expect("X-RateLimit-Reset")
        .to_str()?
        .parse()?;
    // FIXME 'as' conversion
    let reset = Utc.timestamp(reset as i64, 0);
    let reset_local: DateTime<Local> = reset.into();

    Ok(RateLimitValues {
        limit,
        remaining,
        reset,
        reset_local,
    })
}

fn do_gh_rate_limit(client: &mut GhClient) -> Result<()> {
    if let Some(ref limits) = client.limits {
        if limits.remaining == 0 {
            do_gh_rate_limit_delay(&limits);
        }
    }
    Ok(())
}

fn do_gh_rate_limit_delay(limits: &RateLimitValues) {
    println!("<!-- rate limited, sleeping until {:?}", limits.reset_local);
    delay_until(limits.reset);
}

fn do_gh_rate_limit_bookkeeping(client: &mut GhClient, headers: &HeaderMap) -> Result<()> {
    let limits = get_rate_limit_values(headers)?;
    client.limits = Some(limits);
    Ok(())
}

fn print_project(
    project: &Project,
    pulls: &[GhPullWithComments],
    pull_stats: PullStats,
    issue_stats: PullStats,
    open_issue_stats: PullStats,
    opts: &PullCmdOpts,
) -> Result<()> {
    let stubname = make_stubname(project);
    let begin = opts.begin.format("%Y-%m-%d").to_string();
    // The end-date used in the human-readable queries is inclusive, where ours is exclusive.
    // Subtracting one will make the human-readable query links agree with our numbers.
    let end = opts.end - chrono::Duration::days(1);
    let end = end.format("%Y-%m-%d").to_string();

    println!("#### [**{}**]({})", project.name, project.url);
    println!();

    let total_merged_prs = pull_stats.stats.iter().fold(0, |a, s| a + s.count);
    let total_closed_issues = issue_stats.stats.iter().fold(0, |a, s| a + s.count);
    let total_open_issues = open_issue_stats.stats.iter().fold(0, |a, s| a + s.count);
    print!("{} merged PRs (", total_merged_prs);
    for (i, stat) in pull_stats.stats.iter().enumerate() {
        print!("[{}][{}-merged-prs-{}]", i + 1, stubname, i + 1);
        if i < pull_stats.stats.len() - 1 {
            print!(", ");
        }
    }
    print!("), ");
    print!("{} closed issues (", total_closed_issues);
    for (i, stat) in issue_stats.stats.iter().enumerate() {
        print!("[{}][{}-closed_issues-{}]", i + 1, stubname, i + 1);
        if i < issue_stats.stats.len() - 1 {
            print!(", ");
        }
    }
    println!("), ");
    print!("{} open issues (", total_open_issues);
    for (i, stat) in open_issue_stats.stats.iter().enumerate() {
        print!("[{}][{}-open_issues-{}]", i + 1, stubname, i + 1);
        if i < open_issue_stats.stats.len() - 1 {
            print!(", ");
        }
    }
    println!(")");

    println!();
    for (i, stat) in pull_stats.stats.iter().enumerate() {
        let human_query = format!(
            "{}/pulls?q=is%3Apr+is%3Aclosed+merged%3A{}..{}",
            stat.repo, begin, end
        );
        println!("[{}-merged-prs-{}]: {}", stubname, i + 1, human_query);
    }
    for (i, stat) in issue_stats.stats.iter().enumerate() {
        let human_query = format!(
            "{}/issues?q=is%3Aissue+is%3Aclosed+closed%3A{}..{}",
            stat.repo, begin, end
        );
        println!("[{}-closed_issues-{}]: {}", stubname, i + 1, human_query);
    }
    for (i, stat) in open_issue_stats.stats.iter().enumerate() {
        let human_query = format!(
            "{}/issues?q=is%3Aissue+is%3Aopen+created%3A{}..{}",
            stat.repo, begin, end
        );
        println!("[{}-open_issues-{}]: {}", stubname, i + 1, human_query);
    }
    println!();
    for pull in pulls {
        let comments = pull.comments;
        let pull = &pull.pull;
        println!(
            "- PR: [{}]({}) by [@{}](https://github.com/{})",
            pull.title, pull.html_url, pull.user.login, pull.user.login
        );
        if comments > 0 {
            println!("  <!-- ^ comments: {} -->", comments);
        }
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
    Ok(&s[1..s.len() - 1])
}

fn parse_naive_date(s: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(s, "%Y-%m-%d")?)
}

fn delay() {
    let one_second = time::Duration::from_millis(DELAY_MS);
    thread::sleep(one_second);
}

fn delay_until(date: DateTime<Utc>) {
    let now = Utc::now();
    if now < date {
        let wait_time = date - now;
        let wait_time = wait_time.to_std().expect("duration conversion");
        thread::sleep(wait_time);
    }
    thread::sleep(time::Duration::from_secs(5));
}

struct PullStats {
    stats: Vec<PullStat>,
}

struct PullStat {
    repo: String,
    count: usize,
}

fn make_issue_stats(project: &Project, issues: &[GhIssue]) -> Result<PullStats> {
    let mut map = HashMap::new();

    for issue in issues {
        let repo = repo_from_issue(&issue.html_url);
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
        println!("repo mismatch during issue stats: {}", k);
    }

    if !map.is_empty() {
        bail!("repo mismatch during issue stats for {}", project.name);
    }

    Ok(PullStats { stats })
}

fn repo_from_issue(issue: &str) -> String {
    let parts = issue.split("/").collect::<Vec<_>>();
    assert!(parts.len() > 2);
    let new_parts_count = parts.len() - 2;
    let parts = &parts[0..new_parts_count];
    parts.join("/")
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
        println!("repo mismatch during pull stats: {}", k);
    }

    if !map.is_empty() {
        bail!("repo mismatch during pull stats for {}", project.name);
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
