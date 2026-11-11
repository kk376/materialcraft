//! Subsequence and fuzzy matching for node search and library panels.

/// Case-insensitive fuzzy subsequence score (higher is better). Returns `None` if not a match.
///
/// Matching rules:
/// 1. An empty query matches everything with score 0.
/// 2. Every non-whitespace character in `query` must appear in `hay` in sequential order.
/// 3. Scoring rewards:
///    - Exact match bonus (+100)
///    - Exact prefix match bonus (+50)
///    - Word prefix match bonus (+40)
///    - Exact substring containment bonus (+30)
///    - Contiguous character matches (+8 per character)
///    - Word-boundary matches: start of string or right after `' '`, `'_'`, `'-'`, `'/'` (+6 per character)
/// 4. Length penalty: subtracts `(hay.len() / 16)` so shorter, tighter matches rank ahead of sprawling names.
pub fn fuzzy_score(hay: &str, query: &str) -> Option<i32> {
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        return Some(0);
    }

    let hay_lower = case_fold(hay);
    let q_lower = case_fold(query_trimmed);

    let words: Vec<&str> = q_lower.split_whitespace().collect();
    if words.len() > 1 {
        let mut total = 0i32;
        for w in words {
            total = total.saturating_add(single_word_score(&hay_lower, w)?);
        }
        return Some(total);
    }

    single_word_score(&hay_lower, &q_lower)
}

/// Scores a single whitespace-trimmed query word against lowercase candidate text.
///
/// Returns `None` if the query word is not a subsequence of `hay_lower`.
fn single_word_score(hay_lower: &str, q_lower: &str) -> Option<i32> {
    let mut score = 0i32;

    if hay_lower == q_lower {
        score = score.saturating_add(100);
    } else if hay_lower.starts_with(q_lower) {
        score = score.saturating_add(50);
    } else if hay_lower.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q_lower)) {
        score = score.saturating_add(40);
    } else if hay_lower.contains(q_lower) {
        score = score.saturating_add(30);
    }

    let mut last_end_idx: Option<usize> = None;
    let mut chars = hay_lower.char_indices();
    for q in q_lower.chars().filter(|c| !c.is_whitespace()) {
        let (idx, ch) = chars.by_ref().find(|(_, c)| *c == q)?;
        let contiguous = match last_end_idx {
            Some(prev_end) if idx == prev_end => 8,
            _ => 1,
        };
        score = score.saturating_add(contiguous);

        let is_boundary = idx == 0 || matches!(hay_lower.as_bytes().get(idx.saturating_sub(1)), Some(&b' ' | &b'_' | &b'-' | &b'/'));
        if is_boundary {
            score = score.saturating_add(6);
        }
        last_end_idx = Some(idx.saturating_add(ch.len_utf8()));
    }

    let len_penalty = (hay_lower.len() / 16) as i32;
    Some(score.saturating_sub(len_penalty))
}

/// Returns true if `c` is a Unicode combining diacritic mark (U+0300..=U+036F).
fn is_combining_diacritic(c: char) -> bool {
    matches!(c, '\u{0300}'..='\u{036F}')
}

/// Canonical combining class for diacritics: below marks (202) sort before above marks (230).
fn combining_class(c: char) -> u8 {
    match c {
        '\u{0327}' | '\u{0328}' => 202, // Cedilla, ogonek (below)
        _ if is_combining_diacritic(c) => 230,
        _ => 0,
    }
}

/// Normalizes and case-folds text for Unicode-aware, canonically equivalent fuzzy matching.
///
/// 1. Expands ligatures and applies Unicode lowercase.
/// 2. Canonically sorts consecutive combining marks by combining class.
/// 3. Precomposes base Latin letters with combining accents into NFC equivalent forms.
fn case_fold(s: &str) -> String {
    // Step 1: Expand ligatures and special lowercasing into characters
    let mut expanded = Vec::with_capacity(s.len());
    for c in s.chars() {
        for lc in c.to_lowercase() {
            match lc {
                'ß' => expanded.extend(['s', 's']),
                'ς' => expanded.push('σ'),
                'µ' => expanded.push('μ'),
                'ſ' => expanded.push('s'),
                'ﬀ' => expanded.extend(['f', 'f']),
                'ﬁ' => expanded.extend(['f', 'i']),
                'ﬂ' => expanded.extend(['f', 'l']),
                'ﬃ' => expanded.extend(['f', 'f', 'i']),
                'ﬄ' => expanded.extend(['f', 'f', 'l']),
                'ﬅ' | 'ﬆ' => expanded.extend(['s', 't']),
                other => expanded.push(other),
            }
        }
    }

    // Step 2: Canonical combining order & diacritic composition
    let mut out = String::with_capacity(expanded.len());
    let mut i = 0;
    while i < expanded.len() {
        let ch = expanded[i];
        if !is_combining_diacritic(ch) {
            let mut j = i + 1;
            let mut marks = Vec::new();
            while j < expanded.len() && is_combining_diacritic(expanded[j]) {
                marks.push(expanded[j]);
                j += 1;
            }
            if !marks.is_empty() {
                marks.sort_by_key(|&m| combining_class(m));
                let mut curr = ch;
                for m in marks {
                    if let Some(composed) = compose_diacritic(curr, m) {
                        curr = composed;
                    } else {
                        out.push(curr);
                        curr = m;
                    }
                }
                out.push(curr);
                i = j;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }

    out
}

/// Composes a base letter and combining diacritic into its canonical precomposed NFC form.
fn compose_diacritic(base: char, diacritic: char) -> Option<char> {
    match (base.to_lowercase().next()?, diacritic) {
        ('e', '\u{0301}') => Some('é'),
        ('e', '\u{0300}') => Some('è'),
        ('e', '\u{0302}') => Some('ê'),
        ('e', '\u{0308}') => Some('ë'),
        ('e', '\u{030c}') => Some('ě'),
        ('a', '\u{0300}') => Some('à'),
        ('a', '\u{0301}') => Some('á'),
        ('a', '\u{0302}') => Some('â'),
        ('a', '\u{0303}') => Some('ã'),
        ('a', '\u{0308}') => Some('ä'),
        ('a', '\u{030a}') => Some('å'),
        ('a', '\u{0304}') => Some('ā'),
        ('a', '\u{0306}') => Some('ă'),
        ('a', '\u{0328}') => Some('ą'),
        ('o', '\u{0300}') => Some('ò'),
        ('o', '\u{0301}') => Some('ó'),
        ('o', '\u{0302}') => Some('ô'),
        ('o', '\u{0303}') => Some('õ'),
        ('o', '\u{0308}') => Some('ö'),
        ('o', '\u{0304}') => Some('ō'),
        ('o', '\u{030b}') => Some('ő'),
        ('u', '\u{0300}') => Some('ù'),
        ('u', '\u{0301}') => Some('ú'),
        ('u', '\u{0302}') => Some('û'),
        ('u', '\u{0308}') => Some('ü'),
        ('u', '\u{030a}') => Some('ů'),
        ('u', '\u{0304}') => Some('ū'),
        ('u', '\u{030b}') => Some('ű'),
        ('i', '\u{0300}') => Some('ì'),
        ('i', '\u{0301}') => Some('í'),
        ('i', '\u{0302}') => Some('î'),
        ('i', '\u{0308}') => Some('ï'),
        ('i', '\u{0304}') => Some('ī'),
        ('c', '\u{0327}') => Some('ç'),
        ('c', '\u{030c}') => Some('č'),
        ('c', '\u{0301}') => Some('ć'),
        ('j', '\u{030c}') => Some('ǰ'),
        ('j', '\u{0302}') => Some('ĵ'),
        ('g', '\u{0307}') => Some('ġ'),
        ('g', '\u{0306}') => Some('ğ'),
        ('g', '\u{0327}') => Some('ģ'),
        ('g', '\u{0302}') => Some('ĝ'),
        ('h', '\u{0302}') => Some('ĥ'),
        ('h', '\u{0307}') => Some('ḣ'),
        ('k', '\u{0327}') => Some('ķ'),
        ('l', '\u{0301}') => Some('ĺ'),
        ('l', '\u{0327}') => Some('ļ'),
        ('l', '\u{030c}') => Some('ľ'),
        ('l', '\u{0307}') => Some('ŀ'),
        ('s', '\u{030c}') => Some('š'),
        ('s', '\u{0301}') => Some('ś'),
        ('s', '\u{0327}') => Some('ş'),
        ('z', '\u{030c}') => Some('ž'),
        ('z', '\u{0301}') => Some('ź'),
        ('z', '\u{0307}') => Some('ż'),
        ('r', '\u{030c}') => Some('ř'),
        ('d', '\u{030c}') => Some('ď'),
        ('t', '\u{030c}') => Some('ť'),
        ('t', '\u{0327}') => Some('ţ'),
        ('n', '\u{030c}') => Some('ň'),
        ('n', '\u{0303}') => Some('ñ'),
        ('n', '\u{0301}') => Some('ń'),
        ('w', '\u{0302}') => Some('ŵ'),
        ('y', '\u{0301}') => Some('ý'),
        ('y', '\u{0302}') => Some('ŷ'),
        ('y', '\u{0308}') => Some('ÿ'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn test_empty_query_matches_with_zero() {
        assert_eq!(fuzzy_score("Gaussian Blur", ""), Some(0));
        assert_eq!(fuzzy_score("Gaussian Blur", "   "), Some(0));
    }

    #[test]
    fn test_exact_match_score() {
        let score = fuzzy_score("blur", "blur").unwrap();
        assert!(score >= 100);
    }

    #[test]
    fn test_case_insensitive_matching() {
        assert!(fuzzy_score("Gaussian Blur", "gblur").is_some());
        assert!(fuzzy_score("Gaussian Blur", "GBLUR").is_some());
    }

    #[test]
    fn test_subsequence_non_match() {
        assert_eq!(fuzzy_score("Gaussian Blur", "xyz"), None);
        assert_eq!(fuzzy_score("Blur", "blura"), None);
    }

    #[test]
    fn test_shorter_matches_rank_higher() {
        let short_score = fuzzy_score("Blur", "blur").unwrap();
        let long_score = fuzzy_score("Directional Blur Advanced", "blur").unwrap();
        assert!(short_score > long_score);
    }

    #[test]
    fn test_multi_word_query() {
        assert!(fuzzy_score("Directional Blur", "dir blur").is_some());
        assert_eq!(fuzzy_score("Directional Blur", "dir xyz"), None);
    }

    #[test]
    fn test_combining_diacritics_normalization() {
        assert_eq!(fuzzy_score("Crème Brûlée", "crème brûlée"), fuzzy_score("crème brûlée", "crème brûlée"));
        assert!(fuzzy_score("nai\u{0308}ve", "naïve").is_some());
        assert!(fuzzy_score("naïve", "nai\u{0308}ve").is_some());
        assert!(fuzzy_score("e\u{0301}clat", "é").is_some());
        assert!(fuzzy_score("éclat", "e\u{0301}").is_some());
        assert!(fuzzy_score("Å", "A\u{030a}").is_some());
        assert!(fuzzy_score("č", "c\u{030c}").is_some());
        assert_eq!(fuzzy_score("plain", "p\u{0315}lain"), None);
    }

    #[test]
    fn test_reordered_combining_marks_match() {
        // c + acute + cedilla vs c + cedilla + acute
        let s1 = "c\u{0301}\u{0327}at";
        let s2 = "c\u{0327}\u{0301}at";
        assert_eq!(fuzzy_score(s1, s2), fuzzy_score(s1, s1));
        assert!(fuzzy_score(s1, s2).is_some());
    }

    #[test]
    fn test_ligature_expansion_with_accents() {
        // ffi ligature + acute vs f-f-i + acute
        let lig = "ﬃ\u{0301}le";
        let sep = "ffi\u{0301}le";
        assert_eq!(fuzzy_score(lig, sep), fuzzy_score(sep, sep));
    }

    #[test]
    fn test_caron_on_j_and_dot_on_g() {
        assert_eq!(fuzzy_score("ǰ", "j\u{030c}"), fuzzy_score("ǰ", "ǰ"));
        assert_eq!(fuzzy_score("ġ", "g\u{0307}"), fuzzy_score("ġ", "ġ"));
    }

    #[test]
    fn test_turkish_dotted_i_with_macron() {
        let label = "İ\u{0304}tem";
        let query = "İ\u{0304}tem";
        assert_eq!(fuzzy_score(label, query), fuzzy_score(label, label));
    }

    #[test]
    fn test_multibyte_character_contiguity_tracking() {
        let score = fuzzy_score("🦀🦀🦀", "🦀🦀").unwrap();
        assert!(score > 0);
    }
}
