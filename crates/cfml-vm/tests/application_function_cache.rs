//! Regression coverage for Application.cfc cached bytecode functions.
//!
//! Application scope can hold long-lived CFC instances. The VM caches the
//! bytecode functions reachable from that scope so those instances remain
//! callable on later requests. When a request overwrites an app-scope CFC, the
//! cache must drop the old function bodies instead of retaining both old and
//! new copies forever.

use cfml_codegen::{compiler::CfmlCompiler, BytecodeProgram};
use cfml_common::dynamic::CfmlValue;
use cfml_common::vfs::{EmbeddedFs, Vfs};
use cfml_compiler::{parser::Parser, tag_parser};
use cfml_stdlib::builtins::{get_builtin_functions, get_builtins};
use cfml_vm::{CfmlVirtualMachine, ServerState};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::sync::Arc;

const VROOT: &str = "/app";
const APP_NAME: &str = "application-function-cache-test";

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

fn run_request(server_state: &ServerState, vfs: Arc<dyn Vfs>) {
    run_request_with_expected_output(server_state, vfs, "ok");
}

fn run_request_with_expected_output(
    server_state: &ServerState,
    vfs: Arc<dyn Vfs>,
    expected_output: &str,
) {
    let page_path = format!("{}/index.cfm", VROOT);
    let program = compile_page(&vfs, &page_path);

    let mut vm = CfmlVirtualMachine::new(program);
    vm.vfs = vfs;
    vm.source_file = Some(page_path.clone());
    vm.base_template_path = Some(page_path);
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

    vm.server_state = Some(server_state.clone());

    vm.execute_with_lifecycle().unwrap();
    assert_eq!(expected_output, vm.output_buffer.trim());
}

fn cached_function_count(server_state: &ServerState) -> usize {
    server_state
        .applications
        .get(APP_NAME)
        .expect("application state should exist")
        .cached_functions
        .len()
}

#[test]
fn overwritten_application_scope_cfc_does_not_grow_function_cache() {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    files.insert(
        "Application.cfc".to_string(),
        r##"
component {
    this.name = "application-function-cache-test";

    function onRequestStart(targetPage) {
        application.factory = createObject("component", "Factory");
    }

    function onRequest(targetPage) {
        include "#targetPage#";
    }
}
"##
        .as_bytes()
        .to_vec(),
    );
    files.insert(
        "Factory.cfc".to_string(),
        r#"
component {
    function getValue() {
        return "ok";
    }
}
"#
        .as_bytes()
        .to_vec(),
    );
    files.insert(
        "index.cfm".to_string(),
        r#"<cfoutput>#application.factory.getValue()#</cfoutput>"#
            .as_bytes()
            .to_vec(),
    );

    let vfs: Arc<dyn Vfs> = Arc::new(EmbeddedFs::new(files, VROOT.to_string()));
    let server_state = ServerState::with_production(false);

    run_request(&server_state, vfs.clone());
    let first_count = cached_function_count(&server_state);
    assert!(
        first_count > 0,
        "test fixture must cache at least one app-scope CFC function"
    );

    run_request(&server_state, vfs.clone());
    assert_eq!(
        first_count,
        cached_function_count(&server_state),
        "cached functions should not grow after the first repeated request"
    );

    run_request(&server_state, vfs);
    assert_eq!(
        first_count,
        cached_function_count(&server_state),
        "cached functions should remain stable across repeated overwrites"
    );
}

#[test]
fn persistent_and_overwritten_application_cfc_functions_are_cached_sparsely() {
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    files.insert(
        "Application.cfc".to_string(),
        r##"
component {
    this.name = "application-function-cache-test";

    function onApplicationStart() {
        application.persistentFactory = createObject("component", "Factory");
    }

    function onRequestStart(targetPage) {
        application.requestFactory = createObject("component", "RequestFactory");
    }

    function onRequest(targetPage) {
        include "#targetPage#";
    }
}
"##
        .as_bytes()
        .to_vec(),
    );
    files.insert(
        "Factory.cfc".to_string(),
        r#"
component {
    function getValue() {
        return "ok";
    }
}
"#
        .as_bytes()
        .to_vec(),
    );
    files.insert(
        "RequestFactory.cfc".to_string(),
        r#"
component {
    function getValue() {
        return "ok";
    }
}
"#
        .as_bytes()
        .to_vec(),
    );
    files.insert(
        "index.cfm".to_string(),
        r#"<cfoutput>#application.persistentFactory.getValue()##application.requestFactory.getValue()#</cfoutput>"#
            .as_bytes()
            .to_vec(),
    );

    let vfs: Arc<dyn Vfs> = Arc::new(EmbeddedFs::new(files, VROOT.to_string()));
    let server_state = ServerState::with_production(false);

    run_request_with_expected_output(&server_state, vfs.clone(), "okok");
    let first_count = cached_function_count(&server_state);
    assert!(
        first_count > 1,
        "test fixture must cache persistent and per-request app-scope CFC functions"
    );

    run_request_with_expected_output(&server_state, vfs.clone(), "okok");
    assert_eq!(
        first_count,
        cached_function_count(&server_state),
        "sparse cache must not retain stale functions between persistent and overwritten CFCs"
    );

    run_request_with_expected_output(&server_state, vfs, "okok");
    assert_eq!(
        first_count,
        cached_function_count(&server_state),
        "sparse cache should remain stable across repeated mixed app-scope CFCs"
    );
}
