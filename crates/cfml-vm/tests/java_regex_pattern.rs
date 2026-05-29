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

#[test]
fn java_regex_pattern_shim_supports_dynamic_route_matching() {
    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r##"
<cfset oRegex = createObject("java", "java.util.regex.Pattern") />
<cfset matcher = oRegex
    .compile(javaCast("string", "^\/sysadmin\/routes\/([0-9A-Za-z\s\-_]+)$"))
    .matcher(javaCast("string", "/sysadmin/routes/4e4727bd-f93d-40e6-9409-bed317fe76df")) />
<cfif matcher.find()>
    <cfoutput>#matcher.group(1)#|#matcher.groupCount()#</cfoutput>
<cfelse>
    <cfoutput>no-match</cfoutput>
</cfif>
"##
        .as_bytes()
        .to_vec(),
    );

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
    assert_eq!(
        "4e4727bd-f93d-40e6-9409-bed317fe76df|1",
        vm.get_output().split_whitespace().collect::<String>()
    );
}
