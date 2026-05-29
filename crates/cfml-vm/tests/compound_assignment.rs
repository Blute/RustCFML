//! Regression coverage for compound assignments with multi-op RHS expressions.

use cfml_codegen::{compiler::CfmlCompiler, BytecodeProgram};
use cfml_common::vfs::{EmbeddedFs, Vfs};
use cfml_compiler::{parser::Parser, tag_parser};
use cfml_stdlib::builtins::{get_builtin_functions, get_builtins};
use cfml_vm::CfmlVirtualMachine;
use std::collections::HashMap;
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

fn run_page(source: &str) -> String {
    let mut files = HashMap::new();
    files.insert("index.cfm".to_string(), source.as_bytes().to_vec());

    let vfs: Arc<dyn Vfs> = Arc::new(EmbeddedFs::new(files, VROOT.to_string()));
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

    vm.execute().unwrap();
    vm.get_output()
}

#[test]
fn concat_assignment_preserves_function_call_rhs() {
    let output = run_page(
        r#"
<cfset profileInitials = "">
<cfset part = "Mat">
<cfset profileInitials &= ucase(left(part, 1))>
<cfoutput>#profileInitials#</cfoutput>
"#,
    );

    assert_eq!("M", output.trim());
}

#[test]
fn compound_assignment_preserves_function_call_rhs_for_struct_targets() {
    let output = run_page(
        r#"
<cfset item = { count = 2, label = "A" }>
<cfset item.count += val("3")>
<cfset item.label &= ucase(left("bee", 1))>
<cfoutput>#item.count#:#item.label#</cfoutput>
"#,
    );

    assert_eq!("5:AB", output.trim());
}
