use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub path: String,
    pub heading: Option<String>,
    pub line_start: usize,
    pub line_end: usize,
    pub text: String,
    pub file_hash: String,
    pub chunk_hash: String,
}

#[derive(Debug)]
pub struct Corpus {
    pub root: PathBuf,
    pub files: usize,
    pub file_hashes: BTreeMap<String, String>,
    pub chunks: Vec<Chunk>,
}

#[cfg(feature = "experimental-adapters")]
pub(crate) fn file_hashes(root: &Path) -> Result<(PathBuf, BTreeMap<String, String>)> {
    if !root.exists() {
        bail!("project root does not exist: {}", root.display());
    }
    if !root.is_dir() {
        bail!("project root is not a directory: {}", root.display());
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve project root {}", root.display()))?;
    let mut files = selected_files(&root)?;
    files.sort();
    let mut hashes = BTreeMap::new();
    for path in files {
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read selected document {}", path.display()))?;
        let relative = portable_path(path.strip_prefix(&root)?);
        hashes.insert(relative, blake3::hash(&bytes).to_hex().to_string());
    }
    Ok((root, hashes))
}

pub fn load(root: &Path) -> Result<Corpus> {
    if !root.exists() {
        bail!("project root does not exist: {}", root.display());
    }
    if !root.is_dir() {
        bail!("project root is not a directory: {}", root.display());
    }

    let root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve project root {}", root.display()))?;
    let mut files = selected_files(&root)?;
    files.sort();

    let mut file_hashes = BTreeMap::new();
    let mut chunks = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read selected document {}", path.display()))?;
        let relative = path
            .strip_prefix(&root)
            .with_context(|| format!("document escaped project root: {}", path.display()))?;
        let relative = portable_path(relative);
        file_hashes.insert(
            relative.clone(),
            blake3::hash(text.as_bytes()).to_hex().to_string(),
        );
        chunks.extend(chunk_markdown(&relative, &text));
    }

    Ok(Corpus {
        root,
        files: files.len(),
        file_hashes,
        chunks,
    })
}

fn selected_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .require_git(false)
        .build();

    for entry in walker {
        let entry = entry.context("failed while traversing the project documentation")?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .with_context(|| format!("document escaped project root: {}", path.display()))?;
        if is_selected(relative) {
            files.push(path.to_path_buf());
        }
    }
    Ok(files)
}

fn is_selected(relative: &Path) -> bool {
    if relative.extension().and_then(|value| value.to_str()) != Some("md") {
        return false;
    }

    let components: Vec<_> = relative.components().collect();
    if matches!(components.first(), Some(Component::Normal(value)) if *value == "docs") {
        return true;
    }
    if components.len() != 1 {
        return false;
    }

    matches!(
        relative.file_name().and_then(|value| value.to_str()),
        Some("README.md" | "CLAUDE.md" | "AGENTS.md" | "pi-warden.md")
    )
}

fn portable_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    let depth = trimmed.bytes().take_while(|byte| *byte == b'#').count();
    if !(1..=6).contains(&depth) {
        return None;
    }
    let rest = trimmed.get(depth..)?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let title = rest.trim();
    let without_closing_hashes = title.trim_end_matches('#');
    let title = if without_closing_hashes.len() < title.len()
        && without_closing_hashes.ends_with(char::is_whitespace)
    {
        without_closing_hashes.trim_end()
    } else {
        title
    };
    (!title.is_empty()).then(|| (depth, title.to_owned()))
}

fn fence_marker(line: &str) -> Option<(u8, usize)> {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let trimmed = &line[indent..];
    let marker = *trimmed.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let run = trimmed.bytes().take_while(|byte| *byte == marker).count();
    (run >= 3).then_some((marker, run))
}

fn closes_fence(line: &str, marker: u8, opener_run: usize) -> bool {
    let indent = line.bytes().take_while(|byte| *byte == b' ').count();
    if indent > 3 {
        return false;
    }
    let trimmed = &line[indent..];
    let run = trimmed.bytes().take_while(|byte| *byte == marker).count();
    if run < opener_run {
        return false;
    }
    trimmed[run..].trim().is_empty()
}

fn chunk_markdown(path: &str, text: &str) -> Vec<Chunk> {
    let lines: Vec<_> = text.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let file_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
    let mut chunks = Vec::new();
    let mut heading_stack: Vec<String> = Vec::new();
    let mut current_heading = None;
    let mut fence: Option<(u8, usize)> = None;
    let mut start = 0;

    for (index, line) in lines.iter().enumerate() {
        if let Some((marker, opener_run)) = fence {
            if closes_fence(line, marker, opener_run) {
                fence = None;
            }
            continue;
        }

        if let Some(opener) = fence_marker(line) {
            fence = Some(opener);
            continue;
        }

        let Some((depth, title)) = heading(line) else {
            continue;
        };

        push_chunk(
            &mut chunks,
            path,
            &lines,
            start,
            index,
            current_heading.clone(),
            &file_hash,
        );

        heading_stack.truncate(depth.saturating_sub(1));
        while heading_stack.len() < depth.saturating_sub(1) {
            heading_stack.push(String::new());
        }
        heading_stack.push(title);
        current_heading = Some(
            heading_stack
                .iter()
                .filter(|part| !part.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join(" > "),
        );
        start = index;
    }

    push_chunk(
        &mut chunks,
        path,
        &lines,
        start,
        lines.len(),
        current_heading,
        &file_hash,
    );
    chunks
}

#[allow(clippy::too_many_arguments)]
fn push_chunk(
    chunks: &mut Vec<Chunk>,
    path: &str,
    lines: &[&str],
    start: usize,
    end: usize,
    heading: Option<String>,
    file_hash: &str,
) {
    if start >= end {
        return;
    }
    let text = lines[start..end].join("\n");
    if text.trim().is_empty() {
        return;
    }
    let chunk_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
    chunks.push(Chunk {
        path: path.to_owned(),
        heading,
        line_start: start + 1,
        line_end: end,
        text,
        file_hash: file_hash.to_owned(),
        chunk_hash,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_markdown_by_heading_and_keeps_breadcrumbs() {
        let chunks = chunk_markdown(
            "docs/example.md",
            "intro\n# Install\ntext\n## Linux\ncommand\n# Usage\nend\n",
        );

        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].heading, None);
        assert_eq!(chunks[1].heading.as_deref(), Some("Install"));
        assert_eq!(chunks[2].heading.as_deref(), Some("Install > Linux"));
        assert_eq!(chunks[2].line_start, 4);
        assert_eq!(chunks[2].line_end, 5);
        assert_eq!(chunks[3].heading.as_deref(), Some("Usage"));
    }

    #[test]
    fn selects_only_documentation_corpus() {
        assert!(is_selected(Path::new("docs/runbooks/example.md")));
        assert!(is_selected(Path::new("README.md")));
        assert!(is_selected(Path::new("CLAUDE.md")));
        assert!(!is_selected(Path::new("src/README.md")));
        assert!(!is_selected(Path::new(".env")));
        assert!(!is_selected(Path::new("skills/example/SKILL.md")));
    }

    #[test]
    fn preserves_hash_characters_that_are_part_of_a_heading() {
        assert_eq!(heading("## C#"), Some((2, "C#".to_owned())));
        assert_eq!(heading("## Install ##"), Some((2, "Install".to_owned())));
    }

    #[test]
    fn ignores_hash_comments_inside_backtick_fences() {
        let chunks = chunk_markdown(
            "docs/example.md",
            "intro\n# Install\n```bash\n# comment, not a heading\napt install demo\n```\n# Usage\nend\n",
        );

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].heading, None);
        assert_eq!(chunks[0].text, "intro");
        assert_eq!(chunks[1].heading.as_deref(), Some("Install"));
        assert_eq!(
            chunks[1].text,
            "# Install\n```bash\n# comment, not a heading\napt install demo\n```"
        );
        assert_eq!(chunks[2].heading.as_deref(), Some("Usage"));
    }

    #[test]
    fn resumes_heading_breadcrumbs_after_backtick_fence() {
        let chunks = chunk_markdown(
            "docs/example.md",
            "# Install\n```bash\n#### deep fake\n```\n## Linux\ncommand\n### Sub\nmore\n",
        );

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].heading.as_deref(), Some("Install"));
        assert_eq!(chunks[1].heading.as_deref(), Some("Install > Linux"));
        assert_eq!(chunks[2].heading.as_deref(), Some("Install > Linux > Sub"));
    }

    #[test]
    fn ignores_headings_inside_tilde_fences() {
        let chunks = chunk_markdown("docs/example.md", "# A\n~~~\n# fake\n~~~\n# B\nafter\n");

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading.as_deref(), Some("A"));
        assert_eq!(chunks[0].text, "# A\n~~~\n# fake\n~~~");
        assert_eq!(chunks[1].heading.as_deref(), Some("B"));
    }

    #[test]
    fn requires_matching_closing_fence_of_sufficient_length() {
        let chunks = chunk_markdown(
            "docs/example.md",
            "# A\n````\n```\n~~~\n# still fenced\n````\n# B\nafter\n",
        );

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].heading.as_deref(), Some("A"));
        assert_eq!(chunks[0].text, "# A\n````\n```\n~~~\n# still fenced\n````");
        assert_eq!(chunks[1].heading.as_deref(), Some("B"));
    }

    #[test]
    fn treats_unclosed_fence_as_part_of_the_current_chunk() {
        let chunks = chunk_markdown(
            "docs/example.md",
            "# A\n```bash\n# fake\nno closing fence\n",
        );

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].heading.as_deref(), Some("A"));
        assert_eq!(chunks[0].text, "# A\n```bash\n# fake\nno closing fence");
    }
}
