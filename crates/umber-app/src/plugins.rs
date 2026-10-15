//! WASM plugin loading for the Graph panel — the plugins design's
//! slice 3 (`docs/specs/wasm-plugins-design.md`, "Registry integration").
//!
//! # Where plugins come from ([`plugin_paths`])
//!
//! 1. The per-user plugins dir: the sibling of the brush presets dir
//!    (`$XDG_DATA_HOME/umber/plugins`, `$HOME/.local/share/umber/plugins`,
//!    `%APPDATA%\umber\plugins`).
//! 2. Dev builds only (`debug_assertions`): the repo's `examples/`, where
//!    the three example `.wasm` modules are checked in.
//!
//! Each dir's direct-child `*.wasm` files load in file-name byte order.
//! A missing/unreadable dir is a silent skip (the brush library-loader
//! contract); once a file path is in hand, every failure is a per-file
//! WARNING in the [`PluginLoadReport`] — a broken plugin never takes the
//! graph panel down.
//!
//! # Consent (C2-21)
//!
//! A `.wasm` in the user plugins dir is code someone else wrote, dropped
//! where the app scans on every launch. It does NOT load until the user
//! trusts it: the loader hashes the file (blake3) and loads it only when
//! that hash is in the trust file ([`TrustStore`], `plugins-trusted.txt`
//! beside the plugins dir). Otherwise it is reported as PENDING, and the
//! Graph panel offers a per-file "Trust & load". Trust binds the exact
//! bytes. The hash is taken from the same buffer that is compiled (one
//! read, no swap window), so a replaced or modified file is pending
//! again. The dev `examples/` dir (debug builds only) is implicitly
//! trusted: it is the repo's own checked-in reference plugins.
//!
//! # The collision rule
//!
//! [`umber_graph::NodeRegistry::register`] is a plain map insert: a later
//! registration silently REPLACES an earlier one. The loader does not
//! inherit that. A plugin whose name is already registered — a built-in
//! (`blur`, `flood_fill`, …) or a plugin loaded earlier in the scan — is
//! REJECTED as a load failure, so a plugin can never change what an
//! existing document's built-in node means, and the `.mtlx` writer can
//! never declare the same nodedef twice. First registration wins; the
//! user dir scans before `examples/`, so a user's copy of an example
//! plugin is the one that loads.

use std::path::{Path, PathBuf};

use fluent::FluentValue;
use umber_graph::mtlx::NodedefDecl;
use umber_graph::NodeRegistry;
use umber_wasm::PluginRuntime;

use crate::i18n::tr_args;

/// The outcome of one [`load_plugins`] scan.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PluginLoadReport {
    /// Registered plugin node names, in load order.
    pub loaded: Vec<String>,
    /// Files that did not register: `(path, reason)`, in scan order.
    pub failed: Vec<(PathBuf, String)>,
    /// Untrusted files awaiting consent: `(path, blake3 hex)`, in scan
    /// order. Never compiled.
    pub pending: Vec<(PathBuf, String)>,
}

impl PluginLoadReport {
    /// The panel's status line: `None` when the scan found no `.wasm`
    /// files at all (nothing worth saying), else
    /// `N plugin(s) loaded, M failed[: a.wasm, b.wasm]`.
    pub fn status_line(&self) -> Option<String> {
        if self.loaded.is_empty() && self.failed.is_empty() && self.pending.is_empty() {
            return None;
        }
        let mut line = tr_args(
            "status.plugins",
            [
                ("loaded", FluentValue::from(self.loaded.len())),
                ("failed", FluentValue::from(self.failed.len())),
            ],
        );
        if !self.failed.is_empty() {
            let names: Vec<String> = self
                .failed
                .iter()
                .map(|(path, _)| {
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string())
                })
                .collect();
            line.push_str(": ");
            line.push_str(&names.join(", "));
        }
        if !self.pending.is_empty() {
            // Untranslated until the catalogs take a `.plugins-pending` key.
            line.push_str(&format!(" · {} awaiting trust", self.pending.len()));
        }
        Some(line)
    }
}

/// The plugin dirs in scan order, from an already-resolved user brushes
/// dir (its sibling `plugins/` is the user plugins dir) and the dev
/// `examples/` dir. Pure — [`plugin_paths`] is the env-reading wrapper.
pub fn plugin_paths_from(user_brush_dir: Option<&Path>, dev_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(base) = user_brush_dir.and_then(Path::parent) {
        dirs.push(base.join("plugins"));
    }
    if let Some(dev) = dev_dir {
        dirs.push(dev.to_path_buf());
    }
    dirs
}

/// The repo's `examples/` dir (where the example `.wasm` modules live).
pub fn dev_plugin_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// The plugin dirs for this process: the user plugins dir (when a home
/// is discoverable) then, in dev builds, the repo's `examples/`. Dirs
/// need not exist — the loader skips missing ones silently.
pub fn plugin_paths() -> Vec<PathBuf> {
    let user = umber_brush::preset_library::user_preset_dir();
    let dev = cfg!(debug_assertions).then(dev_plugin_dir);
    plugin_paths_from(user.as_deref(), dev.as_deref())
}

/// The trust file's path: `plugins-trusted.txt` beside the plugins dir
/// (`None` without a discoverable home: then nothing user-side loads).
pub fn trust_path() -> Option<PathBuf> {
    umber_brush::preset_library::user_preset_dir()
        .and_then(|b| b.parent().map(|p| p.join("plugins-trusted.txt")))
}

/// The set of plugin hashes the user has consented to (C2-21). One line
/// per trust, `<blake3 hex> <file name>`. The name is for the human
/// reading the file; only the hash is matched.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TrustStore {
    path: Option<PathBuf>,
    hashes: std::collections::BTreeSet<String>,
}

impl TrustStore {
    /// Reads the trust file at `path`. A missing or unreadable file is an
    /// empty store (nothing trusted), and malformed lines are skipped.
    /// `None` gives an in-memory store that never persists.
    pub fn open(path: Option<PathBuf>) -> Self {
        let mut hashes = std::collections::BTreeSet::new();
        if let Some(text) = path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
        {
            for line in text.lines() {
                if let Some(hash) = line.split_whitespace().next() {
                    if is_blake3_hex(hash) {
                        hashes.insert(hash.to_owned());
                    }
                }
            }
        }
        Self { path, hashes }
    }

    /// Whether `hash` (blake3 hex) is trusted.
    pub fn is_trusted(&self, hash: &str) -> bool {
        self.hashes.contains(hash)
    }

    /// Trusts `hash`, recording `name` beside it, and rewrites the file
    /// (write-then-rename, so a crash never leaves a torn trust list).
    ///
    /// # Errors
    ///
    /// A non-hash `hash` (`InvalidInput`), or the write failing. The
    /// in-memory trust is only added once the file is written, so a
    /// failed save never trusts silently for this session.
    pub fn trust(&mut self, hash: &str, name: &str) -> std::io::Result<()> {
        if !is_blake3_hex(hash) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a blake3 hex digest",
            ));
        }
        if self.hashes.contains(hash) {
            return Ok(());
        }
        if let Some(path) = &self.path {
            let mut text = path
                .exists()
                .then(|| std::fs::read_to_string(path))
                .transpose()?
                .unwrap_or_default();
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            // The name is display-only: keep it one line.
            let name: String = name.chars().filter(|c| !c.is_control()).collect();
            text.push_str(&format!("{hash} {name}\n"));
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let tmp = path.with_extension(format!("txt.tmp.{}", std::process::id()));
            std::fs::write(&tmp, text)?;
            std::fs::rename(&tmp, path)?;
        }
        self.hashes.insert(hash.to_owned());
        Ok(())
    }
}

fn is_blake3_hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Whether `dir` is implicitly trusted: the repo's checked-in `examples/`
/// in a debug build, and nothing else.
pub fn implicitly_trusted(dir: &Path) -> bool {
    cfg!(debug_assertions) && dir == dev_plugin_dir()
}

/// Scans `dirs` in order and registers every loadable, TRUSTED plugin
/// into `registry` (see the module docs for the consent, tolerance and
/// collision rules). Untrusted files are reported pending and never
/// compiled. The registry is untouched by a failed file.
pub fn load_plugins(
    registry: &mut NodeRegistry,
    dirs: &[PathBuf],
    trust: &TrustStore,
) -> PluginLoadReport {
    let mut report = PluginLoadReport::default();
    for dir in dirs {
        let implicit = implicitly_trusted(dir);
        for path in sorted_wasm_files(dir) {
            let bytes = match read_capped(&path) {
                Ok(bytes) => bytes,
                Err(reason) => {
                    log::warn!("plugin {} not loaded: {reason}", path.display());
                    report.failed.push((path, reason));
                    continue;
                }
            };
            let hash = blake3::hash(&bytes).to_hex().to_string();
            if !implicit && !trust.is_trusted(&hash) {
                log::info!("plugin {} awaits trust ({hash})", path.display());
                report.pending.push((path, hash));
                continue;
            }
            match load_bytes(registry, &bytes) {
                Ok(name) => report.loaded.push(name),
                Err(reason) => {
                    log::warn!("plugin {} not loaded: {reason}", path.display());
                    report.failed.push((path, reason));
                }
            }
        }
    }
    report
}

/// Loads one file the user just trusted: re-reads it and loads it only
/// if its bytes STILL hash to `hash` (the file may have changed since it
/// was listed as pending). Returns the registered node name.
///
/// # Errors
///
/// The reason, as in the scan's `failed` entries.
pub fn load_trusted_file(
    registry: &mut NodeRegistry,
    path: &Path,
    hash: &str,
) -> Result<String, String> {
    let bytes = read_capped(path)?;
    if blake3::hash(&bytes).to_hex().as_str() != hash {
        return Err("file changed since it was listed; rescan".to_owned());
    }
    load_bytes(registry, &bytes)
}

/// The `.mtlx` declaration for a loaded plugin node. The v1 wire's
/// node-def record carries only the name and the input/param COUNTS — no
/// param names or types — so the decl declares the one image input
/// (`in: color3`, the filter-node convention) and no params. Param VALUES
/// still round-trip: they ride each `<node>`'s own `<input>` elements,
/// which the mtlx layer preserves regardless of the decl. The decl's job
/// is the interchange contract — the type is declared in-document, so it
/// loads without an unknown-type warning.
pub fn plugin_nodedef(name: &str) -> NodedefDecl {
    NodedefDecl {
        name: name.to_string(),
        category: name.to_string(),
        inputs: vec![("in".into(), "color3".into())],
    }
}

/// Reads `path`, refusing a file over [`umber_wasm::MODULE_SIZE_CAP`] by
/// its metadata BEFORE reading it (a multi-gigabyte `.wasm` never lands
/// in memory). The runtime re-checks the buffer's length.
fn read_capped(path: &Path) -> Result<Vec<u8>, String> {
    let len = std::fs::metadata(path)
        .map_err(|e| format!("cannot read: {e}"))?
        .len();
    if len > umber_wasm::MODULE_SIZE_CAP as u64 {
        return Err(format!(
            "module is {len} bytes, over the {} MiB cap",
            umber_wasm::MODULE_SIZE_CAP >> 20
        ));
    }
    std::fs::read(path).map_err(|e| format!("cannot read: {e}"))
}

/// Compile + contract-check -> collision check -> register.
fn load_bytes(registry: &mut NodeRegistry, bytes: &[u8]) -> Result<String, String> {
    let module = PluginRuntime::new()
        .load(bytes)
        .map_err(|e| e.to_string())?;
    let name = module.def().name.clone();
    if registry.get(&name).is_some() {
        return Err(format!(
            "node name {name:?} is already registered (plugins never shadow)"
        ));
    }
    Ok(umber_wasm::register_module(registry, module))
}

/// Direct-child `*.wasm` files of `dir`, sorted by file name in byte
/// order; empty on any directory-level problem (silent skip).
fn sorted_wasm_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(Vec<u8>, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "wasm"))
        .map(|e| (e.file_name().as_encoded_bytes().to_vec(), e.path()))
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.into_iter().map(|(_, path)| path).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A unique scratch dir under `temp_dir` (no test-dep crates).
    fn fresh_tmpdir(tag: &str) -> PathBuf {
        let id = TMP_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "umber-app-plugins-test-{}-{}-{}",
            std::process::id(),
            id,
            tag
        ));
        std::fs::create_dir_all(&dir).expect("test tmpdir must be creatable");
        dir
    }

    fn blur5_bytes() -> Vec<u8> {
        let path = dev_plugin_dir().join("plugin_blur5.wasm");
        std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{} is git-tracked and must exist: {e}", path.display()))
    }

    #[test]
    fn plugin_paths_put_the_user_sibling_dir_before_examples() {
        let brushes = umber_brush::preset_library::user_preset_dir_from("/home/x", None);
        let dev = Path::new("/repo/examples");
        assert_eq!(
            plugin_paths_from(Some(&brushes), Some(dev)),
            vec![
                PathBuf::from("/home/x/.local/share/umber/plugins"),
                PathBuf::from("/repo/examples"),
            ]
        );
        let xdg = umber_brush::preset_library::user_preset_dir_from("/home/x", Some("/xdg"));
        assert_eq!(
            plugin_paths_from(Some(&xdg), None),
            vec![PathBuf::from("/xdg/umber/plugins")]
        );
        assert!(plugin_paths_from(None, None).is_empty(), "no home, no dev");

        // The env wrapper: every entry is a plugins/ or the examples dir,
        // and dev builds (the test profile) include examples/.
        let live = plugin_paths();
        assert!(live
            .iter()
            .all(|p| p.ends_with("plugins") || p.ends_with("examples")));
        if cfg!(debug_assertions) {
            assert_eq!(live.last(), Some(&dev_plugin_dir()));
        }
    }

    #[test]
    fn loader_registers_the_good_plugin_and_warns_on_the_rest() {
        let dir = fresh_tmpdir("tolerance");
        std::fs::write(dir.join("a_blur5.wasm"), blur5_bytes()).unwrap();
        std::fs::write(dir.join("b_garbage.wasm"), b"not wasm at all").unwrap();
        // A second blur5: the collision rule rejects it (first wins).
        std::fs::write(dir.join("c_blur5_again.wasm"), blur5_bytes()).unwrap();
        // Ignored: not *.wasm.
        std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();
        let missing = dir.join("does-not-exist");

        let mut registry = NodeRegistry::seeded();
        let report = load_plugins(&mut registry, &[missing, dir.clone()], &trust_all_in(&dir));
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(report.loaded, vec!["blur5".to_string()]);
        let failed: Vec<&str> = report
            .failed
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(failed, vec!["b_garbage.wasm", "c_blur5_again.wasm"]);
        assert!(report.failed[1].1.contains("already registered"));
        let defs = registry.node_defs();
        assert!(defs.contains(&"blur5"), "good plugin registers: {defs:?}");
        assert!(defs.contains(&"uniform"), "built-ins survive: {defs:?}");
        assert_eq!(
            report.status_line().as_deref(),
            Some("1 plugin(s) loaded, 2 failed: b_garbage.wasm, c_blur5_again.wasm")
        );
    }

    #[test]
    fn a_plugin_never_shadows_a_built_in() {
        // Pre-register a fake built-in under the plugin's name.
        let mut registry = NodeRegistry::seeded();
        let passthrough = registry.get("passthrough").unwrap();
        registry.register("blur5", passthrough);
        let dir = fresh_tmpdir("shadow");
        std::fs::write(dir.join("blur5.wasm"), blur5_bytes()).unwrap();
        let report = load_plugins(
            &mut registry,
            std::slice::from_ref(&dir),
            &trust_all_in(&dir),
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(report.loaded.is_empty());
        assert_eq!(report.failed.len(), 1);
    }

    #[test]
    fn empty_scan_says_nothing() {
        let report = load_plugins(&mut NodeRegistry::new(), &[], &TrustStore::default());
        assert_eq!(report.status_line(), None);
    }

    /// An in-memory store trusting every `.wasm` currently in `dir`.
    fn trust_all_in(dir: &Path) -> TrustStore {
        let mut store = TrustStore::open(None);
        for path in sorted_wasm_files(dir) {
            let hash = blake3::hash(&std::fs::read(&path).unwrap())
                .to_hex()
                .to_string();
            store.trust(&hash, "test").unwrap();
        }
        store
    }

    #[test]
    fn an_untrusted_user_plugin_is_pending_and_never_registers() {
        let dir = fresh_tmpdir("consent");
        std::fs::write(dir.join("blur5.wasm"), blur5_bytes()).unwrap();
        let mut registry = NodeRegistry::seeded();
        let report = load_plugins(
            &mut registry,
            std::slice::from_ref(&dir),
            &TrustStore::default(),
        );
        assert!(report.loaded.is_empty(), "consent first: {report:?}");
        assert_eq!(report.pending.len(), 1);
        let expected = blake3::hash(&blur5_bytes()).to_hex().to_string();
        assert_eq!(report.pending[0].1, expected, "the hash of the exact bytes");
        assert!(
            registry.get("blur5").is_none(),
            "never compiled, never registered"
        );
        assert_eq!(
            report.status_line().as_deref(),
            Some("0 plugin(s) loaded, 0 failed · 1 awaiting trust")
        );

        // Trusting through a real file, then loading the pending entry.
        let trust_file = dir.join("trust").join("plugins-trusted.txt");
        let mut trust = TrustStore::open(Some(trust_file.clone()));
        trust.trust(&expected, "blur5.wasm").unwrap();
        let name = load_trusted_file(&mut registry, &report.pending[0].0, &expected).unwrap();
        assert_eq!(name, "blur5");
        // The trust persists: a fresh scan with a reopened store loads it
        // (into a fresh registry: the old one already holds blur5).
        let reopened = TrustStore::open(Some(trust_file.clone()));
        assert!(reopened.is_trusted(&expected));
        let again = load_plugins(
            &mut NodeRegistry::seeded(),
            std::slice::from_ref(&dir),
            &reopened,
        );
        assert_eq!(again.loaded, vec!["blur5".to_string()]);
        assert!(again.pending.is_empty());

        // Changing ONE byte revokes the trust: the file is pending again.
        let mut bytes = blur5_bytes();
        bytes.push(0);
        std::fs::write(dir.join("blur5.wasm"), &bytes).unwrap();
        let changed = load_plugins(
            &mut NodeRegistry::seeded(),
            std::slice::from_ref(&dir),
            &reopened,
        );
        assert!(changed.loaded.is_empty());
        assert_eq!(changed.pending.len(), 1);
        // ...and the stale "trust & load" refuses the swapped file.
        assert!(load_trusted_file(
            &mut NodeRegistry::seeded(),
            &dir.join("blur5.wasm"),
            &expected
        )
        .unwrap_err()
        .contains("changed"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_trust_file_rejects_junk_and_keeps_one_line_per_entry() {
        let dir = fresh_tmpdir("trust-file");
        let path = dir.join("plugins-trusted.txt");
        let good = "a".repeat(64);
        std::fs::write(
            &path,
            format!(
                "{good} ok.wasm\nnot-a-hash x\n{} upper.wasm\n\n",
                "A".repeat(64)
            ),
        )
        .unwrap();
        let mut store = TrustStore::open(Some(path.clone()));
        assert!(store.is_trusted(&good));
        assert!(
            !store.is_trusted(&"A".repeat(64)),
            "uppercase is not a blake3 hex digest"
        );
        assert!(store.trust("nope", "x").is_err());
        let second = "b".repeat(64);
        store.trust(&second, "evil\nname.wasm").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.ends_with(&format!("{second} evilname.wasm\n")),
            "{text:?}"
        );
        // Idempotent: trusting again writes nothing new.
        store.trust(&second, "again").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        assert!(TrustStore::open(None).hashes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_oversized_module_is_refused_before_it_is_read() {
        let dir = fresh_tmpdir("size-cap");
        let path = dir.join("huge.wasm");
        let file = std::fs::File::create(&path).unwrap();
        // Sparse: the length is over the cap without writing 8 MiB.
        file.set_len(umber_wasm::MODULE_SIZE_CAP as u64 + 1)
            .unwrap();
        let report = load_plugins(
            &mut NodeRegistry::seeded(),
            std::slice::from_ref(&dir),
            &TrustStore::default(),
        );
        assert!(
            report.pending.is_empty(),
            "never hashed, so never offered for trust"
        );
        assert_eq!(report.failed.len(), 1);
        assert!(report.failed[0].1.contains("cap"), "{:?}", report.failed);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
