use std::sync::LazyLock;

// Helper for deriving image prefill suggestions from a container name.
use regex::Regex;

// Matches tags that are numeric-only (e.g. "1.2.3" or "1_2-3")
static VERSION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\d+(?:[._-]\d+)*)$").unwrap());
// Capture a numeric version inside a tag (e.g. "v1.2" -> captures "1.2")
static VER_CAPTURE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?P<ver>\d+(?:[._-]\d+)*)").unwrap());

// Compare two numeric-version vectors treating missing components as zeros.
fn compare_version_vec(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let max_len = std::cmp::max(a.len(), b.len());
    for i in 0..max_len {
        let va = *a.get(i).unwrap_or(&0);
        let vb = *b.get(i).unwrap_or(&0);
        match va.cmp(&vb) {
            Ordering::Less => return Ordering::Less,
            Ordering::Greater => return Ordering::Greater,
            Ordering::Equal => continue,
        }
    }
    // If all compared components are equal, prefer the longer vector (e.g. 1.2.0 > 1.2)
    a.len().cmp(&b.len())
}

pub fn split_repo_tag_digest(s: &str) -> (&str, Option<&str>, Option<&str>) {
    // Return (repo, tag_opt, digest_opt)
    let last_slash = s.rfind('/');
    if let Some(at_pos) = s.rfind('@') {
        if last_slash.map_or(true, |ls| at_pos > ls) {
            // If there is a tag before the @ (colon after last slash), strip it from repo
            let before_at = &s[..at_pos];
            if let Some(col_pos) = before_at.rfind(':') {
                if last_slash.map_or(true, |ls| col_pos > ls) {
                    return (&before_at[..col_pos], None, Some(&s[at_pos + 1..]));
                }
            }
            return (before_at, None, Some(&s[at_pos + 1..]));
        }
    }
    if let Some(col_pos) = s.rfind(':') {
        if last_slash.map_or(true, |ls| col_pos > ls) {
            return (&s[..col_pos], Some(&s[col_pos + 1..]), None);
        }
    }
    (s, None, None)
}

pub fn derive_image_prefill(
    container_name: &str,
    candidates: Option<&[String]>,
) -> (String, Option<String>) {
    // Basic normalization: trim, lowercase, replace spaces with '-', keep ascii alnum, '.', '_', '-', '/',
    // but preserve registry port patterns like ':5000' when immediately before '/' or end of component.
    let s = container_name.trim().to_lowercase();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut last_was_dash = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '/' {
            out.push(c);
            last_was_dash = false;
            i += 1;
            continue;
        }

        if c == ':' {
            // lookahead for digits to detect a port (e.g., :5000) followed by '/' or end
            let mut j = i + 1;
            let mut has_digit = false;
            while j < chars.len() && chars[j].is_ascii_digit() {
                has_digit = true;
                j += 1;
            }
            if has_digit && (j == chars.len() || chars[j] == '/') {
                // append the whole ":digits" substring in one go for clarity
                let slice: String = chars[i..j].iter().collect();
                out.push_str(&slice);
                last_was_dash = false;
                i = j;
                continue;
            }
            // otherwise treat colon as separator
            if !last_was_dash {
                out.push('-');
                last_was_dash = true;
            }
            i += 1;
            continue;
        }

        if c.is_whitespace() {
            if !last_was_dash {
                out.push('-');
                last_was_dash = true;
            }
            i += 1;
            continue;
        }

        // other punctuation -> separator
        if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
        i += 1;
    }

    // Collapse consecutive dashes into one
    let mut collapsed = String::with_capacity(out.len());
    let mut prev = '\0';
    for c in out.chars() {
        if c == '-' && prev == '-' {
            continue;
        }
        collapsed.push(c);
        prev = c;
    }

    // Collapse multiple slashes
    let mut norm = String::with_capacity(collapsed.len());
    let mut prev = '\0';
    for c in collapsed.chars() {
        if c == '/' && prev == '/' {
            continue;
        }
        norm.push(c);
        prev = c;
    }

    // Trim leading/trailing '-' '.' '/'
    let filter = norm
        .trim_matches(|c| c == '-' || c == '.' || c == '/')
        .to_string();
    if filter.is_empty() {
        return (filter, None);
    }

    // If candidates are provided, try to pick the best matching image tag
    if let Some(cands) = candidates {
        // collect matching candidates where repo ends with filter or equals it
        let matching: Vec<&String> = cands
            .iter()
            .filter(|img| {
                let s = img.as_str();
                let (repo, _tag, _digest) = split_repo_tag_digest(s);
                let repo_l = repo.to_ascii_lowercase();
                repo_l == filter || repo_l.ends_with(&format!("/{}", filter))
            })
            .collect();

        // Prefer latest, discard edge, otherwise prefer numeric version tags
        if !matching.is_empty() {
            // 1) try to find latest (case-insensitive)
            for img in &matching {
                let (_repo, tag_opt, _digest) = split_repo_tag_digest(img.as_str());
                if let Some(tag) = tag_opt {
                    if tag.eq_ignore_ascii_case("latest") {
                        return (filter, Some((*img).clone()));
                    }
                }
            }

            // 2) collect numeric-version tags
            let mut semvers: Vec<(&String, Vec<u64>)> = Vec::new();
            for img in &matching {
                let (_repo, tag_opt, _digest) = split_repo_tag_digest(img.as_str());
                if let Some(tag) = tag_opt {
                    let tag_l = tag.to_ascii_lowercase();
                    if tag_l == "edge" {
                        continue;
                    }
                    // require the whole tag to be numeric-like, then capture the numeric portion
                    if VERSION_RE.is_match(&tag_l) {
                        if let Some(cap) = VER_CAPTURE_RE.captures(&tag_l) {
                            let ver = &cap["ver"];
                            let nums: Vec<u64> = ver
                                .split(|c| c == '.' || c == '_' || c == '-')
                                .filter_map(|p| p.parse::<u64>().ok())
                                .collect();
                            if !nums.is_empty() {
                                semvers.push(((*img), nums));
                            }
                        }
                    }
                }
            }

            if !semvers.is_empty() {
                // sort by numeric vector descending using padded comparison
                semvers.sort_by(|a, b| compare_version_vec(&b.1, &a.1));
                return (filter, Some(semvers[0].0.clone()));
            }

            // 3) fallback: pick first non-edge matching
            for img in &matching {
                let (_repo, tag_opt, _digest) = split_repo_tag_digest(img.as_str());
                if let Some(tag) = tag_opt {
                    if tag.eq_ignore_ascii_case("edge") {
                        continue;
                    }
                }
                return (filter, Some((*img).clone()));
            }
        }
    }

    // If we had candidate list but no matches, return None to avoid suggesting non-existing image
    if candidates.is_some() {
        return (filter, None);
    }

    // Default heuristic: suggest <filter>:latest when no candidate info is available
    let suggested = format!("{}:latest", filter);
    (filter, Some(suggested))
}

#[cfg(test)]
mod tests {
    use super::{derive_image_prefill, split_repo_tag_digest};

    #[test]
    fn split_repo_tag_digest_examples() {
        assert_eq!(
            split_repo_tag_digest("repo:1.2.3"),
            ("repo", Some("1.2.3"), None)
        );
        assert_eq!(
            split_repo_tag_digest("repo@sha256:abcdef"),
            ("repo", None, Some("sha256:abcdef"))
        );
        assert_eq!(
            split_repo_tag_digest("host:5000/repo:1.0"),
            ("host:5000/repo", Some("1.0"), None)
        );
        assert_eq!(
            split_repo_tag_digest("host:5000/repo@sha256:abc"),
            ("host:5000/repo", None, Some("sha256:abc"))
        );
        assert_eq!(
            split_repo_tag_digest("repo:tag@sha256:abc"),
            ("repo", None, Some("sha256:abc"))
        );
    }

    #[test]
    fn basic_examples() {
        let (f, s) = derive_image_prefill("Ubuntu", None);
        assert_eq!(f, "ubuntu");
        assert_eq!(s.unwrap(), "ubuntu:latest");

        let (f, s) = derive_image_prefill("My_Box", None);
        assert_eq!(f, "my_box");
        assert_eq!(s.unwrap(), "my_box:latest");

        let (f, s) = derive_image_prefill(" Foo/Bar ", None);
        assert_eq!(f, "foo/bar");
        assert_eq!(s.unwrap(), "foo/bar:latest");

        let (f, s) = derive_image_prefill("", None);
        assert!(f.is_empty());
        assert!(s.is_none());
    }

    #[test]
    fn candidates_latest_preferred_and_case_insensitive() {
        let cands = vec![
            "example/repo:1.0".to_string(),
            "example/repo:LATEST".to_string(),
            "example/repo:edge".to_string(),
        ];
        let (f, s) = derive_image_prefill("example/repo", Some(&cands));
        assert_eq!(f, "example/repo");
        assert_eq!(s.unwrap(), "example/repo:LATEST");
    }

    #[test]
    fn candidates_numeric_sorting_and_edge_skipping() {
        let cands = vec![
            "host:5000/repo:1.2.3".to_string(),
            "host:5000/repo:2.0".to_string(),
            "host:5000/repo:edge".to_string(),
        ];
        let (f, s) = derive_image_prefill("host:5000/repo", Some(&cands));
        assert_eq!(f, "host:5000/repo");
        assert_eq!(s.unwrap(), "host:5000/repo:2.0");
    }

    #[test]
    fn candidates_none_matching_returns_none() {
        let cands = vec![
            "other/repo:1.0".to_string(),
            "another/repo:latest".to_string(),
        ];
        let (f, s) = derive_image_prefill("doesnotexist", Some(&cands));
        assert_eq!(f, "doesnotexist");
        assert!(s.is_none());
    }

    #[test]
    fn split_repo_tag_digest_more_cases() {
        // triple combo: repo:tag@sha256: -> digest should win, tag discarded
        assert_eq!(
            split_repo_tag_digest("repo:tag@sha256:abc"),
            ("repo", None, Some("sha256:abc"))
        );
        // host with port, tag and digest -> digest wins and repo includes host:port/repo
        assert_eq!(
            split_repo_tag_digest("example.com:5000/repo:1.0@sha256:abc"),
            ("example.com:5000/repo", None, Some("sha256:abc"))
        );
        // plain repo no tag/digest
        assert_eq!(
            split_repo_tag_digest("plainrepo"),
            ("plainrepo", None, None)
        );
    }

    #[test]
    fn normalization_preserves_registry_port_and_collapses_dashes() {
        let (f, s) = derive_image_prefill("Host:5000/Repo", None);
        assert_eq!(f, "host:5000/repo");
        assert_eq!(s.unwrap(), "host:5000/repo:latest");

        let (f, s) = derive_image_prefill("foo---bar", None);
        assert_eq!(f, "foo-bar");
        assert_eq!(s.unwrap(), "foo-bar:latest");
    }

    #[test]
    fn candidate_numeric_comparison_handles_subpaths() {
        let cands = vec![
            "registry/example/foo/bar:1.2".to_string(),
            "registry/example/foo/bar:1.10".to_string(),
            "registry/example/foo/bar:edge".to_string(),
        ];
        let (f, s) = derive_image_prefill("foo/bar", Some(&cands));
        assert_eq!(f, "foo/bar");
        assert_eq!(s.unwrap(), "registry/example/foo/bar:1.10");
    }

    #[test]
    fn ipv6_and_bracketed_host_handling() {
        assert_eq!(
            split_repo_tag_digest("[::1]:5000/repo:1.0"),
            ("[::1]:5000/repo", Some("1.0"), None)
        );
        assert_eq!(
            split_repo_tag_digest("[::1]:5000/repo@sha256:abc"),
            ("[::1]:5000/repo", None, Some("sha256:abc"))
        );
    }

    #[test]
    fn version_padding_prefers_longer_with_zero_padding() {
        let cands = vec!["repo:1.2".to_string(), "repo:1.2.0".to_string()];
        let (f, s) = derive_image_prefill("repo", Some(&cands));
        assert_eq!(f, "repo");
        // with padded numeric comparison, 1.2.0 should be treated as >= 1.2
        assert_eq!(s.unwrap(), "repo:1.2.0");
    }

    #[test]
    fn digest_only_candidate_is_accepted_in_fallback() {
        let cands = vec!["repo@sha256:deadbeef".to_string(), "repo:1.0".to_string()];
        // when matching by repo, digest-only candidate should be returned if it's the first non-edge fallback
        let (f, s) = derive_image_prefill("repo", Some(&cands));
        assert_eq!(f, "repo");
        assert_eq!(s.unwrap(), "repo@sha256:deadbeef");
    }
}
