//! Where a plugin comes from, and how to get it onto disk.
//!
//! Four accepted spellings, all resolving to a local directory:
//!
//! ```text
//! owner/repo                                  → GitHub, default branch
//! github:owner/repo@v1.2.0                    → GitHub, explicit ref
//! https://github.com/owner/repo/tree/main/x   → GitHub, ref + subdirectory
//! https://gitlab.com/owner/repo.git           → any git remote (needs `git`)
//! ./path/to/plugin                            → a directory already on disk
//! ./path/to/plugin.zip                        → a zip archive already on disk
//! ```
//!
//! GitHub is fetched as a codeload zip rather than cloned: no `git` binary
//! required, one request, and no `.git` directory left in the install dir.
//! Other remotes fall back to `git clone --depth 1`, which does need `git` on
//! PATH — we say so plainly instead of failing with a transport error.

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Http(String),
    #[error("archive error: {0}")]
    Archive(String),
    #[error("unsupported source: {0}")]
    Unsupported(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("git is required to install from {0}, but no `git` was found on PATH")]
    GitMissing(String),
    #[error("git failed: {0}")]
    Git(String),
}

/// A parsed install source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    Local {
        path: PathBuf,
    },
    GitHub {
        owner: String,
        repo: String,
        /// Branch, tag or commit. `None` = repository default branch.
        git_ref: Option<String>,
        /// Subdirectory inside the repo holding the plugin.
        subdir: Option<String>,
    },
    Git {
        url: String,
        git_ref: Option<String>,
        subdir: Option<String>,
    },
}

impl InstallSource {
    /// Canonical form, stored on the extension row so an update can re-fetch
    /// from the same place.
    pub fn canonical(&self) -> String {
        match self {
            Self::Local { path } => path.display().to_string(),
            Self::GitHub {
                owner,
                repo,
                git_ref,
                subdir,
            } => {
                let mut s = format!("github:{owner}/{repo}");
                if let Some(r) = git_ref {
                    s.push('@');
                    s.push_str(r);
                }
                if let Some(d) = subdir {
                    s.push('#');
                    s.push_str(d);
                }
                s
            }
            Self::Git {
                url,
                git_ref,
                subdir,
            } => {
                let mut s = url.clone();
                if let Some(r) = git_ref {
                    s.push('@');
                    s.push_str(r);
                }
                if let Some(d) = subdir {
                    s.push('#');
                    s.push_str(d);
                }
                s
            }
        }
    }

    /// A short human label for the UI.
    pub fn label(&self) -> String {
        match self {
            Self::Local { path } => path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            Self::GitHub { owner, repo, .. } => format!("{owner}/{repo}"),
            Self::Git { url, .. } => url.clone(),
        }
    }
}

/// Parse an install spec. Never touches the network.
pub fn parse(spec: &str) -> Result<InstallSource, SourceError> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(SourceError::Unsupported("empty source".into()));
    }

    // npm-hosted plugins would need a Node toolchain to be useful; say so
    // rather than downloading a tarball we cannot run.
    if let Some(rest) = spec.strip_prefix("npm:") {
        return Err(SourceError::Unsupported(format!(
            "npm package `{rest}` — Locaryn installs plugins from git or a local \
             directory. Clone it and install from the folder."
        )));
    }

    // An explicit path, or anything that already exists on disk.
    let looks_like_path = spec.starts_with('.')
        || spec.starts_with('/')
        || spec.starts_with('~')
        || spec.starts_with('\\')
        || (spec.len() > 2 && spec.as_bytes()[1] == b':');
    if looks_like_path || Path::new(spec).is_dir() {
        return Ok(InstallSource::Local {
            path: PathBuf::from(spec),
        });
    }

    if let Some(rest) = spec.strip_prefix("github:") {
        let (owner_repo, git_ref, subdir) = split_ref_and_subdir(rest);
        let (owner, repo) = split_owner_repo(&owner_repo)?;
        return Ok(InstallSource::GitHub {
            owner,
            repo,
            git_ref,
            subdir,
        });
    }

    if spec.starts_with("http://") || spec.starts_with("https://") {
        return parse_url(spec);
    }

    if spec.starts_with("git@") || spec.starts_with("ssh://") || spec.ends_with(".git") {
        let (url, git_ref, subdir) = split_ref_and_subdir(spec);
        return Ok(InstallSource::Git {
            url,
            git_ref,
            subdir,
        });
    }

    // Bare `owner/repo` shorthand.
    let (owner_repo, git_ref, subdir) = split_ref_and_subdir(spec);
    if owner_repo.matches('/').count() == 1 && !owner_repo.contains(' ') {
        let (owner, repo) = split_owner_repo(&owner_repo)?;
        return Ok(InstallSource::GitHub {
            owner,
            repo,
            git_ref,
            subdir,
        });
    }

    Err(SourceError::Unsupported(format!(
        "`{spec}` — expected owner/repo, a git URL, or a local directory"
    )))
}

/// `https://github.com/owner/repo[/tree/<ref>/<subdir>]`, plus any other https
/// git remote.
fn parse_url(spec: &str) -> Result<InstallSource, SourceError> {
    let (url, explicit_ref, explicit_subdir) = split_ref_and_subdir(spec);
    let rest = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let mut parts = rest.split('/');
    let host = parts.next().unwrap_or_default();

    if host.eq_ignore_ascii_case("github.com") || host.eq_ignore_ascii_case("www.github.com") {
        let owner = parts.next().unwrap_or_default().to_string();
        let repo = parts
            .next()
            .unwrap_or_default()
            .trim_end_matches(".git")
            .to_string();
        if owner.is_empty() || repo.is_empty() {
            return Err(SourceError::Unsupported(format!(
                "`{spec}` is not a GitHub repository URL"
            )));
        }
        // `/tree/<ref>/<subdir…>` or `/blob/<ref>/<subdir…>`
        let mut git_ref = explicit_ref;
        let mut subdir = explicit_subdir;
        if matches!(parts.next(), Some("tree") | Some("blob")) {
            if let Some(r) = parts.next() {
                git_ref = Some(r.to_string());
            }
            let tail: Vec<&str> = parts.collect();
            if !tail.is_empty() {
                subdir = Some(tail.join("/"));
            }
        }
        return Ok(InstallSource::GitHub {
            owner,
            repo,
            git_ref,
            subdir,
        });
    }

    Ok(InstallSource::Git {
        url,
        git_ref: explicit_ref,
        subdir: explicit_subdir,
    })
}

/// Split `thing@ref#subdir` into its three parts. The `@` is only treated as a
/// ref separator after the last `/`, so `git@github.com:o/r` survives intact.
fn split_ref_and_subdir(spec: &str) -> (String, Option<String>, Option<String>) {
    let (head, subdir) = match spec.split_once('#') {
        Some((h, d)) if !d.is_empty() => (h, Some(d.to_string())),
        _ => (spec, None),
    };
    let last_slash = head.rfind('/').map(|i| i + 1).unwrap_or(0);
    match head[last_slash..].rfind('@') {
        Some(rel) if rel > 0 => {
            let at = last_slash + rel;
            (
                head[..at].to_string(),
                Some(head[at + 1..].to_string()),
                subdir,
            )
        }
        _ => (head.to_string(), None, subdir),
    }
}

fn split_owner_repo(s: &str) -> Result<(String, String), SourceError> {
    let mut it = s.trim_matches('/').splitn(2, '/');
    let owner = it.next().unwrap_or_default().trim().to_string();
    let repo = it
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches(".git")
        .to_string();
    if owner.is_empty() || repo.is_empty() {
        return Err(SourceError::Unsupported(format!(
            "`{s}` — expected owner/repo"
        )));
    }
    Ok((owner, repo))
}

/// Fetch the source into `dest` (created fresh) and return the directory that
/// actually holds the plugin — `dest` itself, or a subdirectory of it when the
/// source named one.
pub async fn fetch(
    http: &reqwest::Client,
    source: &InstallSource,
    dest: &Path,
) -> Result<PathBuf, SourceError> {
    match source {
        InstallSource::Local { path } => {
            let path = expand_home(path);
            if path.is_dir() {
                copy_dir(&path, dest)?;
                Ok(dest.to_path_buf())
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
            {
                // A zip picked on disk installs like a GitHub archive: the
                // single generated root (if there is exactly one) is stripped
                // and the plugin lands directly in `dest`.
                let bytes = std::fs::read(&path)?;
                extract_zip_stripping_root(&bytes, dest)?;
                Ok(dest.to_path_buf())
            } else {
                Err(SourceError::Unsupported(format!(
                    "`{}` exists but is neither a directory nor a .zip archive",
                    path.display()
                )))
            }
        }
        InstallSource::GitHub {
            owner,
            repo,
            git_ref,
            subdir,
        } => {
            let bytes = download_github_zip(http, owner, repo, git_ref.as_deref()).await?;
            extract_zip_stripping_root(&bytes, dest)?;
            resolve_subdir(dest, subdir.as_deref())
        }
        InstallSource::Git {
            url,
            git_ref,
            subdir,
        } => {
            git_clone(url, git_ref.as_deref(), dest)?;
            resolve_subdir(dest, subdir.as_deref())
        }
    }
}

fn resolve_subdir(root: &Path, subdir: Option<&str>) -> Result<PathBuf, SourceError> {
    let Some(sub) = subdir else {
        return Ok(root.to_path_buf());
    };
    // Reject traversal: a plugin may only be fetched from inside its own tree.
    if sub.split(['/', '\\']).any(|c| c == ".." || c.is_empty()) {
        return Err(SourceError::Unsupported(format!(
            "invalid subdirectory `{sub}`"
        )));
    }
    let p = root.join(sub.replace('\\', "/"));
    if !p.is_dir() {
        return Err(SourceError::NotFound(format!(
            "subdirectory `{sub}` does not exist in the fetched source"
        )));
    }
    Ok(p)
}

/// GitHub's codeload endpoint serves a zip of any ref without authentication.
/// With no ref we try the two conventional default branches before asking the
/// API, so the common case costs one request and no rate-limited API call.
async fn download_github_zip(
    http: &reqwest::Client,
    owner: &str,
    repo: &str,
    git_ref: Option<&str>,
) -> Result<Vec<u8>, SourceError> {
    let mut candidates: Vec<String> = Vec::new();
    match git_ref {
        Some(r) => {
            candidates.push(format!("refs/heads/{r}"));
            candidates.push(format!("refs/tags/{r}"));
            // Bare commit SHAs are valid path segments on their own.
            candidates.push(r.to_string());
        }
        None => {
            candidates.push("refs/heads/main".into());
            candidates.push("refs/heads/master".into());
        }
    }

    let mut last_status = None;
    for cand in &candidates {
        let url = format!("https://codeload.github.com/{owner}/{repo}/zip/{cand}");
        match http.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| SourceError::Http(e.to_string()))?;
                return Ok(bytes.to_vec());
            }
            Ok(resp) => last_status = Some(resp.status()),
            Err(e) => return Err(SourceError::Http(e.to_string())),
        }
    }

    // Neither conventional branch existed — ask for the real default branch.
    if git_ref.is_none() {
        let api = format!("https://api.github.com/repos/{owner}/{repo}");
        if let Ok(resp) = http
            .get(&api)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
        {
            if resp.status().is_success() {
                if let Ok(v) = resp.json::<serde_json::Value>().await {
                    if let Some(branch) = v.get("default_branch").and_then(|b| b.as_str()) {
                        let url = format!(
                            "https://codeload.github.com/{owner}/{repo}/zip/refs/heads/{branch}"
                        );
                        let resp = http
                            .get(&url)
                            .send()
                            .await
                            .map_err(|e| SourceError::Http(e.to_string()))?;
                        if resp.status().is_success() {
                            let bytes = resp
                                .bytes()
                                .await
                                .map_err(|e| SourceError::Http(e.to_string()))?;
                            return Ok(bytes.to_vec());
                        }
                    }
                }
            }
        }
    }

    Err(SourceError::NotFound(match last_status {
        Some(s) if s.as_u16() == 404 => format!(
            "github.com/{owner}/{repo}{} — repository, branch or tag not found",
            git_ref.map(|r| format!(" @ {r}")).unwrap_or_default()
        ),
        Some(s) => format!("github.com/{owner}/{repo} — GitHub answered {s}"),
        None => format!("github.com/{owner}/{repo}"),
    }))
}

/// Latest `version` declared by a GitHub-hosted plugin, read from its manifest
/// at the repository's default branch (`HEAD`). Powers the "mise à jour dispo"
/// badge.
///
/// `None` when the source is not a GitHub repo, pins an explicit ref (a pinned
/// version is a target, not a moving branch), or exposes no manifest carrying
/// a version.
pub async fn latest_github_version(
    http: &reqwest::Client,
    spec: &str,
) -> Result<Option<String>, SourceError> {
    let Ok(parsed) = parse(spec) else {
        return Ok(None);
    };
    let InstallSource::GitHub {
        owner,
        repo,
        git_ref,
        subdir,
    } = parsed
    else {
        return Ok(None);
    };
    if git_ref.is_some() {
        return Ok(None);
    }
    let prefix = subdir
        .map(|d| format!("{}/", d.replace('\\', "/")))
        .unwrap_or_default();
    for name in [
        "plugin.json",
        ".claude-plugin/plugin.json",
        "gemini-extension.json",
        "opencode.json",
    ] {
        let url = format!("https://raw.githubusercontent.com/{owner}/{repo}/HEAD/{prefix}{name}");
        let resp = http
            .get(&url)
            .send()
            .await
            .map_err(|e| SourceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            continue;
        }
        let Ok(v) = resp.json::<serde_json::Value>().await else {
            continue;
        };
        if let Some(version) = v.get("version").and_then(|x| x.as_str()) {
            return Ok(Some(version.to_string()));
        }
    }
    Ok(None)
}

/// True when `a` is a newer version than `b`. Compares dot-separated numeric
/// segments; a segment that is not numeric falls back to a plain string
/// comparison of that segment.
pub fn version_gt(a: &str, b: &str) -> bool {
    let a: Vec<&str> = a.split('.').collect();
    let b: Vec<&str> = b.split('.').collect();
    for i in 0..a.len().max(b.len()) {
        let an = a.get(i).and_then(|s| s.parse::<u64>().ok());
        let bn = b.get(i).and_then(|s| s.parse::<u64>().ok());
        match (an, bn) {
            (Some(x), Some(y)) if x != y => return x > y,
            (Some(_), Some(_)) => {}
            _ => {
                let as_ = a.get(i).copied().unwrap_or("0");
                let bs = b.get(i).copied().unwrap_or("0");
                if as_ != bs {
                    return as_ > bs;
                }
            }
        }
    }
    false
}

/// Extract a GitHub archive. Every entry sits under a single generated root
/// (`repo-branch/`), which we strip so the plugin lands directly in `dest`.
fn extract_zip_stripping_root(bytes: &[u8], dest: &Path) -> Result<(), SourceError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| SourceError::Archive(e.to_string()))?;

    // Determine the common root, if there is exactly one.
    let mut root: Option<String> = None;
    let mut single_root = true;
    for i in 0..zip.len() {
        let f = zip
            .by_index(i)
            .map_err(|e| SourceError::Archive(e.to_string()))?;
        let name = f.name().replace('\\', "/");
        let first = name.split('/').next().unwrap_or_default().to_string();
        if first.is_empty() {
            continue;
        }
        match &root {
            None => root = Some(first),
            Some(r) if *r != first => {
                single_root = false;
                break;
            }
            _ => {}
        }
    }
    let strip = if single_root { root } else { None };

    std::fs::create_dir_all(dest)?;
    for i in 0..zip.len() {
        let mut f = zip
            .by_index(i)
            .map_err(|e| SourceError::Archive(e.to_string()))?;
        let raw = f.name().replace('\\', "/");
        let rel = match &strip {
            Some(r) => raw
                .strip_prefix(r.as_str())
                .map(|s| s.trim_start_matches('/'))
                .unwrap_or(raw.as_str()),
            None => raw.as_str(),
        };
        if rel.is_empty() {
            continue;
        }
        // Zip-slip guard: never write outside `dest`.
        if rel.split('/').any(|c| c == "..") {
            return Err(SourceError::Archive(format!(
                "archive entry escapes the target directory: {raw}"
            )));
        }
        let out = dest.join(rel);
        if f.is_dir() || rel.ends_with('/') {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut buf)?;
        let mut handle = std::fs::File::create(&out)?;
        handle.write_all(&buf)?;
    }
    Ok(())
}

fn git_clone(url: &str, git_ref: Option<&str>, dest: &Path) -> Result<(), SourceError> {
    std::fs::create_dir_all(dest)?;
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(r) = git_ref {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(url).arg(dest);
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(SourceError::GitMissing(url.to_string()))
        }
        Err(e) => return Err(SourceError::Io(e)),
    };
    if !out.status.success() {
        return Err(SourceError::Git(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    // Drop the clone metadata: the installed copy is a snapshot, not a checkout.
    let _ = std::fs::remove_dir_all(dest.join(".git"));
    Ok(())
}

pub(crate) fn expand_home(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(rest);
        }
    }
    p.to_path_buf()
}

/// Recursive directory copy. Skips `.git` and `node_modules`, which are never
/// part of what a plugin contributes and can dwarf the plugin itself.
pub fn copy_dir(src: &Path, dest: &Path) -> Result<(), SourceError> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str == ".git" || name_str == "node_modules" || name_str == "target" {
            continue;
        }
        let from = entry.path();
        let to = dest.join(&name);
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_owner_repo() {
        assert_eq!(
            parse("anthropics/claude-code").unwrap(),
            InstallSource::GitHub {
                owner: "anthropics".into(),
                repo: "claude-code".into(),
                git_ref: None,
                subdir: None,
            }
        );
    }

    #[test]
    fn parses_ref_and_subdir() {
        let s = parse("github:acme/tools@v1.2.0#plugins/formatter").unwrap();
        assert_eq!(
            s,
            InstallSource::GitHub {
                owner: "acme".into(),
                repo: "tools".into(),
                git_ref: Some("v1.2.0".into()),
                subdir: Some("plugins/formatter".into()),
            }
        );
        assert_eq!(s.canonical(), "github:acme/tools@v1.2.0#plugins/formatter");
    }

    #[test]
    fn parses_github_tree_url() {
        assert_eq!(
            parse("https://github.com/acme/tools/tree/main/plugins/fmt").unwrap(),
            InstallSource::GitHub {
                owner: "acme".into(),
                repo: "tools".into(),
                git_ref: Some("main".into()),
                subdir: Some("plugins/fmt".into()),
            }
        );
    }

    #[test]
    fn ssh_remote_keeps_its_at_sign() {
        assert_eq!(
            parse("git@github.com:acme/tools.git").unwrap(),
            InstallSource::Git {
                url: "git@github.com:acme/tools.git".into(),
                git_ref: None,
                subdir: None,
            }
        );
    }

    #[test]
    fn local_paths_are_local() {
        assert!(matches!(
            parse("./examples/plugins/my-plugin").unwrap(),
            InstallSource::Local { .. }
        ));
        assert!(matches!(
            parse("C:/tmp/plugin").unwrap(),
            InstallSource::Local { .. }
        ));
    }

    #[test]
    fn version_comparison_is_numeric_on_dot_segments() {
        assert!(version_gt("1.2.1", "1.2.0"));
        assert!(!version_gt("1.2.0", "1.2.1"));
        assert!(version_gt("2.0.0", "1.9.9"));
        assert!(version_gt("1.10.0", "1.9.0"));
        assert!(!version_gt("1.0.0", "1.0.0"));
        // A non-numeric segment falls back to string comparison.
        assert!(version_gt("0.2.0-rc", "0.1.0"));
    }

    #[test]
    fn latest_github_version_ignores_local_and_pinned_sources() {
        // Local paths and explicit refs are not checkable: no error, just None.
        let http = reqwest::Client::new();
        let local = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(latest_github_version(&http, "C:/tmp/plugin"));
        assert!(matches!(local, Ok(None)));
        let pinned = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(latest_github_version(&http, "github:acme/tools@v1.2.0"));
        assert!(matches!(pinned, Ok(None)));
    }

    #[tokio::test]
    async fn local_zip_extracts_and_strips_the_single_root() {
        use std::io::Write;

        let base = std::env::temp_dir().join("locaryn-src-zip");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        // Build a zip whose entries all sit under one generated root, exactly
        // like GitHub's codeload archives.
        let zip_path = base.join("plugin.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        for (name, content) in [
            (
                "repo-main/plugin.json",
                r#"{"name":"zip-plugin","version":"1.0.0"}"#,
            ),
            ("repo-main/commands/go.md", "---\nname: go\n---\nGo"),
        ] {
            writer.start_file(name, options).expect("start_file");
            writer.write_all(content.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");

        let dest = base.join("out");
        let http = reqwest::Client::new();
        let dir = fetch(
            &http,
            &InstallSource::Local {
                path: zip_path.clone(),
            },
            &dest,
        )
        .await
        .expect("zip installs");

        assert_eq!(dir, dest);
        assert!(dest.join("plugin.json").is_file(), "root must be stripped");
        assert!(
            !dest.join("repo-main/plugin.json").is_file(),
            "generated root must not survive"
        );
    }

    #[test]
    fn npm_is_refused_with_a_reason() {
        let err = parse("npm:@scope/thing").unwrap_err();
        assert!(err.to_string().contains("npm package"));
    }

    #[test]
    fn subdir_traversal_is_rejected() {
        let tmp = std::env::temp_dir().join("locaryn-src-test");
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(resolve_subdir(&tmp, Some("../escape")).is_err());
    }
}
