use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use docs_search::{SearchRequest, SearchResponse, search};

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

        /// Emit the stable machine-readable JSON contract.
        #[arg(long)]
        json: bool,
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
            json,
        } => {
            let response = search(SearchRequest {
                root,
                query,
                limit,
                max_excerpt_chars,
            })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                print_human(&response);
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
        println!(
            "\n{}. {}:{}-{}{} [score {:.6}]",
            result.rank, result.path, result.line_start, result.line_end, heading, result.score
        );
        println!("{}", result.excerpt);
    }
}
