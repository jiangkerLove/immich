//! Picomatch-style path matching for library exclusions.
//!
//! Watch uses `nocase` (+ Windows separators). Scan CHECK_OFFLINE uses case-sensitive
//! matching and leaves `\` as a normal character, matching `picomatch.isMatch`.

/// Convert a picomatch-style glob to a SQL LIKE pattern (legacy helper; prefer
/// [`path_matches_exclusion`] for scan/watch exclusion checks).
pub fn glob_to_sql_like(glob: &str) -> String {
    let mut result = String::new();
    let mut chars = glob.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                }
                result.push('%');
            }
            '?' => result.push('_'),
            '%' | '_' | '\\' => {
                result.push('\\');
                result.push(ch);
            }
            other => result.push(other),
        }
    }

    result
}

#[derive(Clone, Copy, Debug)]
pub struct GlobMatchOptions {
    /// Watcher `ignore` and fast-glob crawl: true. `picomatch.isMatch` / scan: false.
    pub nocase: bool,
    /// Watcher only, and only on Windows. Scan leaves `\` as a normal character.
    pub windows: bool,
}

impl GlobMatchOptions {
    pub const SCAN: Self = Self {
        nocase: false,
        windows: false,
    };

    pub const WATCH: Self = Self {
        nocase: true,
        windows: cfg!(windows),
    };

    pub const CRAWL: Self = Self {
        nocase: true,
        windows: cfg!(windows),
    };
}

pub fn path_matches_exclusion(path: &str, patterns: &[String]) -> bool {
    path_matches_exclusion_with(path, patterns, GlobMatchOptions::SCAN)
}

pub fn path_matches_exclusion_with(
    path: &str,
    patterns: &[String],
    options: GlobMatchOptions,
) -> bool {
    patterns
        .iter()
        .any(|pattern| glob_match(pattern, path, options))
}

#[derive(Clone)]
enum Token {
    Star,
    Qmark,
    Lit(char),
}

fn glob_match(pattern: &str, path: &str, options: GlobMatchOptions) -> bool {
    if path.is_empty() {
        return false;
    }

    let mut pattern = match pattern {
        "***" | "**/**" | "**/**/**" => "**".to_string(),
        other => other.to_string(),
    };
    let mut path = path.to_string();
    if options.windows {
        pattern = pattern.replace('\\', "/");
        path = path.replace('\\', "/");
    }

    let parts: Vec<Option<Vec<Token>>> = pattern
        .split('/')
        .map(|segment| {
            if segment == "**" {
                None
            } else {
                Some(parse_segment(segment))
            }
        })
        .collect();
    let segments: Vec<String> = path.split('/').map(str::to_string).collect();
    match_parts(&parts, &segments, 0, 0, options.nocase)
}

fn parse_segment(segment: &str) -> Vec<Token> {
    let chars: Vec<char> = segment.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' => {
                // a**b collapses to one star, matching picomatch.
                if chars.get(index + 1) == Some(&'*') {
                    index += 1;
                }
                tokens.push(Token::Star);
            }
            '?' => tokens.push(Token::Qmark),
            '\\' => {
                if let Some(next) = chars.get(index + 1) {
                    tokens.push(Token::Lit(*next));
                    index += 1;
                }
            }
            ch => tokens.push(Token::Lit(ch)),
        }
        index += 1;
    }
    tokens
}

fn match_parts(
    parts: &[Option<Vec<Token>>],
    segments: &[String],
    part_index: usize,
    segment_index: usize,
    nocase: bool,
) -> bool {
    if part_index == parts.len() {
        return segment_index == segments.len();
    }
    match &parts[part_index] {
        None => {
            if match_parts(parts, segments, part_index + 1, segment_index, nocase) {
                return true;
            }
            segment_index < segments.len()
                && !segments[segment_index].starts_with('.')
                && match_parts(parts, segments, part_index, segment_index + 1, nocase)
        }
        Some(tokens) => {
            segment_index < segments.len()
                && match_segment(tokens, &segments[segment_index], nocase)
                && match_parts(parts, segments, part_index + 1, segment_index + 1, nocase)
        }
    }
}

fn match_segment(tokens: &[Token], text: &str, nocase: bool) -> bool {
    fn rec(
        tokens: &[Token],
        text: &[char],
        token_index: usize,
        char_index: usize,
        nocase: bool,
    ) -> bool {
        if token_index == tokens.len() {
            return char_index == text.len();
        }
        match tokens[token_index] {
            Token::Lit(expected) => {
                char_index < text.len()
                    && chars_eq(text[char_index], expected, nocase)
                    && rec(tokens, text, token_index + 1, char_index + 1, nocase)
            }
            Token::Qmark => {
                if char_index >= text.len() || text[char_index] == '/' {
                    return false;
                }
                if token_index == 0 && text[char_index] == '.' {
                    return false;
                }
                rec(tokens, text, token_index + 1, char_index + 1, nocase)
            }
            Token::Star => {
                if token_index == 0 && (text.is_empty() || text[0] == '.') {
                    return false;
                }
                (char_index..=text.len())
                    .any(|next| rec(tokens, text, token_index + 1, next, nocase))
            }
        }
    }
    let chars: Vec<char> = text.chars().collect();
    rec(tokens, &chars, 0, 0, nocase)
}

fn chars_eq(left: char, right: char, nocase: bool) -> bool {
    if nocase {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobMatchOptions, path_matches_exclusion_with};

    #[test]
    fn star_does_not_cross_slash() {
        let patterns = vec!["**/abc/*.tif".to_string()];
        assert!(!path_matches_exclusion_with(
            "/foo/abc/sub/scan.tif",
            &patterns,
            GlobMatchOptions::SCAN
        ));
        assert!(path_matches_exclusion_with(
            "/foo/abc/scan.tif",
            &patterns,
            GlobMatchOptions::SCAN
        ));
    }

    #[test]
    fn double_star_can_be_empty() {
        let patterns = vec!["**/*.tif".to_string()];
        assert!(path_matches_exclusion_with(
            "bar.tif",
            &patterns,
            GlobMatchOptions::SCAN
        ));
        assert!(path_matches_exclusion_with(
            "/foo/bar.tif",
            &patterns,
            GlobMatchOptions::SCAN
        ));
    }

    #[test]
    fn watch_is_case_insensitive() {
        let patterns = vec!["**/Raw/**".to_string()];
        assert!(!path_matches_exclusion_with(
            "/foo/raw/bar.jpg",
            &patterns,
            GlobMatchOptions::SCAN
        ));
        assert!(path_matches_exclusion_with(
            "/foo/raw/bar.jpg",
            &patterns,
            GlobMatchOptions {
                nocase: true,
                windows: false
            }
        ));
    }

    #[test]
    fn double_star_skips_dot_segments() {
        let patterns = vec!["**/*.tif".to_string()];
        assert!(!path_matches_exclusion_with(
            "/foo/.hidden/bar.tif",
            &patterns,
            GlobMatchOptions::SCAN
        ));
    }

    #[test]
    fn ea_dir_and_apple_double() {
        let patterns = vec!["**/@eaDir/**".to_string(), "**/._*".to_string()];
        assert!(path_matches_exclusion_with(
            "/photos/@eaDir/thumb.jpg",
            &patterns,
            GlobMatchOptions::WATCH
        ));
        assert!(path_matches_exclusion_with(
            "/photos/._resource",
            &patterns,
            GlobMatchOptions::SCAN
        ));
    }
}
