// SPDX-License-Identifier: Apache-2.0
//!
//! SigmaOS-plugin-registry — plugin registry & dependency manager.
//!
//! Tracks installed SigmaOS plugins: metadata, versions, dependencies,
//! conflicts, enabled state, and load order. Resolves dependency graphs
//! (topological), detects cycles/conflicts, and validates manifests
//! before install. Marketplace installs delegate to a `PluginBackend`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

/// Semantic-ish version: major.minor.patch with optional pre-release tag.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub pre: Option<String>,
}
impl Version {
    pub fn parse(s: &str) -> Result<Self, PluginError> {
        let (core, pre) = match s.split_once('-') {
            Some((c,p)) => (c, Some(p.to_string())),
            None => (s, None),
        };
        let parts: Vec<&str> = core.split('.').collect();
        if parts.len() != 3 { return Err(PluginError::BadVersion(s.into())); }
        Ok(Self {
            major: parts[0].parse().map_err(|_| PluginError::BadVersion(s.into()))?,
            minor: parts[1].parse().map_err(|_| PluginError::BadVersion(s.into()))?,
            patch: parts[2].parse().map_err(|_| PluginError::BadVersion(s.into()))?,
            pre,
        })
    }
    pub fn as_str(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.patch);
        if let Some(p) = &self.pre { s.push('-'); s.push_str(p); }
        s
    }
}
impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.as_str()) }
}

/// Pad a partial version "1.2" → "1.2.0" for requirement parsing.
fn pad_version(s: &str) -> String {
    let core = s.split('-').next().unwrap_or(s);
    let n = core.split('.').count();
    match n {
        1 => format!("{}.0.0{}", core, s.strip_prefix(core).unwrap_or("")),
        2 => format!("{}.0{}", core, s.strip_prefix(core).unwrap_or("")),
        _ => s.to_string(),
    }
}

/// Version requirement: "^1.2", "~1.2.3", ">=1.0", "=2.0.0", "*".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Req {
    Any,
    Exact(Version),
    Gte(Version),
    Lt(Version),
    Compatible(Version), // ^x.y.z
    Pessimistic(Version), // ~x.y.z
}
impl Req {
    pub fn parse(s: &str) -> Result<Self, PluginError> {
        let s = s.trim();
        if s == "*" || s.is_empty() { return Ok(Self::Any); }
        if let Some(v) = s.strip_prefix('=') { return Ok(Self::Exact(Version::parse(&pad_version(v))?)); }
        if let Some(v) = s.strip_prefix(">=") { return Ok(Self::Gte(Version::parse(&pad_version(v))?)); }
        if let Some(v) = s.strip_prefix('<') { return Ok(Self::Lt(Version::parse(&pad_version(v))?)); }
        if let Some(v) = s.strip_prefix('^') { return Ok(Self::Compatible(Version::parse(&pad_version(v))?)); }
        if let Some(v) = s.strip_prefix('~') { return Ok(Self::Pessimistic(Version::parse(&pad_version(v))?)); }
        // bare "1.2" → compatible; "1.2.3" → exact-ish compatible
        // reqs allow partial versions: pad missing components with 0
        let padded = pad_version(s);
        Ok(Self::Compatible(Version::parse(&padded)?))
    }
    pub fn matches(&self, v: &Version) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(r) => v == r,
            Self::Gte(r) => v >= r,
            Self::Lt(r) => v < r,
            Self::Compatible(r) => {
                // same major, >= r
                v.major == r.major && v >= r
            }
            Self::Pessimistic(r) => {
                // same major.minor, >= r
                v.major == r.major && v.minor == r.minor && v >= r
            }
        }
    }
}

/// A plugin manifest.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub description: String,
    /// plugin kind: "extension" | "theme" | "driver" | "service".
    pub kind: String,
    /// entry point path (relative to plugin dir).
    pub entry: String,
    /// dependencies: plugin-id → version req.
    pub depends: BTreeMap<String, String>,
    /// plugins this one conflicts with.
    pub conflicts: Vec<String>,
    /// min host API version.
    pub min_api: Option<Version>,
    /// permissions requested (informational).
    pub permissions: Vec<String>,
}

/// Installed plugin record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Installed {
    pub manifest: Manifest,
    pub enabled: bool,
    pub install_path: PathBuf,
    pub installed_unix: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PluginError {
    DuplicatePlugin(String),
    NoSuchPlugin(String),
    BadVersion(String),
    BadReq(String),
    InvalidManifest(String),
    DependencyMissing { plugin: String, need: String },
    DependencyConflict { plugin: String, have: String, need: String },
    ConflictDetected { a: String, b: String },
    CycleDetected(String),
    Backend(String),
    Disabled(String),
}
impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicatePlugin(id)=>write!(f,"plugin {id} installed"),
            Self::NoSuchPlugin(id)=>write!(f,"no plugin {id}"),
            Self::BadVersion(v)=>write!(f,"bad version {v}"),
            Self::BadReq(r)=>write!(f,"bad requirement {r}"),
            Self::InvalidManifest(m)=>write!(f,"invalid manifest: {m}"),
            Self::DependencyMissing{plugin,need}=>write!(f,"{plugin} requires missing dep {need}"),
            Self::DependencyConflict{plugin,have,need}=>write!(f,"{plugin} needs {need} but {have} installed"),
            Self::ConflictDetected{a,b}=>write!(f,"{a} conflicts with {b}"),
            Self::CycleDetected(c)=>write!(f,"dependency cycle at {c}"),
            Self::Backend(e)=>write!(f,"backend: {e}"),
            Self::Disabled(id)=>write!(f,"plugin {id} disabled"),
        }
    }
}
impl std::error::Error for PluginError {}

/// Backend for plugin payload operations.
pub trait PluginBackend {
    /// Fetch plugin payload (manifest + files) by id+version.
    /// Returns manifest and install dir path.
    fn fetch(&self, id: &str, version: &Version) -> Result<(Manifest, PathBuf), String>;
    /// Remove plugin payload dir.
    fn remove_payload(&self, path: &Path) -> Result<(), String>;
    /// Current unix time.
    fn now_unix(&self) -> u64;
}

/// Host backend: reads from a local plugins dir (SIGMAOS_PLUGIN_DIR).
pub struct HostBackend { pub root: PathBuf }
impl HostBackend {
    pub fn new() -> Self {
        Self { root: std::env::var("SIGMAOS_PLUGIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/usr/lib/sigmaos/plugins")) }
    }
}
impl PluginBackend for HostBackend {
    fn fetch(&self, id: &str, version: &Version) -> Result<(Manifest, PathBuf), String> {
        // look for <root>/<id>-<version>/manifest.json
        let dir = self.root.join(format!("{}-{}", id, version.as_str()));
        let mpath = dir.join("manifest.json");
        let txt = std::fs::read_to_string(&mpath).map_err(|e| format!("{}: {}", mpath.display(), e))?;
        let manifest: Manifest = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
        Ok((manifest, dir))
    }
    fn remove_payload(&self, path: &Path) -> Result<(), String> {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    }
    fn now_unix(&self) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
    }
}

/// In-memory backend for tests.
pub struct FakeBackend {
    pub catalog: std::cell::RefCell<BTreeMap<(String, String), (Manifest, PathBuf)>>,
    pub now: std::cell::Cell<u64>,
}
impl FakeBackend {
    pub fn new() -> Self {
        Self { catalog: std::cell::RefCell::new(BTreeMap::new()),
            now: std::cell::Cell::new(1_700_000_000) }
    }
    pub fn add(&self, m: Manifest) {
        let key = (m.id.clone(), m.version.as_str());
        self.catalog.borrow_mut().insert(key, (m, PathBuf::from("/fake/plugins/x")));
    }
}
impl PluginBackend for FakeBackend {
    fn fetch(&self, id: &str, version: &Version) -> Result<(Manifest, PathBuf), String> {
        self.catalog.borrow().get(&(id.into(), version.as_str()))
            .cloned().ok_or_else(|| format!("not in catalog: {id} {version}"))
    }
    fn remove_payload(&self, _p: &Path) -> Result<(), String> { Ok(()) }
    fn now_unix(&self) -> u64 { self.now.get() }
}

/// The plugin registry.
pub struct SigmaOSPluginRegistry {
    pub plugins: BTreeMap<String, Installed>,
    /// Load order (computed from deps).
    pub load_order: Vec<String>,
}

impl SigmaOSPluginRegistry {
    pub fn new() -> Self {
        Self { plugins: BTreeMap::new(), load_order: vec![] }
    }

    /// Validate a manifest before install.
    pub fn validate(&self, m: &Manifest) -> Result<(), PluginError> {
        if m.id.is_empty() { return Err(PluginError::InvalidManifest("empty id".into())); }
        if m.entry.is_empty() { return Err(PluginError::InvalidManifest("empty entry".into())); }
        if m.kind.is_empty() { return Err(PluginError::InvalidManifest("empty kind".into())); }
        // check deps exist & satisfy reqs
        for (dep_id, req_s) in &m.depends {
            let req = Req::parse(req_s).map_err(|_| PluginError::BadReq(req_s.clone()))?;
            match self.plugins.get(dep_id) {
                None => return Err(PluginError::DependencyMissing {
                    plugin: m.id.clone(), need: format!("{dep_id} {req_s}") }),
                Some(inst) => {
                    if !req.matches(&inst.manifest.version) {
                        return Err(PluginError::DependencyConflict {
                            plugin: m.id.clone(), have: inst.manifest.version.as_str(),
                            need: format!("{dep_id} {req_s}") });
                    }
                }
            }
        }
        // conflicts
        for c in &m.conflicts {
            if self.plugins.contains_key(c) {
                return Err(PluginError::ConflictDetected { a: m.id.clone(), b: c.clone() });
            }
        }
        // also check if any installed plugin conflicts with this one
        for inst in self.plugins.values() {
            if inst.manifest.conflicts.contains(&m.id) {
                return Err(PluginError::ConflictDetected { a: inst.manifest.id.clone(), b: m.id.clone() });
            }
        }
        Ok(())
    }

    /// Install a plugin (manifest already validated/fetched).
    pub fn install(&mut self, manifest: Manifest, path: PathBuf, now: u64) -> Result<(), PluginError> {
        self.validate(&manifest)?;
        if self.plugins.contains_key(&manifest.id) {
            return Err(PluginError::DuplicatePlugin(manifest.id.clone()));
        }
        let inst = Installed { manifest, enabled: true, install_path: path, installed_unix: now };
        self.plugins.insert(inst.manifest.id.clone(), inst);
        self.recompute_order()?;
        Ok(())
    }

    /// Install from backend (fetch + install).
    pub fn install_from<B: PluginBackend>(&mut self, be: &B, id: &str, version: &Version) -> Result<(), PluginError> {
        let (manifest, path) = be.fetch(id, version).map_err(PluginError::Backend)?;
        if manifest.id != id {
            return Err(PluginError::InvalidManifest(format!("id mismatch: manifest={} requested={}", manifest.id, id)));
        }
        if &manifest.version != version {
            return Err(PluginError::InvalidManifest(format!("version mismatch: manifest={} requested={}", manifest.version, version)));
        }
        let now = be.now_unix();
        self.install(manifest, path, now)
    }

    /// Uninstall a plugin (fails if others depend on it).
    pub fn uninstall<B: PluginBackend>(&mut self, be: &B, id: &str) -> Result<(), PluginError> {
        if !self.plugins.contains_key(id) { return Err(PluginError::NoSuchPlugin(id.into())); }
        // check dependents
        for inst in self.plugins.values() {
            if inst.manifest.depends.contains_key(id) {
                return Err(PluginError::DependencyMissing {
                    plugin: inst.manifest.id.clone(), need: format!("{id} (required by {})", inst.manifest.id) });
            }
        }
        let inst = self.plugins.remove(id).unwrap();
        let _ = be.remove_payload(&inst.install_path);
        self.recompute_order()?;
        Ok(())
    }

    /// Enable/disable a plugin.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), PluginError> {
        let inst = self.plugins.get_mut(id).ok_or_else(|| PluginError::NoSuchPlugin(id.into()))?;
        inst.enabled = enabled;
        self.recompute_order()?;
        Ok(())
    }

    /// Recompute load order via topological sort on enabled plugins.
    pub fn recompute_order(&mut self) -> Result<(), PluginError> {
        // build dep graph among installed plugins
        let mut g: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for inst in self.plugins.values().filter(|i| i.enabled) {
            let deps: Vec<String> = inst.manifest.depends.keys()
                .filter(|d| self.plugins.contains_key(*d))
                .filter(|d| self.plugins.get(*d).map(|p| p.enabled).unwrap_or(false))
                .cloned().collect();
            g.insert(inst.manifest.id.clone(), deps);
        }
        let mut order = vec![];
        let mut visited = BTreeSet::new();
        let mut temp = BTreeSet::new();
        fn visit(n: &str, g: &BTreeMap<String,Vec<String>>, v: &mut BTreeSet<String>,
                 t: &mut BTreeSet<String>, order: &mut Vec<String>) -> Result<(), PluginError> {
            if v.contains(n) { return Ok(()); }
            if !t.insert(n.into()) { return Err(PluginError::CycleDetected(n.into())); }
            if let Some(deps) = g.get(n) {
                for d in deps { visit(d, g, v, t, order)?; }
            }
            t.remove(n); v.insert(n.into());
            order.push(n.into());
            Ok(())
        }
        let names: Vec<String> = g.keys().cloned().collect();
        for n in &names { visit(n, &g, &mut visited, &mut temp, &mut order)?; }
        self.load_order = order;
        Ok(())
    }

    /// Plugins that depend on a given plugin (direct only).
    pub fn dependents(&self, id: &str) -> Vec<String> {
        self.plugins.values()
            .filter(|i| i.manifest.depends.contains_key(id))
            .map(|i| i.manifest.id.clone()).collect()
    }

    /// Installed plugins that declare a conflict with `id`
    /// (either direction: p.conflicts contains id, or id's manifest
    /// conflicts contains p — resolved by caller knowing both sides).
    pub fn conflicts_with(&self, id: &str) -> Vec<String> {
        self.plugins.values()
            .filter(|i| i.manifest.id != id && i.manifest.conflicts.iter().any(|c| c == id))
            .map(|i| i.manifest.id.clone()).collect()
    }

    /// Enabled plugins in load order.
    pub fn enabled_in_order(&self) -> Vec<&Installed> {
        self.load_order.iter()
            .filter_map(|id| self.plugins.get(id))
            .filter(|i| i.enabled)
            .collect()
    }

    /// Stats.
    pub fn stats(&self) -> (usize, usize, usize) {
        let total = self.plugins.len();
        let enabled = self.plugins.values().filter(|i| i.enabled).count();
        (total, enabled, self.load_order.len())
    }

    pub fn status_line(&self) -> String {
        let (t,e,l) = self.stats();
        format!("plugins: {} installed, {} enabled, load-order={}", t, e, l)
    }
}

impl Default for SigmaOSPluginRegistry { fn default() -> Self { Self::new() } }

// ---- persistence ----
#[derive(serde::Serialize, serde::Deserialize)]
struct Persist {
    plugins: Vec<Installed>,
    load_order: Vec<String>,
}
impl SigmaOSPluginRegistry {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let p = Persist { plugins: self.plugins.values().cloned().collect(), load_order: self.load_order.clone() };
        let j = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        if let Some(d) = path.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
        std::fs::write(path, j).map_err(|e| e.to_string())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let txt = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let p: Persist = serde_json::from_str(&txt).map_err(|e| e.to_string())?;
        let mut m = Self { plugins: BTreeMap::new(), load_order: p.load_order };
        for i in p.plugins { m.plugins.insert(i.manifest.id.clone(), i); }
        Ok(m)
    }
    pub fn default_path() -> PathBuf {
        if let Ok(p) = std::env::var("SIGMAOS_PLUGIN_REG_FILE") { return PathBuf::from(p); }
        PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
            .join(".local/share/sigmaos/plugins.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, ver: &str, deps: &[(&str,&str)], conflicts: &[&str]) -> Manifest {
        Manifest {
            id: id.into(), name: id.into(),
            version: Version::parse(ver).unwrap(),
            description: format!("{} plugin", id),
            kind: "extension".into(), entry: format!("{id}.so"),
            depends: deps.iter().map(|(a,b)| (a.to_string(), b.to_string())).collect(),
            conflicts: conflicts.iter().map(|s| s.to_string()).collect(),
            min_api: None, permissions: vec![],
        }
    }

    #[test]
    fn version_parse() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.as_str(), "1.2.3");
        let v2 = Version::parse("2.0.0-rc1").unwrap();
        assert_eq!(v2.pre.as_deref(), Some("rc1"));
        assert!(Version::parse("1.2").is_err());
        assert!(Version::parse("x.y.z").is_err());
    }

    #[test]
    fn req_matching() {
        let v = Version::parse("1.5.0").unwrap();
        assert!(Req::parse("*").unwrap().matches(&v));
        assert!(Req::parse("^1.2.0").unwrap().matches(&v));
        assert!(!Req::parse("^2.0.0").unwrap().matches(&v));
        assert!(Req::parse(">=1.0.0").unwrap().matches(&v));
        assert!(Req::parse("=1.5.0").unwrap().matches(&v));
        assert!(!Req::parse("=1.5.1").unwrap().matches(&v));
        assert!(Req::parse("~1.5.0").unwrap().matches(&v));
        assert!(!Req::parse("~1.4.0").unwrap().matches(&v));
    }

    #[test]
    fn install_and_deps() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        // base plugin no deps
        fb.add(manifest("base","1.0.0",&[],&[]));
        reg.install_from(&fb, "base", &Version::parse("1.0.0").unwrap()).unwrap();
        // dependent plugin
        fb.add(manifest("ext","1.0.0",&[("base","^1.0")],&[]));
        reg.install_from(&fb, "ext", &Version::parse("1.0.0").unwrap()).unwrap();
        // dep missing → fail
        fb.add(manifest("orphan","1.0.0",&[("nonexistent","*")],&[]));
        assert!(reg.install_from(&fb, "orphan", &Version::parse("1.0.0").unwrap()).is_err());
        // dep version mismatch → fail
        fb.add(manifest("bad-dep","1.0.0",&[("base","^2.0")],&[]));
        assert!(reg.install_from(&fb, "bad-dep", &Version::parse("1.0.0").unwrap()).is_err());
        // load order: base before ext
        assert_eq!(reg.load_order, vec!["base".to_string(), "ext".to_string()]);
    }

    #[test]
    fn conflicts_rejected() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("theme-a","1.0.0",&[],&["theme-b"]));
        fb.add(manifest("theme-b","1.0.0",&[],&[]));
        reg.install_from(&fb, "theme-a", &Version::parse("1.0.0").unwrap()).unwrap();
        // theme-b conflicts implicitly? theme-a lists theme-b in conflicts → installing theme-b fails
        assert!(reg.install_from(&fb, "theme-b", &Version::parse("1.0.0").unwrap()).is_err());
        // reverse: remove theme-a, install theme-b then theme-a fails
        reg.uninstall(&fb, "theme-a").unwrap();
        reg.install_from(&fb, "theme-b", &Version::parse("1.0.0").unwrap()).unwrap();
        assert!(reg.install_from(&fb, "theme-a", &Version::parse("1.0.0").unwrap()).is_err());
    }

    #[test]
    fn uninstall_blocked_by_dependents() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("base","1.0.0",&[],&[]));
        fb.add(manifest("ext","1.0.0",&[("base","*")],&[]));
        reg.install_from(&fb, "base", &Version::parse("1.0.0").unwrap()).unwrap();
        reg.install_from(&fb, "ext", &Version::parse("1.0.0").unwrap()).unwrap();
        // can't remove base while ext depends on it
        assert!(reg.uninstall(&fb, "base").is_err());
        // remove ext first, then base
        reg.uninstall(&fb, "ext").unwrap();
        reg.uninstall(&fb, "base").unwrap();
        assert_eq!(reg.plugins.len(), 0);
    }

    #[test]
    fn enable_disable_affects_order() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("a","1.0.0",&[],&[]));
        fb.add(manifest("b","1.0.0",&[("a","*")],&[]));
        reg.install_from(&fb, "a", &Version::parse("1.0.0").unwrap()).unwrap();
        reg.install_from(&fb, "b", &Version::parse("1.0.0").unwrap()).unwrap();
        assert_eq!(reg.enabled_in_order().len(), 2);
        // disable a → b loses dep satisfaction? b still installed but order recomputes
        reg.set_enabled("a", false).unwrap();
        // load order only includes enabled: b has dep on a (disabled) → dep filtered out
        assert_eq!(reg.load_order.len(), 1);
        assert_eq!(reg.load_order[0], "b");
    }

    #[test]
    fn dependency_cycle_detected() {
        let mut reg = SigmaOSPluginRegistry::new();
        // can't create cycle via install (deps must exist) — construct manually
        let m1 = manifest("x","1.0.0",&[("y","*")],&[]);
        let m2 = manifest("y","1.0.0",&[("x","*")],&[]);
        // install x first (y missing → would fail validation)... install bypass:
        reg.plugins.insert("x".into(), Installed { manifest: m1, enabled: true,
            install_path: PathBuf::from("/x"), installed_unix: 0 });
        reg.plugins.insert("y".into(), Installed { manifest: m2, enabled: true,
            install_path: PathBuf::from("/y"), installed_unix: 0 });
        assert!(reg.recompute_order().is_err());
    }

    #[test]
    fn duplicate_install_rejected() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("p","1.0.0",&[],&[]));
        reg.install_from(&fb, "p", &Version::parse("1.0.0").unwrap()).unwrap();
        assert!(reg.install_from(&fb, "p", &Version::parse("1.0.0").unwrap()).is_err());
    }

    #[test]
    fn manifest_id_version_mismatch_rejected() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("real","1.0.0",&[],&[]));
        // request wrong version
        assert!(reg.install_from(&fb, "real", &Version::parse("2.0.0").unwrap()).is_err());
        // request wrong id
        assert!(reg.install_from(&fb, "fake", &Version::parse("1.0.0").unwrap()).is_err());
    }

    #[test]
    fn dependents_and_conflicts_queries() {
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("base","1.0.0",&[],&[]));
        fb.add(manifest("ext","1.0.0",&[("base","*")],&["rival"]));
        fb.add(manifest("rival","1.0.0",&[],&["ext"]));
        reg.install_from(&fb, "base", &Version::parse("1.0.0").unwrap()).unwrap();
        reg.install_from(&fb, "ext", &Version::parse("1.0.0").unwrap()).unwrap();
        // rival declares conflict with ext → can't install while ext present
        assert!(reg.install_from(&fb, "rival", &Version::parse("1.0.0").unwrap()).is_err());
        assert_eq!(reg.dependents("base"), vec!["ext".to_string()]);
        // ext's manifest conflicts rival; rival (not installed) n/a — check reverse query on installed set
        assert!(reg.plugins.get("ext").unwrap().manifest.conflicts.contains(&"rival".to_string()));
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sigmaos-plug-test-{}", std::process::id()));
        let p = dir.join("plugins.json");
        let mut reg = SigmaOSPluginRegistry::new();
        let fb = FakeBackend::new();
        fb.add(manifest("a","1.0.0",&[],&[]));
        reg.install_from(&fb, "a", &Version::parse("1.0.0").unwrap()).unwrap();
        reg.save(&p).unwrap();
        let reg2 = SigmaOSPluginRegistry::load(&p).unwrap();
        assert_eq!(reg2.plugins.len(), 1);
        assert!(reg2.plugins.contains_key("a"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
