//! Claude Code's `SessionStart` contract hook, as `tt skill install` wires it.
//!
//! These run the real Node scripts against a sandboxed `HOME`, so the
//! `settings.json` they edit is the case's own. Without Node there is nothing
//! to run, and each case says so and passes.

mod common;

use common::Case;
use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const SKILL_MD: &str = include_str!("../skills/tt-time-logging/SKILL.md");
const CARD_MARKER: &str = "<!-- card:end";

fn node_available() -> bool {
    let found = Command::new("node")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !found {
        eprintln!("node is not on PATH; skipped");
    }
    found
}

/// The first heading below the card marker: in the contract, not in the card.
fn heading_below_card() -> &'static str {
    let below = &SKILL_MD[SKILL_MD
        .find(CARD_MARKER)
        .expect("SKILL.md has a card marker")..];
    below
        .lines()
        .find(|line| line.starts_with("## "))
        .expect("SKILL.md has a section below the card")
}

/// `tt skill install --dir` with hooks, then the sandbox's settings.json.
fn install(case: &Case, extra: &[&str]) -> Value {
    std::fs::create_dir_all(case.home.join(".claude")).unwrap();
    let skills = case.home.join("skills");
    // The installer's `tt` probe must find this build, not one on the machine.
    let tt_dir = Path::new(env!("CARGO_BIN_EXE_tt")).parent().unwrap();
    let path = std::env::join_paths(std::iter::once(tt_dir.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();

    let mut args = vec!["skill", "install", "--dir", skills.to_str().unwrap()];
    args.extend_from_slice(extra);
    let run = case.run_raw(
        &args,
        &[
            ("PATH", path.to_str().unwrap()),
            ("USERPROFILE", case.home.to_str().unwrap()),
        ],
    );
    assert_eq!(run.status, Some(0), "stderr: {}", run.stderr);

    let settings = std::fs::read_to_string(case.home.join(".claude/settings.json")).unwrap();
    serde_json::from_str(&settings).unwrap()
}

/// Every SessionStart command that runs the contract hook.
fn session_start_contract_commands(settings: &Value) -> Vec<String> {
    settings["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().unwrap())
        .filter_map(|hook| hook["command"].as_str())
        .filter(|command| command.contains("tt-contract-hook.mjs"))
        .map(str::to_string)
        .collect()
}

fn installed_hook(case: &Case) -> PathBuf {
    case.home
        .join(".claude/hooks/tt-time-logging/tt-contract-hook.mjs")
}

/// What the installed contract hook injects in `mode`.
fn inject(case: &Case, mode: &str) -> String {
    let mut child = Command::new("node")
        .arg(installed_hook(case))
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"session_id":"s-1"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("the hook injected something")
        .to_string()
}

#[test]
fn session_start_injects_the_whole_contract_by_default() {
    if !node_available() {
        return;
    }
    let case = Case::new("claude-hooks-default");

    let settings = install(&case, &[]);

    let commands = session_start_contract_commands(&settings);
    assert_eq!(commands.len(), 1, "{commands:?}");
    assert!(commands[0].ends_with("\" session"), "{commands:?}");
    assert!(inject(&case, "session").contains(heading_below_card()));
}

#[test]
fn the_session_card_flag_swaps_the_session_start_hook_and_back() {
    if !node_available() {
        return;
    }
    let case = Case::new("claude-hooks-swap");

    let card = session_start_contract_commands(&install(&case, &["--session-card"]));
    assert_eq!(card.len(), 1, "both variants would inject: {card:?}");
    assert!(card[0].ends_with("\" session-card"), "{card:?}");

    let full = session_start_contract_commands(&install(&case, &[]));
    assert_eq!(full.len(), 1, "both variants would inject: {full:?}");
    assert!(full[0].ends_with("\" session"), "{full:?}");
}

#[test]
fn the_session_card_mode_injects_the_card_and_the_contract_path() {
    if !node_available() {
        return;
    }
    let case = Case::new("claude-hooks-card");
    install(&case, &["--session-card"]);

    let injected = inject(&case, "session-card");
    let contract_path = installed_hook(&case)
        .with_file_name("SKILL.md")
        .to_string_lossy()
        .replace('\\', "/");

    assert!(injected.contains(&contract_path), "{injected}");
    assert!(!injected.contains(heading_below_card()), "{injected}");
    let prompt = inject(&case, "prompt");
    let card = prompt.split("\n\nThis session's id").next().unwrap();
    assert!(
        injected.starts_with(card),
        "it opens with the per-prompt card"
    );
    assert!(
        injected.contains("`s-1`"),
        "the session id is still appended"
    );
}
