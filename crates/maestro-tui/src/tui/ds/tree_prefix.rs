//! `ds::tree_prefix` (#1256): the `├─ └─ │` prefix of a tree row
//! (`treePrefix` in `docs/design-system/index.html`, §04 Kit v2), with an
//! ASCII fallback (`|- `, `` `- ``, `|  `) for terminals without box glyphs.

/// The prefix for one tree row.
///
/// `last_path[i]` says whether the row's ancestor at level `i` is the last
/// child of its parent; the final entry is the row itself. An empty path is
/// a root row and gets no prefix. Each level is three columns wide: a last
/// ancestor leaves blank space under it, any other draws the `│` rail.
pub fn tree_prefix(last_path: &[bool], ascii: bool) -> String {
    let (rail, tee, elbow) = if ascii {
        ("|  ", "|- ", "`- ")
    } else {
        ("│  ", "├─ ", "└─ ")
    };
    let Some((&last, ancestors)) = last_path.split_last() else {
        return String::new();
    };
    let mut out: String = ancestors
        .iter()
        .map(|&done| if done { "   " } else { rail })
        .collect();
    out.push_str(if last { elbow } else { tee });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_prefix_levels() {
        // Root rows have no prefix.
        assert_eq!(tree_prefix(&[], false), "");
        assert_eq!(tree_prefix(&[], true), "");

        // Unicode: one, two and three levels, middle and last children.
        assert_eq!(tree_prefix(&[false], false), "├─ ");
        assert_eq!(tree_prefix(&[true], false), "└─ ");
        assert_eq!(tree_prefix(&[false, false], false), "│  ├─ ");
        assert_eq!(tree_prefix(&[false, true], false), "│  └─ ");
        assert_eq!(tree_prefix(&[true, false], false), "   ├─ ");
        assert_eq!(tree_prefix(&[true, true], false), "   └─ ");
        assert_eq!(tree_prefix(&[false, true, false], false), "│     ├─ ");

        // ASCII fallback: same shape, same width.
        assert_eq!(tree_prefix(&[false], true), "|- ");
        assert_eq!(tree_prefix(&[true], true), "`- ");
        assert_eq!(tree_prefix(&[false, true], true), "|  `- ");
        assert_eq!(tree_prefix(&[true, false], true), "   |- ");
        assert_eq!(tree_prefix(&[false, true, false], true), "|     |- ");

        // Every level is three columns in both modes, so text lines up.
        for path in [&[false][..], &[true, false], &[false, true, true]] {
            for ascii in [false, true] {
                assert_eq!(tree_prefix(path, ascii).chars().count(), 3 * path.len());
            }
        }
    }
}
