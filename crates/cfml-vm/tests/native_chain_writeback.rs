//! GH #492: a method chain ending in a `set`-prefixed call (`settle()`) must not
//! write its result back over a root variable holding a native object.

use cfml_codegen::{compiler::CfmlCompiler, BytecodeProgram};
use cfml_common::dynamic::{CfmlNative, CfmlValue};
use cfml_common::vfs::{EmbeddedFs, Vfs};
use cfml_common::vm::{CfmlError, CfmlResult};
use cfml_compiler::{parser::Parser, tag_parser};
use cfml_stdlib::builtins::{get_builtin_functions, get_builtins};
use cfml_vm::CfmlVirtualMachine;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

const VROOT: &str = "/app";

#[derive(Debug)]
struct Browser;

#[derive(Debug)]
struct Page;

fn native(obj: impl CfmlNative + 'static) -> CfmlValue {
    CfmlValue::NativeObject(Arc::new(RwLock::new(obj)))
}

fn unknown(class: &str, name: &str) -> CfmlResult {
    Err(CfmlError::runtime(format!(
        "{} has no method [{}]",
        class,
        name.to_lowercase()
    )))
}

impl CfmlNative for Browser {
    fn class_name(&self) -> &str {
        "Browser"
    }
    fn call_method(&mut self, name: &str, _args: Vec<CfmlValue>) -> CfmlResult {
        match name.to_lowercase().as_str() {
            "newpage" => Ok(native(Page)),
            // A `set*` name returning something that is NOT the receiver.
            "settings" => Ok(CfmlValue::string("cfg")),
            _ => unknown("Browser", name),
        }
    }
}

impl CfmlNative for Page {
    fn class_name(&self) -> &str {
        "Page"
    }
    fn call_method(&mut self, name: &str, _args: Vec<CfmlValue>) -> CfmlResult {
        match name.to_lowercase().as_str() {
            "settle" | "goto" => Ok(CfmlValue::NativeObject(Arc::new(RwLock::new(Page)))),
            "kind" => Ok(CfmlValue::string("page")),
            _ => unknown("Page", name),
        }
    }
}

fn browser_new(_args: Vec<CfmlValue>) -> CfmlResult {
    Ok(native(Browser))
}

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

fn run(src: &str) -> String {
    let mut files = HashMap::new();
    files.insert("index.cfm".to_string(), src.as_bytes().to_vec());
    let vfs: Arc<dyn Vfs> = Arc::new(EmbeddedFs::new(files, VROOT.to_string()));
    let page_path = format!("{}/index.cfm", VROOT);
    let program = compile_page(&vfs, &page_path);

    let mut vm = CfmlVirtualMachine::new(program);
    vm.vfs = vfs;
    vm.source_file = Some(Arc::from(page_path.as_str()));
    vm.base_template_path = Some(page_path);
    for (name, value) in get_builtins() {
        vm.globals.insert(name, value);
    }
    for (name, func) in get_builtin_functions() {
        vm.builtins.insert(name, func);
    }
    vm.register_native_class("Browser", browser_new);
    vm.execute().unwrap();
    vm.get_output().trim().to_string()
}

#[test]
fn set_prefixed_chain_tail_keeps_native_root() {
    let out = run(r#"<cfscript>
        br = createObject("rust", "Browser");
        p = br.newPage().settle(100);
        q = br.newPage().goto("u").settle(100);
        s = br.settings();
        writeOutput(br.newPage().kind() & "|" & p.kind() & "|" & q.kind() & "|" & s);
    </cfscript>"#);
    assert_eq!(out, "page|page|page|cfg");
}

#[test]
fn set_prefixed_chain_tail_keeps_deep_native_root() {
    let out = run(r#"<cfscript>
        holder = { br: createObject("rust", "Browser") };
        p = holder.br.newPage().settle(100);
        writeOutput(holder.br.newPage().kind() & "|" & p.kind());
    </cfscript>"#);
    assert_eq!(out, "page|page");
}
