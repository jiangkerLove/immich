pub fn tokenize_for_search(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        let start = i;
        if is_cjk(chars[i]) {
            while i < chars.len() && is_cjk(chars[i]) {
                i += 1;
            }
            if i - start == 1 {
                tokens.push(chars[start].to_string());
            } else {
                for k in start..i - 1 {
                    tokens.push(format!("{}{}", chars[k], chars[k + 1]));
                }
            }
        } else {
            while i < chars.len() && !chars[i].is_whitespace() && !is_cjk(chars[i]) {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        }
    }

    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4e00}'..='\u{9fff}'
            | '\u{ac00}'..='\u{d7af}'
            | '\u{3040}'..='\u{309f}'
            | '\u{30a0}'..='\u{30ff}'
            | '\u{3400}'..='\u{4dbf}'
    )
}
