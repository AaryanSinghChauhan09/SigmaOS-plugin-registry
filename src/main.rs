//! SigmaOS-plugin-registry — plugin registry CLI.
//!
//! Usage:
//!   sigmaos-plugins status | list | order | info <id>
//!   sigmaos-plugins install <id> <version>   (from SIGMAOS_PLUGIN_DIR catalog)
//!   sigmaos-plugins remove <id> | enable <id> | disable <id>
//!   sigmaos-plugins deps <id> | dependents <id>
//!
//! Catalog layout: $SIGMAOS_PLUGIN_DIR/<id>-<version>/manifest.json
//! State: ~/.local/share/sigmaos/plugins.json

use sigmaos_plugin_registry::{HostBackend, SigmaOSPluginRegistry, PluginBackend, Version};
use std::process::exit;

fn usage() -> ! {
    eprintln!("usage: sigmaos-plugins <status|list|order|info|install|remove|enable|disable|deps|dependents> [args]");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage(); }
    let db = SigmaOSPluginRegistry::default_path();
    let mut reg = SigmaOSPluginRegistry::load(&db).unwrap_or_else(|_| SigmaOSPluginRegistry::new());
    let be = HostBackend::new();
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let arg = |i: usize| -> &str { rest.get(i).map(|s| s.as_str()).unwrap_or_else(|| usage()) };

    let result: Result<(), String> = (|| -> Result<(), String> { match cmd {
        "status" => { println!("{}", reg.status_line()); Ok(()) }
        "list" => {
            for i in reg.plugins.values() {
                println!("{:<20} {:<10} {:<12} enabled={} kind={} deps={}",
                    i.manifest.id, i.manifest.version.as_str(),
                    if i.enabled {"yes"} else {"no"}, i.enabled,
                    i.manifest.kind, i.manifest.depends.len());
            }
            Ok(())
        }
        "order" => {
            for (n,id) in reg.load_order.iter().enumerate() {
                let en = reg.plugins.get(id).map(|i| i.enabled).unwrap_or(false);
                println!("{:<3} {:<20} {}", n+1, id, if en {""} else {"(disabled)"});
            }
            Ok(())
        }
        "info" => {
            let i = reg.plugins.get(arg(0)).ok_or_else(|| format!("no plugin {}", arg(0)))?;
            let m = &i.manifest;
            println!("id: {}", m.id);
            println!("name: {}", m.name);
            println!("version: {}", m.version);
            println!("kind: {}", m.kind);
            println!("entry: {}", m.entry);
            println!("description: {}", m.description);
            println!("enabled: {}", i.enabled);
            println!("path: {}", i.install_path.display());
            if !m.depends.is_empty() {
                println!("depends:");
                for (d,r) in &m.depends { println!("  {} {}", d, r); }
            }
            if !m.conflicts.is_empty() { println!("conflicts: {}", m.conflicts.join(", ")); }
            if !m.permissions.is_empty() { println!("permissions: {}", m.permissions.join(", ")); }
            Ok(())
        }
        "install" => {
            let v = Version::parse(arg(1)).map_err(|e| e.to_string())?;
            reg.install_from(&be, arg(0), &v).map_err(|e| e.to_string())?;
            println!("installed {} {}", arg(0), v); Ok(())
        }
        "remove" => { reg.uninstall(&be, arg(0)).map_err(|e| e.to_string())?; println!("removed"); Ok(()) }
        "enable" => { reg.set_enabled(arg(0), true).map_err(|e| e.to_string())?; println!("enabled"); Ok(()) }
        "disable" => { reg.set_enabled(arg(0), false).map_err(|e| e.to_string())?; println!("disabled"); Ok(()) }
        "deps" => {
            let i = reg.plugins.get(arg(0)).ok_or_else(|| format!("no plugin {}", arg(0)))?;
            for (d,r) in &i.manifest.depends { println!("{} {}", d, r); }
            Ok(())
        }
        "dependents" => {
            for d in reg.dependents(arg(0)) { println!("{}", d); }
            Ok(())
        }
        _ => usage(),
    } })();

    match result {
        Ok(()) => { if let Err(e)=reg.save(&db) { eprintln!("sigmaos-plugins: warn: save: {e}"); } }
        Err(e) => { eprintln!("sigmaos-plugins: {e}"); exit(1); }
    }
}
