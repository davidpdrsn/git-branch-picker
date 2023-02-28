#![feature(exit_status_error)]

use anyhow::Result;
use chrono::prelude::*;
use chrono::Duration;
use git2::BranchType;
use git2::Oid;
use git2::Repository;
use git2::StatusOptions;
use skim::prelude::*;
use std::io::Cursor;
use std::process::exit;
use std::process::Command;

fn main() -> Result<()> {
    let repo = Repository::open_from_env()?;

    if !is_clean(&repo)? {
        eprintln!("Not clean");
        exit(1);
    }

    let branches = find_branches(&repo)?;
    let branch = pick_branch(branches)?;
    checkout_branch(repo, &branch.name)?;

    Ok(())
}

fn is_clean(repo: &Repository) -> Result<bool> {
    let mut options = StatusOptions::default();
    options.include_ignored(false);
    let statuses = repo.statuses(Some(&mut options))?;
    Ok(statuses.is_empty())
}

fn find_branches(repo: &Repository) -> Result<Vec<ListBranch>> {
    let mut branches = repo
        .branches(Some(BranchType::Local))?
        .map(|branch| {
            let (branch, _) = branch?;

            let name = branch.name()?.expect("Branch name wasn't invalid UTF-8");

            let commit = branch.get().peel_to_commit().expect("No target for branch");

            let time = commit.time();
            let offset = Duration::minutes(i64::from(time.offset_minutes()));
            let time = NaiveDateTime::from_timestamp_opt(time.seconds(), 0).unwrap() + offset;

            Ok(ListBranch {
                name: name.to_string(),
                id: commit.id(),
                time,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    branches.sort_by_key(|branch| branch.time);
    branches.reverse();
    Ok(branches)
}

#[derive(Debug)]
struct ListBranch {
    name: String,
    #[allow(dead_code)]
    id: Oid,
    time: NaiveDateTime,
}

fn pick_branch(branches: Vec<ListBranch>) -> Result<ListBranch> {
    let branch_len = branches
        .iter()
        .map(|branch| branch.name.len())
        .max()
        .expect("no branches");

    let now = Local::now().naive_local();

    let delta_humans = branches
        .iter()
        .map(|branch| {
            let delta = now.signed_duration_since(branch.time);
            if delta.num_minutes() == 0 {
                "less than 1 minute".to_string()
            } else if delta.num_days() == 0 {
                if delta.num_minutes() > 100 {
                    format!("{} hours", delta.num_hours())
                } else {
                    format!("{} minutes", delta.num_minutes())
                }
            } else if delta.num_weeks() > 0 {
                format!("{} weeks", delta.num_weeks())
            } else {
                format!("{} days", delta.num_days())
            }
        })
        .collect::<Vec<_>>();

    let delta_human_len = delta_humans
        .iter()
        .map(|line| line.len())
        .max()
        .expect("no branches");

    let input = branches
        .iter()
        .zip(delta_humans.into_iter())
        .map(|(branch, delta_human)| {
            let branch_padding = " ".repeat(branch_len.saturating_sub(branch.name.len()));

            let delta_padding = " ".repeat(delta_human_len.saturating_sub(delta_human.len()));

            Ok(format!(
                "{}{} | {}{} ({})",
                branch.name, branch_padding, delta_human, delta_padding, branch.time
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(input));

    let options = SkimOptionsBuilder::default().build().unwrap();

    let selected_items = Skim::run_with(&options, Some(items))
        .map(|out| out.selected_items)
        .unwrap_or_else(Vec::new);

    let picked_branch = selected_items
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            eprintln!("No selection made");
            exit(1)
        })
        .text()
        .to_string();

    let le_branch = picked_branch.split(' ').next().unwrap();

    let branch = branches
        .into_iter()
        .find(|branch| le_branch == branch.name)
        .expect("No matching branch");

    Ok(branch)
}

fn checkout_branch(_repo: Repository, branch_name: &str) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(["checkout", branch_name]);
    cmd.spawn()?.wait()?.exit_ok()?;

    Ok(())
}
