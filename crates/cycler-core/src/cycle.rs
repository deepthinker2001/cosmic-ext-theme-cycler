//! Which theme comes next: the first existing theme after the last one
//! applied, in list order, wrapping round. A deleted theme is never chosen
//! but keeps its place, so the cycle carries on from where it stood.
//!
//! Pure logic: no files and no clock, so every rule is tested here.

/// The theme to apply after `last`.
///
/// `themes` holds every theme toggled on, in list order, by key, each with
/// whether it still exists. The answer is the first existing theme after
/// `last`'s place, wrapping round to the start, so with a single existing
/// theme it is that theme again. With no `last`, or a `last` that is no
/// longer in `themes` and so has no place, the cycle starts at the first
/// existing theme.
///
/// `None` when no theme in `themes` exists: nothing is toggled on, or every
/// theme toggled on has been deleted.
#[must_use]
pub fn next_after<'a, K: AsRef<str>>(
    themes: &'a [(K, bool)],
    last: Option<&str>,
) -> Option<&'a str> {
    let from = last.and_then(|last| themes.iter().position(|(key, _)| key.as_ref() == last));
    let start = from.map_or(0, |index| index + 1);
    // Every place once, starting just after `last` and ending on it.
    (0..themes.len())
        .map(|offset| &themes[(start + offset) % themes.len()])
        .find(|(_, exists)| *exists)
        .map(|(key, _)| key.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    const THEMES: [(&str, bool); 4] = [("a", true), ("b", true), ("c", true), ("d", true)];

    /// `steps` moves from `start`, feeding each answer back as the last
    /// applied theme, as the applet does.
    fn walk(themes: &[(&str, bool)], start: Option<&str>, steps: usize) -> Vec<String> {
        let mut last = start.map(str::to_owned);
        (0..steps)
            .map(|_| {
                let key = next_after(themes, last.as_deref()).unwrap().to_owned();
                last = Some(key.clone());
                key
            })
            .collect()
    }

    /// `THEMES` with the `deleted` ones marked as no longer existing.
    fn without(deleted: &[&str]) -> Vec<(&'static str, bool)> {
        THEMES
            .iter()
            .map(|&(key, _)| (key, !deleted.contains(&key)))
            .collect()
    }

    #[test]
    fn the_cycle_follows_the_list_and_wraps() {
        assert_eq!(walk(&THEMES, None, 5), ["a", "b", "c", "d", "a"]);
        assert_eq!(next_after(&THEMES, Some("d")), Some("a"));
    }

    #[test]
    fn a_last_theme_no_longer_listed_starts_from_the_first() {
        // Toggled off since, say: it has no place to carry on from.
        assert_eq!(next_after(&THEMES, Some("gone")), Some("a"));
    }

    #[test]
    fn a_deleted_last_theme_keeps_its_place() {
        // c was showing and has been deleted: the cycle carries on from
        // c's place rather than starting over.
        let themes = without(&["c"]);
        assert_eq!(next_after(&themes, Some("c")), Some("d"));
        // A deleted theme is never the answer, and is stepped over.
        assert_eq!(walk(&themes, Some("b"), 4), ["d", "a", "b", "d"]);
    }

    #[test]
    fn a_deleted_last_theme_at_the_end_wraps() {
        assert_eq!(next_after(&without(&["d"]), Some("d")), Some("a"));
        // Deleted together with the first: the wrap skips both.
        assert_eq!(next_after(&without(&["d", "a"]), Some("d")), Some("b"));
    }

    #[test]
    fn a_single_theme_is_always_the_answer() {
        let one = [("only", true)];
        assert_eq!(walk(&one, None, 3), ["only", "only", "only"]);
        // The only existing one among deleted themes, from its own place.
        assert_eq!(next_after(&without(&["a", "b", "d"]), Some("c")), Some("c"));
    }

    #[test]
    fn nothing_toggled_on_or_everything_deleted_gives_none() {
        let none: [(&str, bool); 0] = [];
        assert_eq!(next_after(&none, None), None);
        let gone = without(&["a", "b", "c", "d"]);
        assert_eq!(next_after(&gone, None), None);
        assert_eq!(next_after(&gone, Some("b")), None);
    }

    #[test]
    fn owned_keys_work_too() {
        let themes = vec![("system:a".to_owned(), true), ("user:b".to_owned(), true)];
        assert_eq!(next_after(&themes, Some("system:a")), Some("user:b"));
    }
}
