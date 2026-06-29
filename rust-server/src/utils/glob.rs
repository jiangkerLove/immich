/// Convert a picomatch-style glob to a SQL LIKE pattern (mirrors server globToSqlPattern).
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

pub fn path_matches_exclusion(path: &str, patterns: &[String]) -> bool {
    let normalized = path.replace('\\', "/");
    patterns.iter().any(|pattern| {
        let like = glob_to_sql_like(pattern);
        sql_like_match(&normalized, &like)
    })
}

fn sql_like_match(text: &str, pattern: &str) -> bool {
    let text_chars: Vec<char> = text.chars().collect();
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let mut cache = vec![vec![None; pattern_chars.len() + 1]; text_chars.len() + 1];
    sql_like_match_inner(&text_chars, &pattern_chars, 0, 0, &mut cache)
}

fn sql_like_match_inner(
    text: &[char],
    pattern: &[char],
    ti: usize,
    pi: usize,
    cache: &mut [Vec<Option<bool>>],
) -> bool {
    if let Some(value) = cache[ti][pi] {
        return value;
    }

    let result = if pi == pattern.len() {
        ti == text.len()
    } else if pattern[pi] == '%' {
        (ti..=text.len()).any(|next| sql_like_match_inner(text, pattern, next, pi + 1, cache))
    } else if ti < text.len() && (pattern[pi] == '_' || pattern[pi] == text[ti]) {
        sql_like_match_inner(text, pattern, ti + 1, pi + 1, cache)
    } else {
        false
    };

    cache[ti][pi] = Some(result);
    result
}
