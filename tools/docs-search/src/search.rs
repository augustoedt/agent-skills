use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use crate::corpus::{Chunk, load};
use crate::types::{
    CorpusSummary, ENGINE, SCHEMA_VERSION, SearchRequest, SearchResponse, SearchResult,
};

const K1: f64 = 1.2;
const B: f64 = 0.75;

#[derive(Debug)]
struct PreparedChunk<'a> {
    chunk: &'a Chunk,
    normalized: String,
    tokens: Vec<String>,
    heading: String,
    path: String,
}

#[derive(Debug)]
struct RankedChunk<'a> {
    prepared: &'a PreparedChunk<'a>,
    score: f64,
    matched_terms: Vec<String>,
}

pub fn search(request: SearchRequest) -> Result<SearchResponse> {
    if request.query.trim().is_empty() {
        bail!("query must not be empty");
    }
    if request.limit == 0 || request.limit > 100 {
        bail!("limit must be between 1 and 100");
    }
    if request.max_excerpt_chars < 80 {
        bail!("max excerpt size must be at least 80 characters");
    }
    if request
        .max_results_per_path
        .is_some_and(|maximum| maximum == 0 || maximum > 100)
    {
        bail!("max results per path must be between 1 and 100");
    }

    let corpus = load(&request.root)?;
    let query_normalized = normalize(&request.query);
    let query_tokens = meaningful_query_tokens(&request.query);
    if query_tokens.is_empty() {
        bail!("query must contain at least one letter or number");
    }

    let prepared: Vec<_> = corpus
        .chunks
        .iter()
        .map(|chunk| PreparedChunk {
            chunk,
            normalized: normalize(&chunk.text),
            tokens: tokens(&chunk.text),
            heading: normalize(chunk.heading.as_deref().unwrap_or_default()),
            path: normalize(&chunk.path),
        })
        .collect();

    let average_length = if prepared.is_empty() {
        1.0
    } else {
        prepared
            .iter()
            .map(|chunk| chunk.tokens.len())
            .sum::<usize>() as f64
            / prepared.len() as f64
    };
    let document_frequencies = document_frequencies(&prepared, &query_tokens);
    let total_chunks = prepared.len() as f64;

    let mut ranked: Vec<_> = prepared
        .iter()
        .filter_map(|chunk| {
            rank_chunk(
                chunk,
                &query_tokens,
                &query_normalized,
                &document_frequencies,
                total_chunks,
                average_length,
            )
        })
        .collect();

    ranked.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.prepared.chunk.path.cmp(&right.prepared.chunk.path))
            .then_with(|| {
                left.prepared
                    .chunk
                    .line_start
                    .cmp(&right.prepared.chunk.line_start)
            })
    });

    let results = select_with_path_diversity(
        ranked,
        request.limit,
        request.max_results_per_path,
        |ranked| ranked.prepared.chunk.path.as_str(),
    )
    .into_iter()
    .enumerate()
    .map(|(index, (raw_rank, ranked))| {
        let (excerpt, line_start, line_end) = excerpt(
            ranked.prepared.chunk,
            &query_tokens,
            request.max_excerpt_chars,
        );
        SearchResult {
            rank: index + 1,
            raw_rank,
            path: ranked.prepared.chunk.path.clone(),
            heading: ranked.prepared.chunk.heading.clone(),
            line_start,
            line_end,
            excerpt,
            file_hash: ranked.prepared.chunk.file_hash.clone(),
            chunk_hash: ranked.prepared.chunk.chunk_hash.clone(),
            score: round_score(ranked.score),
            matched_terms: ranked.matched_terms,
        }
    })
    .collect();

    Ok(SearchResponse {
        schema_version: SCHEMA_VERSION,
        engine: ENGINE,
        query: request.query,
        root: corpus.root.to_string_lossy().into_owned(),
        corpus: CorpusSummary {
            files: corpus.files,
            chunks: corpus.chunks.len(),
        },
        results,
    })
}

fn select_with_path_diversity<T, F>(
    ranked: Vec<T>,
    limit: usize,
    max_results_per_path: Option<usize>,
    path: F,
) -> Vec<(usize, T)>
where
    F: Fn(&T) -> &str,
{
    let mut selected = Vec::with_capacity(limit);
    let mut path_counts = HashMap::new();

    for (index, item) in ranked.into_iter().enumerate() {
        if let Some(maximum) = max_results_per_path {
            let count = path_counts.entry(path(&item).to_owned()).or_insert(0usize);
            if *count >= maximum {
                continue;
            }
            *count += 1;
        }

        selected.push((index + 1, item));
        if selected.len() == limit {
            break;
        }
    }

    selected
}

fn document_frequencies(
    chunks: &[PreparedChunk<'_>],
    query_tokens: &[String],
) -> HashMap<String, usize> {
    query_tokens
        .iter()
        .map(|term| {
            let count = chunks
                .iter()
                .filter(|chunk| {
                    chunk.tokens.iter().any(|token| token == term)
                        || chunk.heading.split_whitespace().any(|token| token == term)
                        || chunk.path.split_whitespace().any(|token| token == term)
                })
                .count();
            (term.clone(), count)
        })
        .collect()
}

fn rank_chunk<'a>(
    chunk: &'a PreparedChunk<'a>,
    query_tokens: &[String],
    normalized_query: &str,
    frequencies: &HashMap<String, usize>,
    total_chunks: f64,
    average_length: f64,
) -> Option<RankedChunk<'a>> {
    let mut score = 0.0;
    let mut matched_terms = Vec::new();

    for term in query_tokens {
        let body_frequency = chunk.tokens.iter().filter(|token| *token == term).count() as f64;
        let heading_frequency = chunk
            .heading
            .split_whitespace()
            .filter(|token| *token == term)
            .count() as f64;
        let path_frequency = chunk
            .path
            .split(|character: char| !character.is_alphanumeric())
            .filter(|token| *token == term)
            .count() as f64;
        let weighted_frequency = body_frequency + heading_frequency * 2.0 + path_frequency * 3.0;
        if weighted_frequency == 0.0 {
            continue;
        }

        matched_terms.push(term.clone());
        let document_frequency = *frequencies.get(term).unwrap_or(&0) as f64;
        let inverse_document_frequency =
            (1.0 + (total_chunks - document_frequency + 0.5) / (document_frequency + 0.5)).ln();
        let length_normalization =
            K1 * (1.0 - B + B * (chunk.tokens.len() as f64 / average_length.max(1.0)));
        score += inverse_document_frequency * (weighted_frequency * (K1 + 1.0))
            / (weighted_frequency + length_normalization);
    }

    if matched_terms.len() < minimum_should_match(query_tokens.len()) {
        return None;
    }

    if query_tokens.len() > 1 && normalized_query.len() > 2 {
        if contains_phrase(&chunk.normalized, normalized_query) {
            score += 2.5;
        }
        if contains_phrase(&chunk.heading, normalized_query) {
            score += 3.0;
        }
        if contains_phrase(&chunk.path, normalized_query) {
            score += 2.0;
        }
    }

    Some(RankedChunk {
        prepared: chunk,
        score,
        matched_terms,
    })
}

fn minimum_should_match(query_length: usize) -> usize {
    match query_length {
        0 => 0,
        1 => 1,
        length => (length * 3).div_ceil(5),
    }
}

fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    let padded_haystack = format!(" {haystack} ");
    let padded_phrase = format!(" {phrase} ");
    padded_haystack.contains(&padded_phrase)
}

fn excerpt(chunk: &Chunk, query_tokens: &[String], max_chars: usize) -> (String, usize, usize) {
    let lines: Vec<_> = chunk.text.lines().collect();
    if lines.is_empty() {
        return (String::new(), chunk.line_start, chunk.line_start);
    }

    let match_index = lines
        .iter()
        .position(|line| {
            let normalized = normalize(line);
            query_tokens.iter().any(|term| {
                normalized
                    .split_whitespace()
                    .any(|candidate| candidate == term)
            })
        })
        .unwrap_or(0);
    let mut start = match_index.saturating_sub(2);
    let mut end = match_index + 1;
    let mut size = lines[start..end].join("\n").chars().count();

    while end < lines.len() {
        let addition = lines[end].chars().count() + 1;
        if size + addition > max_chars {
            break;
        }
        size += addition;
        end += 1;
    }
    while start > 0 {
        let addition = lines[start - 1].chars().count() + 1;
        if size + addition > max_chars {
            break;
        }
        size += addition;
        start -= 1;
    }

    let mut text = lines[start..end].join("\n");
    if text.chars().count() > max_chars {
        text = text.chars().take(max_chars).collect();
    }

    (
        text,
        chunk.line_start + start,
        chunk.line_start + end.saturating_sub(1),
    )
}

fn meaningful_query_tokens(query: &str) -> Vec<String> {
    let all = tokens(query);
    let filtered: Vec<_> = all
        .iter()
        .filter(|token| !is_stopword(token))
        .cloned()
        .collect();
    let selected = if filtered.is_empty() { all } else { filtered };
    let mut seen = HashSet::new();
    selected
        .into_iter()
        .filter(|token| seen.insert(token.clone()))
        .collect()
}

fn tokens(value: &str) -> Vec<String> {
    normalize(value)
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize(value: &str) -> String {
    value
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "a" | "as"
            | "ao"
            | "aos"
            | "como"
            | "da"
            | "das"
            | "de"
            | "do"
            | "dos"
            | "e"
            | "em"
            | "na"
            | "nas"
            | "no"
            | "nos"
            | "o"
            | "os"
            | "para"
            | "por"
            | "que"
            | "um"
            | "uma"
            | "the"
            | "to"
            | "of"
            | "and"
            | "in"
            | "for"
    )
}

fn round_score(score: f64) -> f64 {
    (score * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_removes_accents_and_punctuation() {
        assert_eq!(normalize("Emissão: São Paulo!"), "emissao sao paulo");
    }

    #[test]
    fn query_tokens_are_unique_and_ignore_common_words() {
        assert_eq!(
            meaningful_query_tokens("como instalar uma skill instalar"),
            vec!["instalar", "skill"]
        );
    }

    #[test]
    fn match_threshold_requires_most_query_terms() {
        assert_eq!(minimum_should_match(1), 1);
        assert_eq!(minimum_should_match(2), 2);
        assert_eq!(minimum_should_match(3), 2);
        assert_eq!(minimum_should_match(5), 3);
    }

    #[test]
    fn phrase_matching_respects_token_boundaries() {
        assert!(contains_phrase("local api client", "api client"));
        assert!(!contains_phrase("capital gains", "api"));
    }

    #[test]
    fn path_diversity_looks_beyond_the_initial_limit_and_preserves_raw_ranks() {
        let ranked = vec!["a", "a", "a", "b", "c", "c"];

        let selected = select_with_path_diversity(ranked, 4, Some(2), |path| path);

        assert_eq!(selected, vec![(1, "a"), (2, "a"), (4, "b"), (5, "c")]);
    }

    #[test]
    fn unlimited_selection_preserves_the_original_top_results() {
        let ranked = vec!["a", "a", "b", "c"];

        let selected = select_with_path_diversity(ranked, 3, None, |path| path);

        assert_eq!(selected, vec![(1, "a"), (2, "a"), (3, "b")]);
    }
}
