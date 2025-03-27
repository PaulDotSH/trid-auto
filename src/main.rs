mod config;
mod file_processing;
mod trid;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use crate::config::parse_arguments;
use crate::file_processing::{collect_file_paths, validate_path, write_results};
use crate::trid::{check_trid_database, get_trid_output, parse_trid_output};

fn main() -> Result<()> {
    let config = parse_arguments()?;
    validate_path(&config.dir_path)?;

    check_trid_database()?;

    let paths = collect_file_paths(&config)?;
    
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
