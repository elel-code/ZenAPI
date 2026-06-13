use anyhow::{Context, Result, anyhow, bail};
use tokio::runtime::Runtime;
use zenapi::{
    collection_runner::{
        CollectionRunResult, CollectionRunSummary, FailureStrategy, RunnerOptions, run_collection,
    },
    collections::ApiCollection,
    variables::VariableStore,
};

pub fn run(args: Vec<String>) -> Result<()> {
    let command = args.first().map(String::as_str);
    match command {
        Some("run") => run_collection_command(&args[1..]),
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some(command) => bail!("unknown command: {command}\n\n{}", usage()),
        None => crate::app::run(),
    }
}

fn run_collection_command(args: &[String]) -> Result<()> {
    let parsed = parse_run_args(args)?;
    let collection = ApiCollection::load_file(&parsed.collection_path)?;
    let runtime = Runtime::new().context("failed to create async runtime")?;
    let summary = runtime.block_on(run_collection(
        &collection,
        &VariableStore::new(),
        None,
        parsed.options,
    ));

    println!("{}", format_collection_run_summary(&summary));
    if summary.failed > 0 {
        bail!(
            "collection run failed: {} of {} executed requests failed",
            summary.failed,
            summary.results.len()
        );
    }

    Ok(())
}

fn parse_run_args(args: &[String]) -> Result<ParsedRunArgs> {
    let mut collection_path = None;
    let mut delay_ms = 0;
    let mut failure_strategy = FailureStrategy::Continue;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--stop-on-failure" => {
                failure_strategy = FailureStrategy::StopOnFailure;
                index += 1;
            }
            "--continue-on-failure" => {
                failure_strategy = FailureStrategy::Continue;
                index += 1;
            }
            "--delay-ms" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| anyhow!("--delay-ms requires a value"))?;
                delay_ms = value
                    .parse::<u64>()
                    .with_context(|| format!("invalid --delay-ms value: {value}"))?;
                index += 2;
            }
            "--help" | "-h" => {
                print_run_usage();
                std::process::exit(0);
            }
            value if value.starts_with('-') => bail!("unknown run option: {value}"),
            value => {
                if collection_path.replace(value.to_string()).is_some() {
                    bail!("run accepts exactly one collection path");
                }
                index += 1;
            }
        }
    }

    let collection_path = collection_path.ok_or_else(|| anyhow!("missing collection path"))?;
    Ok(ParsedRunArgs {
        collection_path,
        options: RunnerOptions {
            delay_ms,
            failure_strategy,
        },
    })
}

fn format_collection_run_summary(summary: &CollectionRunSummary) -> String {
    let mut lines = vec![format!(
        "{}: {} passed, {} failed, {} total",
        summary.collection_name, summary.passed, summary.failed, summary.total
    )];

    if summary.stopped_early {
        lines.push("Stopped on first failure.".to_string());
    }

    for result in &summary.results {
        lines.push(format_collection_run_result(result));
    }

    lines.join("\n")
}

fn format_collection_run_result(result: &CollectionRunResult) -> String {
    let status = result
        .status
        .map(|status| status.to_string())
        .unwrap_or_else(|| "ERR".to_string());
    let outcome = if result.success { "PASS" } else { "FAIL" };
    let path = result.path.join(" / ");
    let mut line = format!(
        "[{outcome}] {status} {} {} ({path})",
        result.method, result.url
    );

    if let Some(error) = &result.error {
        line.push_str(&format!(" - {error}"));
    }

    if !result.pre_request_actions.is_empty() {
        line.push_str(&format!(
            " - pre-request {}",
            result.pre_request_actions.len()
        ));
    }

    if !result.assertions.is_empty() {
        let failed = result
            .assertions
            .iter()
            .filter(|assertion| !assertion.passed)
            .count();
        line.push_str(&format!(
            " - tests {}/{}",
            result.assertions.len() - failed,
            result.assertions.len()
        ));
    }

    line
}

fn print_usage() {
    println!("{}", usage());
}

fn print_run_usage() {
    println!("{}", run_usage());
}

fn usage() -> &'static str {
    "Usage:\n  zenapi                 Start the GPUI desktop app\n  zenapi run <collection.json> [--delay-ms N] [--stop-on-failure]\n"
}

fn run_usage() -> &'static str {
    "Usage:\n  zenapi run <collection.json> [--delay-ms N] [--stop-on-failure]\n\nOptions:\n  --delay-ms N          Delay between requests\n  --stop-on-failure     Stop after the first failed request\n  --continue-on-failure Continue after failures (default)\n"
}

struct ParsedRunArgs {
    collection_path: String,
    options: RunnerOptions,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_run_options() {
        let parsed = parse_run_args(&[
            "collection.json".to_string(),
            "--delay-ms".to_string(),
            "25".to_string(),
            "--stop-on-failure".to_string(),
        ])
        .expect("parse");

        assert_eq!(parsed.collection_path, "collection.json");
        assert_eq!(parsed.options.delay_ms, 25);
        assert_eq!(
            parsed.options.failure_strategy,
            FailureStrategy::StopOnFailure
        );
    }

    #[test]
    fn formats_run_summary() {
        let summary = CollectionRunSummary {
            collection_name: "Demo".to_string(),
            total: 1,
            passed: 1,
            failed: 0,
            stopped_early: false,
            elapsed_ms: 12,
            results: vec![CollectionRunResult {
                index: 0,
                path: vec!["Demo".to_string(), "Health".to_string()],
                name: "Health".to_string(),
                method: "GET".to_string(),
                url: "http://localhost/health".to_string(),
                status: Some(200),
                success: true,
                elapsed_ms: 3,
                body_bytes: 2,
                pre_request_actions: vec!["set_header Authorization".to_string()],
                assertions: Vec::new(),
                error: None,
            }],
        };

        let output = format_collection_run_summary(&summary);

        assert!(output.contains("Demo: 1 passed, 0 failed, 1 total"));
        assert!(output.contains("[PASS] 200 GET http://localhost/health"));
        assert!(output.contains("pre-request 1"));
        assert!(!output.contains(" ms"));
        assert!(!output.contains(" B"));
    }
}
