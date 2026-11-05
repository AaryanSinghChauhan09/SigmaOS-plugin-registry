#[derive(Debug, Clone)]
pub struct ShellPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: Vec<ShellPlugin>,
}

impl PluginRegistry {
    pub fn new() -> Self { Self::default() }
    pub fn register(&mut self, p: ShellPlugin) { self.plugins.push(p); }
    pub fn count(&self) -> usize { self.plugins.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_plugin_registration() {
        let mut reg = PluginRegistry::new();
        reg.register(ShellPlugin {
            id: "org.sigmaos.battery".into(),
            name: "Battery Monitor".into(),
            version: "1.0.0".into(),
            permissions: vec!["hardware.read".into()],
        });
        assert_eq!(reg.count(), 1);
    }
}
