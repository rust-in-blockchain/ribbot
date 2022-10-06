# ribbot

Script for querying merged PRs, open issues, and closed issues from config repos.

## How to use it

Clone ribbot:

```
$ git clone https://github.com/rust-in-blockchain/ribbot.git && cd ribbot/
$ cargo run pulls --help

USAGE:
    ribbot pulls [OPTIONS] --begin <BEGIN> --end <END>

OPTIONS:
        --begin <BEGIN>
            e.g. 2022-09-01

        --end <END>
            e.g. 2022-10-01

    -h, --help
            Print help information

        --include-dependabot
            If set, include issues/PRs created by dependabot in analysis

        --no-comments
            If set, don't sort pull by comment count

        --oauth-token <OAUTH_TOKEN>
            GitHub token

        --only-project <ONLY_PROJECT>
            Project name must be spelled as in rib-config.toml

        --smoke-test
            Check if all the repos are good to query
```

Run ribbot:

```
$ cargo run -- pulls --begin 2022-09-01 --end 2022-10-01 --oauth-token <your-github-token> --no-comments

    Finished dev [unoptimized + debuginfo] target(s) in 0.11s
     Running `target/debug/ribbot pulls --begin 2022-09-01 --end 2022-10-01 --oauth-token <your-github-token> --no-comments`
### General

<!-- fetching pulls for project Aleo -->
<!-- fetching pulls for repo AleoHQ/aleo -->
<!-- fetching page 1: https://api.github.com/repos/AleoHQ/aleo/pulls?state=closed&sort=updated&direction=desc -->
<!-- discard unmerged: https://github.com/AleoHQ/aleo/pull/372 -->
<!-- discard unmerged: https://github.com/AleoHQ/aleo/pull/371 -->
<!-- discard unmerged: https://github.com/AleoHQ/aleo/pull/370 -->
<!-- discard unmerged: https://github.com/AleoHQ/aleo/pull/369 -->
<!-- discard dependabot: https://github.com/AleoHQ/aleo/pull/366 -->

...
```

To query a specific project, run ribbot with `--only-project <project-name>`.
The project name must be spelled as in [`rib-config.toml`]:

```
$ cargo run -- pulls --begin 2022-09-01 --end 2022-10-01 --oauth-token <your-github-token> --only-project Aleo --no-comments
```

Ribbot filters out activities from `dependabot` as default.
To include `dependabot`, run ribbot with `--include-dependabot`.

[`rib-config.toml`]: src/rib-config.toml

## TODO

- [ ] Refactor the code
- [ ] Auto update changed repos/orgs' names and URLs
