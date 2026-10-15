//! The recent-projects list (docs/specs/gui-plan-2026-10-15.md kill
//! `g5-menu-completion`, its File ▸ Recent part; P:A1, B:A3.2).
//!
//! [`RecentProjects`] is pure: most recent first, deduplicated by path,
//! capped at [`MAX_RECENT`]. It persists as one path per line in a
//! `recent` file next to the locale preference ([`recent_path`]), the
//! per-user Umber dir. A missing or unreadable file is an empty list, and
//! blank lines are skipped, so a hand-edited file can never stop the app
//! from starting. Paths that no longer exist stay in the list but draw
//! disabled in the menu ([`RecentProjects::existing`]), so a project on an
//! unplugged drive comes back when the drive does.

use std::path::{Path, PathBuf};

/// The list cap.
pub const MAX_RECENT: usize = 10;

/// The recent-projects list (see the module docs).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentProjects {
    paths: Vec<PathBuf>,
}

impl RecentProjects {
    /// The paths, most recent first.
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Records an open or save of `path`: moved to the front, the oldest
    /// entry dropped past [`MAX_RECENT`]. Returns whether the list changed.
    pub fn note(&mut self, path: &Path) -> bool {
        if self.paths.first().map(PathBuf::as_path) == Some(path) {
            return false;
        }
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.to_path_buf());
        self.paths.truncate(MAX_RECENT);
        true
    }

    /// Drops `path` (a project that failed to open). Returns whether it
    /// was listed.
    pub fn remove(&mut self, path: &Path) -> bool {
        let before = self.paths.len();
        self.paths.retain(|p| p != path);
        self.paths.len() != before
    }

    /// Empties the list (the menu's Clear).
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Each path with whether it still exists, per `exists`.
    pub fn existing(&self, exists: impl Fn(&Path) -> bool) -> Vec<(&Path, bool)> {
        self.paths
            .iter()
            .map(|p| (p.as_path(), exists(p)))
            .collect()
    }

    /// The file form: one path per line.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for p in &self.paths {
            out.push_str(&p.to_string_lossy());
            out.push('\n');
        }
        out
    }

    /// Parses [`Self::to_text`]'s form: blank lines skipped, duplicates
    /// dropped (first wins), capped at [`MAX_RECENT`].
    pub fn from_text(text: &str) -> Self {
        let mut list = RecentProjects::default();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let path = PathBuf::from(line);
            if !list.paths.contains(&path) && list.paths.len() < MAX_RECENT {
                list.paths.push(path);
            }
        }
        list
    }

    /// Loads from `path`; missing or unreadable reads as empty.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|t| Self::from_text(&t))
            .unwrap_or_default()
    }

    /// Writes to `path` (creating the parent dir).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_text())
    }
}

/// The recent file: a sibling of the locale preference, in the per-user
/// Umber dir. `None` when no home is discoverable.
pub fn recent_path() -> Option<PathBuf> {
    crate::i18n::locale_pref_path().map(|p| p.with_file_name("recent"))
}

/// The menu label for a project dir: its last component, or the whole
/// path when it has none (a root).
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn note_moves_to_front_dedups_and_caps() {
        let mut r = RecentProjects::default();
        assert!(r.note(&p("/a")));
        assert!(r.note(&p("/b")));
        assert_eq!(r.paths(), [p("/b"), p("/a")]);
        // Re-noting an older entry moves it up; the front is a no-op.
        assert!(r.note(&p("/a")));
        assert_eq!(r.paths(), [p("/a"), p("/b")]);
        assert!(!r.note(&p("/a")));
        for i in 0..20 {
            r.note(&p(&format!("/p{i}")));
        }
        assert_eq!(r.paths().len(), MAX_RECENT);
        assert_eq!(r.paths()[0], p("/p19"));
        assert!(!r.paths().contains(&p("/a")), "the oldest fell off");
    }

    #[test]
    fn remove_and_clear() {
        let mut r = RecentProjects::default();
        r.note(&p("/a"));
        r.note(&p("/b"));
        assert!(r.remove(&p("/a")));
        assert!(!r.remove(&p("/a")));
        assert_eq!(r.paths(), [p("/b")]);
        r.clear();
        assert!(r.paths().is_empty());
    }

    #[test]
    fn text_round_trips_and_tolerates_hand_edits() {
        let mut r = RecentProjects::default();
        r.note(&p("/one"));
        r.note(&p("/two words/dir"));
        assert_eq!(RecentProjects::from_text(&r.to_text()), r);
        // Blank lines, CRLF, padding and duplicates are survivable.
        let messy = "\r\n  /x  \r\n\r\n/y\n/x\n";
        assert_eq!(RecentProjects::from_text(messy).paths(), [p("/x"), p("/y")]);
        let long: String = (0..30).map(|i| format!("/d{i}\n")).collect();
        assert_eq!(RecentProjects::from_text(&long).paths().len(), MAX_RECENT);
    }

    #[test]
    fn load_and_save_through_a_real_file() {
        let dir = std::env::temp_dir().join(format!("umber-recent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("nested").join("recent");
        assert!(
            RecentProjects::load(&file).paths().is_empty(),
            "missing = empty"
        );
        let mut r = RecentProjects::default();
        r.note(&p("/proj"));
        r.save(&file).expect("save creates the parent dir");
        assert_eq!(RecentProjects::load(&file), r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_flags_missing_paths_and_names_are_the_leaf() {
        let mut r = RecentProjects::default();
        r.note(&p("/gone"));
        r.note(&p("/here"));
        let flags = r.existing(|path| path == Path::new("/here"));
        assert_eq!(
            flags,
            vec![(Path::new("/here"), true), (Path::new("/gone"), false)]
        );
        assert_eq!(display_name(Path::new("/work/robot-arm")), "robot-arm");
        assert_eq!(display_name(Path::new("/")), "/");
    }

    #[test]
    fn recent_lives_beside_the_locale_pref() {
        if let (Some(recent), Some(locale)) = (recent_path(), crate::i18n::locale_pref_path()) {
            assert_eq!(recent.parent(), locale.parent());
            assert_eq!(recent.file_name().unwrap(), "recent");
        }
    }
}
