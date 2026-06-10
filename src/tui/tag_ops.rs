//! Tag system operations — remapping tag ranges after operations.

use std::collections::HashSet;

use lograph::operator::Operation;

use crate::tui::app::App;

/// Remap tag ranges after a non-destructive operation.
/// `old_lines` is the pre-operation line set.
pub fn remap_tags_after_operation(app: &mut App, op: &Operation, old_lines: &[String]) {
    let tags = app.tag_store.get_tags(&app.repo_name).to_vec();
    if tags.is_empty() {
        return;
    }

    let new_tags = match op {
        Operation::Filter { pattern, keep } => {
            if let Ok(re) = regex::Regex::new(pattern) {
                let mut mapping: Vec<Option<usize>> = vec![None; old_lines.len()];
                let mut new_idx = 0usize;
                for (old_idx, line) in old_lines.iter().enumerate() {
                    let survives = if *keep {
                        re.is_match(line)
                    } else {
                        !re.is_match(line)
                    };
                    if survives {
                        mapping[old_idx] = Some(new_idx);
                        new_idx += 1;
                    }
                }
                remap_tag_ranges(&tags, &mapping)
            } else {
                tags.clone()
            }
        }
        Operation::DeleteLines { line_indices } => {
            let removed: HashSet<usize> = line_indices.iter().copied().collect();
            let mut mapping: Vec<Option<usize>> = vec![None; old_lines.len()];
            let mut new_idx = 0usize;
            for old_idx in 0..old_lines.len() {
                if !removed.contains(&old_idx) {
                    mapping[old_idx] = Some(new_idx);
                    new_idx += 1;
                }
            }
            remap_tag_ranges(&tags, &mapping)
        }
        Operation::InsertLines {
            after_line, content, ..
        } => {
            let offset = content.len();
            tags.iter()
                .map(|tag| {
                    let new_ranges = tag
                        .ranges
                        .iter()
                        .map(|&(s, e)| {
                            if s > *after_line {
                                (s + offset, e + offset)
                            } else if e > *after_line {
                                (s, e + offset)
                            } else {
                                (s, e)
                            }
                        })
                        .collect();
                    lograph::tag::Tag {
                        ranges: new_ranges,
                        ..tag.clone()
                    }
                })
                .collect()
        }
        Operation::Replace { .. } | Operation::ModifyLine { .. } => {
            tags.clone() // no line count change
        }
        _ => tags.clone(),
    };

    if !new_tags.is_empty() {
        app.tag_store
            .repos
            .insert(app.repo_name.clone(), new_tags);
    }
    let _ = app.tag_store.save(&app.workspace.root());
}

/// Given an old→new line mapping (None = removed), compute new tag ranges.
/// Consecutive surviving positions are coalesced into ranges.
pub fn remap_tag_ranges(
    tags: &[lograph::tag::Tag],
    mapping: &[Option<usize>],
) -> Vec<lograph::tag::Tag> {
    tags.iter()
        .map(|tag| {
            let mut new_ranges = Vec::new();
            for &(s, e) in &tag.ranges {
                let mut range_start: Option<usize> = None;
                let end_idx = e.min(mapping.len().saturating_sub(1));
                for old_idx in s..=end_idx {
                    if let Some(new_pos) = mapping[old_idx] {
                        if range_start.is_none() {
                            range_start = Some(new_pos);
                        }
                    } else if let Some(start) = range_start.take() {
                        if old_idx > 0 {
                            if let Some(last_pos) = mapping[old_idx - 1] {
                                new_ranges.push((start, last_pos));
                            }
                        }
                    }
                }
                if let Some(start) = range_start {
                    if s <= mapping.len().saturating_sub(1) {
                        if let Some(last_pos) = mapping[end_idx] {
                            new_ranges.push((start, last_pos));
                        }
                    }
                }
            }
            new_ranges.sort_by_key(|&(s, _)| s);
            let mut merged: Vec<(usize, usize)> = Vec::new();
            for (s, e) in new_ranges {
                if let Some((_, ref mut last_e)) = merged.last_mut() {
                    if s <= *last_e + 1 {
                        *last_e = (*last_e).max(e);
                        continue;
                    }
                }
                merged.push((s, e));
            }
            lograph::tag::Tag {
                ranges: merged,
                ..tag.clone()
            }
        })
        .collect()
}
