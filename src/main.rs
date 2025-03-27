use anyhow::anyhow;
use clap::{arg, command, value_parser};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use rayon::prelude::*;
use regex::Regex;
use std::{
    env, fs::File, io::{self, Write}, path::Path, process::{exit, Command, Stdio}
};
use walkdir::WalkDir;

lazy_static! {
    static ref TRID_REGEX: Regex = Regex::new(r"(?s).*?([0-9\.]+%) \(([a-zA-Z/\.]+)\) ([a-zA-Z /]+) \(.*Mime type +: ([a-zA-Z/-]+).*Related URL: (https?://[^\n]+)\n.*?Definition +: +([a-zA-Z-\.-]+)").unwrap();
    static ref TRID_REGEX_SMALL: Regex = Regex::new(r"(?s).*?([0-9\.]+%) \(([a-zA-Z/\.]+)\) ([a-zA-Z /]+) .*").unwrap();
}

fn main() -> Result<(), anyhow::Error> {
    let matches = command!()
        .arg(arg!([path] "Path to the input folder").required(true))
        .arg(arg!(-o --output <FILE> "Sets custom output file path"))
        .arg(arg!(-t --threads <COUNT> "Sets the thread count to be used").value_parser(value_parser!(usize)))
        .arg(arg!(-f --filter <FILTER> "Sets a regex filter to be used on the subfolders"))
        .get_matches();

    let dir_path = matches.get_one::<String>("path").unwrap();

    let mut output_path = String::new();
    if let Some(path) = matches.get_one::<String>("output") {
        if !path.ends_with(".csv") {
            return Err(anyhow!("Only .csv output files are supported at the moment".red()))
        }
        output_path = path.clone();
    }

    if let Some(thread_count) = matches.get_one::<usize>("threads") {
        rayon::ThreadPoolBuilder::new()
        .num_threads(*thread_count)
        .build_global()
        .unwrap();
    }

    if dir_path.contains("./") || dir_path.contains("../") {
        let args = env::args().collect::<Vec<String>>();
        return Err(anyhow!("{} {} {}",
        "TrID does not play well with paths that contain ./ or ../ please try running the app with".red(),
        args[0].green(),
        dir_path.replace("../", "").replace("./", "").green()));
    }

    if dir_path.contains(' ') {
        return Err(anyhow!(
            "Trid does not support paths with spaces, change the input directory name or location"
                .red()
        ));
    }

    let path = Path::new(dir_path);
    if !path.exists() {
        return Err(anyhow!("Path does not exist".red()));
    }

    if !path.is_dir() {
        return Err(anyhow!("Path is not a directory".red()));
    }

    check_trid_database()?;

    let mut paths: Vec<String> = WalkDir::new(dir_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().display().to_string())
        .filter_map(|path| {
            if path.contains(' ') {
                eprintln!(
                    "{} {}{}",
                    "Path contains a space:".red(),
                    path.yellow(),
                    ", skipping".red()
                );
                None
            } else {
                Some(path)
            }
        })
        .collect();

        if let Some(filter) = matches.get_one::<String>("filter") {
            let regex = Regex::new(filter)?;
            paths = paths.iter().filter(|p| {
                regex.is_match(p)
            }).cloned().collect::<Vec<_>>();
        }

    let pb = ProgressBar::new(paths.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")
            .unwrap()
            .progress_chars("=>-"),
    );

    let data: Vec<_> = paths
        .par_iter()
        .filter_map(|path| match get_trid_output(path) {
            Ok(output) => Some((path, output.0)),
            Err(e) => {
                eprintln!("{}", e);
                None
            }
        })
        .filter_map(|(path, stdout)| match parse_trid_output(&stdout) {
            Ok(output) => Some((path, output)),
            Err(e) => {
                eprintln!("{}", e);
                None
            }
        })
        .map(|(path, output)| {
            pb.inc(1);
            (path, output)
        })
        .collect();
    pb.finish_with_message("Processing complete.");

    let writer: Box<dyn Write> = match output_path.len() {
        0 => Box::new(io::stdout()),
        _ => Box::new(File::create(output_path).expect("Failed to create output file")),
    };

    let mut wtr = csv::Writer::from_writer(writer);
    
    wtr.write_record([
        "File path",
        "Percentage",
        "Extension",
        "Name",
        "Mime Type",
        "Url",
        "Definition",
    ])?;
    for (path, extensions) in data {
        for extension in extensions {
            wtr.write_record([
                path,
                &extension.percentage,
                &extension.extension,
                &extension.name,
                &extension.mime_type,
                &extension.url,
                &extension.definition,
            ])?;
        }
    }

    Ok(())
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

fn parse_trid_output(content: &str) -> Result<Vec<Extension>, anyhow::Error> {
    let before_header = content.find("Collecting data from file")
        .ok_or_else(|| anyhow!("Malformed trid output: {}", content))?;

    let after_header = content[before_header..].find("\n ")
        .ok_or_else(|| anyhow!("Malformed trid output: {}", content))?;


    let guesses: Vec<_> = content[after_header..]
        .split("\n\n")
        .filter_map(|guess| {
            if let Some(caps) = TRID_REGEX.captures(guess) {
                Some(Extension {
                    percentage: caps[1].to_owned(),
                    name: caps[3].to_owned(),
                    extension: caps[2].to_owned(),
                    mime_type: caps[4].to_owned(),
                    url: caps[5].to_owned(),
                    definition: caps[6].to_owned(),
                })
            } else {
                TRID_REGEX_SMALL.captures(guess).map(|caps| Extension {
                    percentage: caps[1].to_owned(),
                    name: caps[3].to_owned(),
                    extension: caps[2].to_owned(),
                    mime_type: String::new(),
                    url: String::new(),
                    definition: String::new(),
                })
            }
        })
        .collect();

    Ok(guesses)
}

fn check_trid_database() -> Result<(), anyhow::Error> {
    let child = Command::new("trid")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.contains("database not found!") {
        #[cfg(target_os = "linux")]
        {
            return Err(anyhow!(
                "Trid definitions database not found, please run update-trid-defs as root."
            ));
        }
        #[cfg(target_os = "windows")]
        {
            return Err(anyhow!(
                "Trid definitions database not found please update the trid database"
            ));
        }
    }

    Ok(())
}

fn get_trid_output(path: &str) -> Result<(String, String), anyhow::Error> {
    let child = Command::new("trid")
        .arg("-v")
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let output = child.wait_with_output()?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Ok((stdout, stderr))
}
