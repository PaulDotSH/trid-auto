use anyhow::{anyhow, Context, Result};
use lazy_static::lazy_static;
use regex::Regex;
use std::process::{Command, Stdio};

lazy_static! {
    static ref TRID_REGEX: Regex = Regex::new(r"(?s).*?([0-9\.]+%) \(([a-zA-Z/\.]+)\) ([a-zA-Z /]+) \(.*Mime type +: ([a-zA-Z/-]+).*Related URL: (https?://[^\n]+)\n.*?Definition +: +([a-zA-Z-\.-]+)").unwrap();
}

#[derive(Debug)]
pub struct Extension {
    pub percentage: String,
    pub name: String,
    pub extension: String,
    pub mime_type: String,
    pub url: String,
    pub definition: String,
}

pub fn check_trid_database() -> Result<()> {
    let output = Command::new("trid")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute trid")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("database not found!") {
        return Err(anyhow!("Trid definitions database not found, please update the database"));
    }

    Ok(())
}

pub fn get_trid_output(path: &str) -> Result<(String, String)> {
    let output = Command::new("trid")
        .arg("-v")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute trid")?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((stdout, stderr))
}

pub fn parse_trid_output(content: &str) -> Result<Vec<Extension>> {
    let guesses: Vec<_> = content
        .split("\n\n")
        .filter_map(|guess| {
            TRID_REGEX.captures(guess).map(|caps| Extension {
                percentage: caps[1].to_owned(),
                name: caps[3].to_owned(),
                extension: caps[2].to_owned(),
                mime_type: caps[4].to_owned(),
                url: caps[5].to_owned(),
                definition: caps[6].to_owned(),
            })
        })
        .collect();

    if guesses.is_empty() {
        Err(anyhow!("Failed to parse trid output"))
    } else {
        Ok(guesses)
    }
}
