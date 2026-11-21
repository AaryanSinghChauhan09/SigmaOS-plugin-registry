// SPDX-License-Identifier: Apache-2.0
use sigmaos_plugin_registry::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[test]
fn end_to_end_registry_workflow() {
    let mut reg = SigmaOSPluginRegistry::new();
    let fb = FakeBackend::new();
    let m1 = Manifest { id:"core".into(), name:"Core".into(), version:Version::parse("2.1.0").unwrap(),
        description:"d".into(), kind:"service".into(), entry:"core.so".into(),
        depends:BTreeMap::new(), conflicts:vec![], min_api:None, permissions:vec![] };
    let m2 = Manifest { id:"ui".into(), name:"UI".into(), version:Version::parse("1.0.0").unwrap(),
        description:"d".into(), kind:"extension".into(), entry:"ui.so".into(),
        depends:{let mut d=BTreeMap::new();d.insert("core".into(),"^2.0".into());d},
        conflicts:vec![], min_api:None, permissions:vec![] };
    fb.add(m1); fb.add(m2);
    reg.install_from(&fb,"core",&Version::parse("2.1.0").unwrap()).unwrap();
    reg.install_from(&fb,"ui",&Version::parse("1.0.0").unwrap()).unwrap();
    assert_eq!(reg.load_order, vec!["core".to_string(),"ui".to_string()]);
    assert_eq!(reg.enabled_in_order().len(), 2);
    // disable core → ui still loads (dep filtered) but order shrinks
    reg.set_enabled("core", false).unwrap();
    assert_eq!(reg.load_order.len(), 1);
}
