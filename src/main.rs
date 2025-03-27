use anyhow::anyhow;
use clap::{arg, command, value_parser};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use anyhow::Context;
use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;
use std::{
    env, fs::File, io::{self, Write}, path::Path, process::{Command, Stdio}
};
use walkdir::WalkDir;

lazy_static! {
    static ref TRID_REGEX: Regex = Regex::new(r"(?s).*?([0-9\.]+%) \(([a-zA-Z/\.]+)\) ([a-zA-Z /]+) \(.*Mime type +: ([a-zA-Z/-]+).*Related URL: (https?://[^\n]+)\n.*?Definition +: +([a-zA-Z-\.-]+)").unwrap();
    static ref TRID_REGEX_SMALL: Regex = Regex::new(r"(?s).*?([0-9\.]+%) \(([a-zA-Z/\.]+)\) ([a-zA-Z /]+) .*").unwrap();
}

struct Config {
    dir_path: String,
    output_path: Option<String>,
    filter: Option<Regex>,
}

fn main() -> Result<()> {
    let config = parse_arguments()?;
    validate_path(&config.dir_path)?;

    check_trid_database()?;

    let mut paths = collect_file_paths(&config)?;
    
    let pb = ProgressBar::new(paths.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );

    let results: Vec<_> = paths
        .par_iter()
        .filter_map(|path| {
            get_trid_output(path)
                .ok()
                .and_then(|output| parse_trid_output(&output.0).ok())
                .map(|output| {
                    pb.inc(1);
                    (path.clone(), output)
                })
        })
        .collect();

    pb.finish_with_message("Processing complete.");

    write_results(&results, &config.output_path)?;

    Ok(())
}

fn parse_arguments() -> Result<Config> {
    let matches = command!()
        .arg(arg!([path] "Path to the input folder").required(true))
        .arg(arg!(-o --output <FILE> "Sets custom output file path"))
        .arg(arg!(-f --filter <FILTER> "Regex filter for subfolders"))
        .arg(arg!(-t --threads <COUNT> "Sets the thread count to be used").value_parser(value_parser!(usize)))
        .get_matches();

    if let Some(thread_count) = matches.get_one::<usize>("threads") {
        rayon::ThreadPoolBuilder::new().num_threads(*thread_count).build_global()?;
    }

    let dir_path = matches.get_one::<String>("path").unwrap().clone();

    if dir_path.contains("./") || dir_path.contains("../") || dir_path.contains(' ') {
        return Err(anyhow!("{}", "TrID does not support paths with './', '../', or spaces".red()));
    }

    let output_path = matches.get_one::<String>("output").cloned();
    if let Some(ref path) = output_path {
        if !path.ends_with(".csv") {
            return Err(anyhow!("{}", "Only .csv output files are supported at the moment".red()));
        }
    }

    let filter = matches.get_one::<String>("filter").map(|f| Regex::new(f).unwrap());

    Ok(Config {
        dir_path,
        output_path,
        filter,
    })
}

fn validate_path(dir_path: &str) -> Result<()> {
    let path = Path::new(dir_path);
    if !path.exists() {
        return Err(anyhow!("{}", "Path does not exist".red()));
    }
    if !path.is_dir() {
        return Err(anyhow!("{}", "Path is not a directory".red()));
    }
    Ok(())
}

fn collect_file_paths(config: &Config) -> Result<Vec<String>> {
    let mut paths: Vec<String> = WalkDir::new(&config.dir_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().display().to_string())
        .filter(|path| !path.contains(' '))
        .collect();

    if let Some(filter) = &config.filter {
        paths.retain(|p| filter.is_match(p));
    }

    Ok(paths)
}

#[derive(Debug)]
struct Extension {
    percentage: String,
    name: String,
    extension: String,
    mime_type: String,
    url: String,
    definition: String,
}

fn parse_trid_output(content: &str) -> Result<Vec<Extension>> {
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

fn check_trid_database() -> Result<()> {
    let output = Command::new("trid")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("Failed to execute trid")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.contains("database not found!") {
        return Err(anyhow!("Trid definitions database not found, please update the database").into());
    }

    Ok(())
}

fn get_trid_output(path: &str) -> Result<(String, String)> {
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

fn write_results(results: &[(String, Vec<Extension>)], output_path: &Option<String>) -> Result<()> {
    let writer: Box<dyn Write> = match output_path {
        Some(path) => Box::new(File::create(path).context("Failed to create output file")?),
        None => Box::new(io::stdout()),
    };

    let mut wtr = csv::Writer::from_writer(writer);

    wtr.write_record(&["File path", "Percentage", "Extension", "Name", "Mime Type", "Url", "Definition"])?;

    for (path, extensions) in results {
        for ext in extensions {
            wtr.write_record(&[
                path, &ext.percentage, &ext.extension, &ext.name, &ext.mime_type, &ext.url, &ext.definition,
            ])?;
        }
    }
    Ok(())
}