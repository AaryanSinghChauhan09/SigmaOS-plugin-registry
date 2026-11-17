// Integration test for SigmaOS-plugin-registry
// Verifies the crate builds and links as a dependency.
#![allow(unused_imports)]

use sigmaos_plugin_registry as _;

#[test]
fn crate_links_and_runs() {
    assert_eq!(2 + 2, 4);
}
