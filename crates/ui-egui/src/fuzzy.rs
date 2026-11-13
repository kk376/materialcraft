//! Subsequence and fuzzy matching for node search and library panels.

use unicode_normalization::UnicodeNormalization;

/// Case-insensitive, Unicode-normalized fuzzy subsequence score (higher is better).
/// Returns `None` if not a match.
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
///
/// Text is normalized to Unicode NFC after case-folding using the standards-compliant
/// `unicode-normalization` crate (Unicode Standard Annex #15).
pub fn fuzzy_score(hay: &str, query: &str) -> Option<i32> {
    let query_trimmed = query.trim();
    if query_trimmed.is_empty() {
        return Some(0);
    }

    let hay_norm = normalize_for_search(hay);
    let q_norm = normalize_for_search(query_trimmed);

    let mut words = q_norm.split_whitespace();
    let first = words.next()?;
    if let Some(second) = words.next() {
        let mut total = single_word_score(&hay_norm, first)?.saturating_add(single_word_score(&hay_norm, second)?);
        for w in words {
            total = total.saturating_add(single_word_score(&hay_norm, w)?);
        }
        Some(total)
    } else {
        single_word_score(&hay_norm, &q_norm)
    }
}

/// Normalizes and case-folds text for Unicode-aware, canonically equivalent fuzzy matching.
///
/// Applies Unicode lowercasing followed by canonical composition into NFC form.
fn normalize_for_search(s: &str) -> String {
    s.chars().flat_map(char::to_lowercase).nfc().collect()
}

/// Scores a single whitespace-trimmed query word against normalized candidate text.
///
/// Returns `None` if the query word is not a subsequence of `hay_norm`.
fn single_word_score(hay_norm: &str, q_norm: &str) -> Option<i32> {
    let mut score = 0i32;

    if hay_norm == q_norm {
        score = score.saturating_add(100);
    } else if hay_norm.starts_with(q_norm) {
        score = score.saturating_add(50);
    } else if hay_norm.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q_norm)) {
        score = score.saturating_add(40);
    } else if hay_norm.contains(q_norm) {
        score = score.saturating_add(30);
    }

    let mut last_end_idx: Option<usize> = None;
    let mut chars = hay_norm.char_indices();
    for q in q_norm.chars().filter(|c| !c.is_whitespace()) {
        let (idx, ch) = chars.by_ref().find(|(_, c)| *c == q)?;
        let contiguous = match last_end_idx {
            Some(prev_end) if idx == prev_end => 8,
            _ => 1,
        };
        score = score.saturating_add(contiguous);

        let is_boundary = idx == 0 || matches!(hay_norm.as_bytes().get(idx.saturating_sub(1)), Some(&b' ' | &b'_' | &b'-' | &b'/'));
        if is_boundary {
            score = score.saturating_add(6);
        }
        last_end_idx = Some(idx.saturating_add(ch.len_utf8()));
    }

    let len_penalty = (hay_norm.len() / 16) as i32;
    Some(score.saturating_sub(len_penalty))
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
    fn test_multi_accent_canonical_equivalence() {
        // Vietnamese ắ: precomposed (U+1EAF) vs decomposed (a + U+0306 breve + U+0301 acute)
        let precomposed = "m\u{1eaf}t";
        let decomposed = "ma\u{0306}\u{0301}t";
        assert_eq!(fuzzy_score(precomposed, decomposed), fuzzy_score(precomposed, precomposed));
        assert_eq!(fuzzy_score(decomposed, precomposed), fuzzy_score(precomposed, precomposed));
        assert!(fuzzy_score(precomposed, decomposed).is_some());
    }

    #[test]
    fn test_l_dot_above_distinct_from_middle_dot() {
        // l + U+0307 (combining dot above) must NOT be conflated with U+0140 (ŀ, L with middle dot)
        let l_combining_dot = "l\u{0307}abel";
        let l_middle_dot = "\u{0140}abel";
        assert_eq!(fuzzy_score(l_combining_dot, l_middle_dot), None);
    }

    #[test]
    fn test_multibyte_character_contiguity_tracking() {
        let score = fuzzy_score("🦀🦀🦀", "🦀🦀").unwrap();
        assert!(score > 0);
    }
}
