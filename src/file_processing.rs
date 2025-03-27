use anyhow::{anyhow, Result};
use colored::Colorize;
use std::{fs::{self, File}, io::{self, Write}, path::Path};
use walkdir::WalkDir;
use crate::config::Config;
use csv::Writer;

pub fn validate_path(dir_path: &str) -> Result<()> {
    let path = Path::new(dir_path);
    if !path.exists() {
        return Err(anyhow!("Path does not exist"));
    }
    if !path.is_dir() {
        return Err(anyhow!("Path is not a directory"));
    }
    Ok(())
}

pub fn collect_file_paths(config: &Config) -> Result<Vec<String>> {
    let mut paths: Vec<String> = WalkDir::new(&config.dir_path)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            fs::metadata(e.path()).ok().map_or(false, |metadata| {
                let file_size = metadata.len();
                config.min_file_size.map_or(true, |min| file_size >= min)
                    && config.max_file_size.map_or(true, |max| file_size <= max)
            })
        })
        .map(|e| e.path().display().to_string())
        .filter(|path| !path.contains(' '))
        .collect();

    if let Some(filter) = &config.filter {
        paths.retain(|p| filter.is_match(p));
    }

    if paths.is_empty() {
        return Err(anyhow!("{}", "No files matching the given criteria".red()));
    }

    Ok(paths)
}

pub fn write_results(results: &[(String, Vec<crate::trid::Extension>)], output_path: &Option<String>) -> Result<()> {
    let writer: Box<dyn Write> = match output_path {
        Some(path) => Box::new(File::create(path)?),
        None => Box::new(io::stdout()),
    };

    let mut wtr = Writer::from_writer(writer);
    wtr.write_record(["File path", "Percentage", "Extension", "Name", "Mime Type", "Url", "Definition"])?;

    for (path, extensions) in results {
        for ext in extensions {
            wtr.write_record([
                path, &ext.percentage, &ext.extension, &ext.name, &ext.mime_type, &ext.url, &ext.definition,
            ])?;
        }
    }
    Ok(())
}
