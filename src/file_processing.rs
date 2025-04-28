use anyhow::{anyhow, Result};
use colored::Colorize;
use std::{fs::{self, File}, io::{self, Write}, path::Path};
use walkdir::WalkDir;
use crate::config::Config;
use csv::Writer;
use serde_json;
use sailfish::TemplateSimple;

#[derive(TemplateSimple)]
#[template(path = "results.stpl")]
struct ResultsTemplate<'a> {
    results: &'a [(String, Vec<crate::trid::Extension>)],
}

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
            fs::metadata(e.path()).ok().is_some_and(|metadata| {
                let file_size = metadata.len();
                config.min_file_size.is_none_or(|min| file_size >= min)
                    && config.max_file_size.is_none_or(|max| file_size <= max)
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
    let mut writer: Box<dyn Write> = match output_path {
        Some(path) => Box::new(File::create(path)?),
        None => Box::new(io::stdout()),
    };

    // Determine format based on file extension
    let format = output_path
        .as_ref()
        .and_then(|path| Path::new(path).extension())
        .and_then(|ext| ext.to_str())
        .unwrap_or("csv");

    match format.to_lowercase().as_str() {
        "csv" => {
            let mut wtr = Writer::from_writer(writer);
            wtr.write_record(["File path", "Percentage", "Extension", "Name", "Mime Type", "Url", "Definition"])?;

            for (path, extensions) in results {
                for ext in extensions {
                    wtr.write_record([
                        path, &ext.percentage, &ext.extension, &ext.name, &ext.mime_type, &ext.url, &ext.definition,
                    ])?;
                }
            }
        }
        "json" => {
            let json_results: Vec<_> = results
                .iter()
                .map(|(path, extensions)| {
                    extensions
                        .iter()
                        .map(|ext| {
                            serde_json::json!({
                                "file_path": path,
                                "percentage": ext.percentage,
                                "extension": ext.extension,
                                "name": ext.name,
                                "mime_type": ext.mime_type,
                                "url": ext.url,
                                "definition": ext.definition
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .flatten()
                .collect();
            
            serde_json::to_writer_pretty(writer, &json_results)?;
        }
        "html" => {
            let template = ResultsTemplate { results };
            write!(writer, "{}", template.render_once()?)?;
        }
        "xml" => {
            let xml_results = results
                .iter()
                .map(|(path, extensions)| {
                    extensions
                        .iter()
                        .map(|ext| {
                            format!(
                                r#"<result>
    <file_path>{}</file_path>
    <percentage>{}</percentage>
    <extension>{}</extension>
    <name>{}</name>
    <mime_type>{}</mime_type>
    <url>{}</url>
    <definition>{}</definition>
</result>"#,
                                path, ext.percentage, ext.extension, ext.name, ext.mime_type, ext.url, ext.definition
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect::<Vec<_>>()
                .join("\n");

            write!(writer, "<results>\n{}\n</results>", xml_results)?;
        }
        _ => return Err(anyhow!("Unsupported output format: {}", format)),
    }

    Ok(())
}
