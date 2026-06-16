//! Regression coverage for Application.cfc inheritance through baseline
//! `.cfconfig.json` mappings.
//!
//! Lucee makes configured mappings available while loading Application.cfc
//! itself, so an app can use `extends="framework.ApplicationBase"` without
//! duplicating the mapping inside the Application.cfc pseudo-constructor.

use cfml_codegen::{compiler::CfmlCompiler, BytecodeProgram};
use cfml_common::dynamic::CfmlValue;
use cfml_common::vfs::{EmbeddedFs, Vfs};
use cfml_compiler::{parser::Parser, tag_parser};
use cfml_config::RustCfmlConfig;
use cfml_stdlib::builtins::{get_builtin_functions, get_builtins};
use cfml_vm::{CfmlVirtualMachine, ServerState};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

const VROOT: &str = "/app";

fn compile_page(vfs: &Arc<dyn Vfs>, path: &str) -> BytecodeProgram {
    let source = vfs.read_to_string(path).unwrap();
    let processed = if tag_parser::has_cfml_tags(&source) {
        tag_parser::tags_to_script(&source)
    } else {
        source
    };
    let ast = Parser::new(processed).parse().unwrap();
    CfmlCompiler::new().compile(ast)
}

#[test]
fn baseline_cfconfig_mapping_resolves_application_cfc_extends() {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    files.insert(
        "Application.cfc".to_string(),
        r#"
component extends="framework.ApplicationBase" {
    this.name = "cfconfig-application-extends-mapping-test";

    function onApplicationStart() {
        super.onApplicationStart();
        return true;
    }
}
"#
        .as_bytes()
        .to_vec(),
    );
    files.insert(
        "lib/framework/ApplicationBase.cfc".to_string(),
        r#"
component {
    function onApplicationStart() {
        application.fromParent = "parent-started";
        return true;
    }

    function onRequest(targetPage) {
        writeOutput(application.fromParent);
    }
}
"#
        .as_bytes()
        .to_vec(),
    );
    files.insert("index.cfm".to_string(), b"direct page".to_vec());

    let vfs: Arc<dyn Vfs> = Arc::new(EmbeddedFs::new(files, VROOT.to_string()));
    let page_path = format!("{}/index.cfm", VROOT);
    let program = compile_page(&vfs, &page_path);

    let mut cfg = RustCfmlConfig::default();
    cfg.mappings.insert(
        "/framework".to_string(),
        format!("{}/lib/framework", VROOT),
    );

    let mut server_state = ServerState::with_config(false, Arc::new(cfg));
    server_state.webroot = Some(PathBuf::from(VROOT));

    let mut vm = CfmlVirtualMachine::new(program);
    vm.vfs = vfs;
    vm.source_file = Some(page_path.clone());
    vm.base_template_path = Some(page_path);
    vm.apply_cfconfig(&server_state.cfconfig);
    vm.server_state = Some(server_state);

    for (name, value) in get_builtins() {
        vm.globals.insert(name, value);
    }
    for (name, func) in get_builtin_functions() {
        vm.builtins.insert(name, func);
    }
    vm.globals
        .entry("url".to_string())
        .or_insert_with(|| CfmlValue::strukt(IndexMap::new()));
    vm.globals
        .entry("cgi".to_string())
        .or_insert_with(|| CfmlValue::strukt(IndexMap::new()));
    vm.globals
        .entry("form".to_string())
        .or_insert_with(|| CfmlValue::strukt(IndexMap::new()));

    vm.execute_with_lifecycle().unwrap();

    assert_eq!("parent-started", vm.output_buffer.trim());
}
