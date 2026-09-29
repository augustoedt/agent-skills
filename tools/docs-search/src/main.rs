use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use docs_search::{
    EvaluateRequest, EvaluationReport, SearchRequest, SearchResponse, evaluate, search,
};

#[derive(Debug, Parser)]
#[command(
    name = "docs-search",
    version,
    about = "Search project documentation with minimal sufficient context"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Search Markdown documentation under a project root.
    Search {
        /// Project root whose documentation should be searched.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Natural-language or keyword query.
        #[arg(long)]
        query: String,

        /// Maximum number of ranked chunks.
        #[arg(long, default_value_t = 5)]
        limit: usize,

        /// Maximum excerpt size per result, in Unicode characters.
        #[arg(long, default_value_t = 1_200)]
        max_excerpt_chars: usize,

        /// Optional maximum number of selected chunks from the same path.
        #[arg(long)]
        max_results_per_path: Option<usize>,

        /// Emit the stable machine-readable JSON contract.
        #[arg(long)]
        json: bool,
    },

    /// Run a versioned retrieval evaluation dataset and calculate metrics.
    Evaluate {
        /// Project root whose documentation should be evaluated.
        #[arg(long, default_value = ".")]
        root: PathBuf,

        /// Versioned evaluation dataset.
        #[arg(long)]
        queries: PathBuf,

        /// Maximum ranked chunks per query; must be at least 5 for Recall@5.
        #[arg(long, default_value_t = 5)]
        limit: usize,

        /// Maximum excerpt size per result, in Unicode characters.
        #[arg(long, default_value_t = 1_200)]
        max_excerpt_chars: usize,

        /// Optional maximum number of selected chunks from the same path.
        #[arg(long)]
        max_results_per_path: Option<usize>,

        /// Emit the versioned machine-readable evaluation report.
        #[arg(long)]
        json: bool,

        /// Explicitly write the JSON report to this file.
        #[arg(long)]
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Search {
            root,
            query,
            limit,
            max_excerpt_chars,
            max_results_per_path,
            json,
        } => {
            let response = search(SearchRequest {
                root,
                query,
                limit,
                max_excerpt_chars,
                max_results_per_path,
            })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_human(&response);
            }
        }
        Command::Evaluate {
            root,
            queries,
            limit,
            max_excerpt_chars,
            max_results_per_path,
            json,
            output,
        } => {
            let report = evaluate(EvaluateRequest {
                root,
                queries_path: queries,
                limit,
                max_excerpt_chars,
                max_results_per_path,
            })?;
            let report_json = serde_json::to_string_pretty(&report)?;
            if let Some(path) = output {
                fs::write(&path, format!("{report_json}\n")).with_context(|| {
                    format!("failed to write evaluation report {}", path.display())
                })?;
            }
            if json {
                println!("{report_json}");
            } else {
                print_evaluation_human(&report);
            }
            if report.has_failures() {
                bail!(
                    "evaluation completed with {} failed query or queries",
                    report.summary.failed
                );
            }
        }
    }
    Ok(())
}

fn print_human(response: &SearchResponse) {
    println!(
        "{} result(s) across {} file(s), engine {}",
        response.results.len(),
        response.corpus.files,
        response.engine
    );
    for result in &response.results {
        let heading = result
            .heading
            .as_deref()
            .map(|value| format!(" — {value}"))
            .unwrap_or_default();
        let raw_rank = if result.raw_rank != result.rank {
            format!(", raw rank {}", result.raw_rank)
        } else {
            String::new()
        };
        println!(
            "\n{}. {}:{}-{}{} [score {:.6}{}]",
            result.rank,
            result.path,
            result.line_start,
            result.line_end,
            heading,
            result.score,
            raw_rank
        );
        println!("{}", result.excerpt);
    }
}

fn print_evaluation_human(report: &EvaluationReport) {
    let summary = &report.summary;
    println!(
        "{} evaluation query or queries across {} file(s), engine {}",
        summary.queries_total, report.corpus.files, report.engine
    );
    println!(
        "executed {}, skipped {}, failed {}",
        summary.executed, summary.skipped, summary.failed
    );
    println!(
        "Hit@1 {} | Recall@5 macro {} | Recall@5 micro {} | MRR@5 {}",
        metric(summary.hit_at_1),
        metric(summary.recall_at_5_macro),
        metric(summary.recall_at_5_micro),
        metric(summary.mrr_at_5)
    );
    println!(
        "no-answer false positives {}/{} ({})",
        summary.no_answer_false_positives,
        summary.no_answer,
        metric(summary.no_answer_false_positive_rate)
    );
    println!(
        "latency p50 {} ms, p95 {} ms, total {:.3} ms",
        metric(summary.latency_ms.p50),
        metric(summary.latency_ms.p95),
        summary.latency_ms.total
    );
    println!(
        "context mean {} chars, p95 {}, total {} chars",
        metric(summary.context_chars.mean),
        summary
            .context_chars
            .p95
            .map(|value| value.to_string())
            .unwrap_or_else(|| "n/a".to_owned()),
        summary.context_chars.total
    );
}

fn metric(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "n/a".to_owned())
}
