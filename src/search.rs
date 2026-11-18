use crate::config::{Matching, Search};
use crate::items::Item;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};

pub fn matches(items: &[Item], query: &str, config: &Search) -> Vec<usize> {
    let query = query.trim();
    if query.is_empty() {
        return (0..items.len()).collect();
    }

    let matcher = SkimMatcherV2::default();
    let (matcher, query) = if config.case_sensitive {
        (matcher.respect_case(), query.to_owned())
    } else {
        (matcher.ignore_case(), query.to_lowercase())
    };
    let score = |text: &str| -> Option<i64> {
        let folded;
        let text = if config.case_sensitive {
            text
        } else {
            folded = text.to_lowercase();
            &folded
        };
        let score = match config.matching {
            Matching::Fuzzy => matcher.fuzzy_match(text, &query)?,
            Matching::Substring => 30 - text.find(&query)? as i64,
            Matching::Prefix => text.starts_with(&query).then_some(40)?,
        };
        let bonus = if text == query {
            100
        } else if text.starts_with(&query) {
            30
        } else {
            0
        };
        Some(score + bonus)
    };

    let mut found: Vec<_> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let label = score(&item.label).map(|score| score + 60);
            let secondary = item
                .description
                .iter()
                .chain(&item.keywords)
                .filter_map(|text| score(text))
                .max();
            Some((index, label.into_iter().chain(secondary).max()?))
        })
        .collect();
    if config.sort {
        found.sort_by_key(|&(index, score)| (std::cmp::Reverse(score), index));
    }
    found.into_iter().map(|(index, _)| index).collect()
}
