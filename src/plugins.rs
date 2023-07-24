use std::path::Path;
use std::fs;
use std::ffi::OsString;
use dlopen::wrapper::{Container, WrapperApi};

// ========================= Plugin traits ========================= //

// NodeLink client
pub trait NodeLinkClient {
    fn query_accounts(&mut self, addresses: Vec<String>) -> Result<Vec<String>, String>;
}

// Link to Node (front or console)
pub trait NodeLink {
    fn create_client(&self, cfg_str: String) -> Box<dyn NodeLinkClient>;
    fn name(&self) -> String;
}

#[derive(WrapperApi)]
pub struct PluginApi {
    get_node_link: extern fn() -> *mut dyn NodeLink
}

// ^^^^^^^^^^^^^^^^^^^^^^^^^ Plugin traits ^^^^^^^^^^^^^^^^^^^^^^^^^ //

#[cfg(any(target_os = "linux",
          target_os = "freebsd",
          target_os = "dragonfly"))]
fn get_dylib_extension() -> OsString {
    OsString::from("so")
}

#[cfg(target_os = "macos")]
fn get_dylib_extension() -> OsString {
    OsString::from("dylib")
}

#[cfg(target_os = "windows")]
fn get_dylib_extension() -> OsString {
    OsString::from("dll")
}

pub struct Plugin {
    lib_path:   Box<Path>,
    cfg_path:   Box<Path>,
    plugin_api: Container<PluginApi>,
    link:       Option<Box<dyn NodeLink>>
}

pub struct Plugins {
    dir_path: Box<Path>,
    plugins:  Vec<Plugin>
}

impl Plugin {
    pub fn new(lib_path: Box<Path>, cfg_path: Box<Path>) -> Self {
        let plugin_api: Container<PluginApi> = unsafe { Container::load(lib_path.as_os_str()) }.unwrap();
        let link = unsafe {
            let ptr = plugin_api.get_node_link();
            if !ptr.is_null() { Some(Box::from_raw(ptr)) } else { None }
        };
        Self { lib_path, cfg_path, plugin_api, link }
    }
    pub fn create_client(&self) -> Box<dyn NodeLinkClient> {
        let cfg_str = std::fs::read_to_string(&self.cfg_path).expect(format!("Can't load plugin config ({})", self.cfg_path.display()).as_str());
        self.link.as_ref().expect(format!("create_client for plugin empty link").as_str()).create_client(cfg_str)
    }
    pub fn link_name(&self) -> Option<String> {
        self.link.as_ref().map(|x| x.name())
    }
}

impl Plugins {
    pub fn new(dir_path: Box<Path>) -> Self {
        let plugins = Self::iterate_dir(dir_path.as_ref()).unwrap();
        Self { dir_path, plugins }
    }
    pub fn link(&self, name: &str) -> Option<&Plugin> {
        for plugin in &self.plugins {
            match plugin.link.as_ref() {
                Some(link) => {
                    let link_name = link.name();
                    println!("> Found plugin link: {}", link_name);
                    if link_name == name {
                        return Some(&plugin);
                    }
                },
                None => {
                    println!("> Noname plugin link: {}", plugin.lib_path.display());
                }
            }
        }
        return None;
    }
    pub fn link_names(&self) -> Vec<String> {
        self.plugins.iter().filter_map(|x| x.link_name()).collect()
    }
    pub fn create_client(&self, link_name: &str) -> Box<dyn NodeLinkClient> {
        self.link(link_name).expect(format!("Can't create link with name ({link_name})").as_str()).
            create_client()
    }
    fn iterate_dir(dir_path: &Path) -> Result<Vec<Plugin>, String> {
        let mut rv = Vec::<Plugin>::new();
        let dylib_ext = get_dylib_extension();
        let it = fs::read_dir(dir_path);
        if it.is_err() { return Ok(rv); }
        for entry in it.unwrap() {
            let entry = entry.map_err(|e| format!("Error iterating plugin entries: {}", e))?;
            let lib_path = entry.path();
            if lib_path.is_dir() {
                continue;
            }
            if lib_path.extension().unwrap() != dylib_ext {
                continue;
            }
            let cfg_path = lib_path.with_extension("cfg");
            if !cfg_path.exists() {
                continue;
            }
            println!("> Plugin lib found: {}", lib_path.display());
            println!("> Plugin cfg found: {}", cfg_path.display());
            rv.push(Plugin::new(lib_path.into_boxed_path(), cfg_path.into_boxed_path()));
        }
        Ok(rv)
    }
}
