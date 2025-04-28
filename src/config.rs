use anyhow::{anyhow, Result};
use clap::{arg, command, value_parser};
use parse_size::parse_size;
use regex::Regex;

pub struct Config {
    pub dir_path: String,
    pub output_path: Option<String>,
    pub filter: Option<Regex>,
    pub min_file_size: Option<u64>,
    pub max_file_size: Option<u64>,
}

pub fn parse_arguments() -> Result<Config> {
    let matches = command!()
        .arg(arg!([path] "Path to the input folder").required(true))
        .arg(arg!(-o --output <FILE> "Sets custom output file path"))
        .arg(arg!(-f --filter <FILTER> "Regex filter for subfolders"))
        .arg(arg!(-t --threads <COUNT> "Sets the thread count").value_parser(value_parser!(usize)))
        .arg(arg!(-n --min <SIZE> "Sets the min file size"))
        .arg(arg!(-m --max <SIZE> "Sets the max file size"))
        .get_matches();

    if let Some(thread_count) = matches.get_one::<usize>("threads") {
        rayon::ThreadPoolBuilder::new().num_threads(*thread_count).build_global()?;
    }

    let dir_path = matches.get_one::<String>("path").unwrap().clone();

    if dir_path.contains("./") || dir_path.contains("../") || dir_path.contains(' ') {
        return Err(anyhow!("TrID does not support paths with './', '../', or spaces"));
    }

    let output_path = matches.get_one::<String>("output").cloned();
    if let Some(ref path) = output_path {
        if !path.ends_with(".csv") && !path.ends_with(".json") && !path.ends_with(".xml") && !path.ends_with(".html") {
            return Err(anyhow!("Only .csv, .json, or .xml output files are supported at the moment"));
        }
    }

    let filter = matches.get_one::<String>("filter").map(|f| Regex::new(f).unwrap());

    let min_file_size = matches.get_one::<String>("min").and_then(|fs| parse_size(fs).ok());


    let max_file_size = matches.get_one::<String>("max").and_then(|fs| parse_size(fs).ok());

    Ok(Config {
        dir_path,
        output_path,
        filter,
        min_file_size,
        max_file_size,
    })
}
