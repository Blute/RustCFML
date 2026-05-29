//! Custom tag lifecycle behavior.

use cfml_codegen::{compiler::CfmlCompiler, BytecodeProgram};
use cfml_common::dynamic::CfmlValue;
use cfml_common::vfs::{EmbeddedFs, Vfs};
use cfml_compiler::{parser::Parser, tag_parser};
use cfml_stdlib::builtins::{get_builtin_functions, get_builtins};
use cfml_vm::CfmlVirtualMachine;
use indexmap::IndexMap;
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

fn run_page(files: HashMap<String, Vec<u8>>) -> String {
    let vfs: Arc<dyn Vfs> = Arc::new(EmbeddedFs::new(files, VROOT.to_string()));
    let page_path = format!("{}/index.cfm", VROOT);
    let program = compile_page(&vfs, &page_path);

    let mut vm = CfmlVirtualMachine::new(program);
    vm.vfs = vfs;
    vm.source_file = Some(page_path.clone());
    vm.base_template_path = Some(page_path);
    vm.custom_tag_paths = vec![format!("{}/tags", VROOT)];
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

    vm.execute().unwrap();
    vm.get_output()
}

#[test]
fn self_closing_custom_tag_runs_end_phase_with_start_locals() {
    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r#"<cf_capture value="ok" returnContentVariable="result" /><cfoutput>#result#</cfoutput>"#
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "tags/capture.cfm".to_string(),
        r#"
<cfif thisTag.executionMode EQ "start">
    <cfset savedValue = attributes.value />
</cfif>

<cfif thisTag.executionMode EQ "end">
    <cfset caller[attributes.returnContentVariable] = savedValue />
</cfif>
"#
        .as_bytes()
        .to_vec(),
    );

    assert_eq!("ok", run_page(files).trim());
}

#[test]
fn self_closing_custom_tag_reports_has_end_tag() {
    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r#"<cf_requires_end returnContentVariable="result" /><cfoutput>#result#</cfoutput>"#
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "tags/requires_end.cfm".to_string(),
        r#"
<cfif not thisTag.HasEndTag>
    <cfabort showerror="Must have an end tag..." />
</cfif>

<cfif thisTag.executionMode EQ "end">
    <cfset caller[attributes.returnContentVariable] = thisTag.HasEndTag />
</cfif>
"#
        .as_bytes()
        .to_vec(),
    );

    assert_eq!("true", run_page(files).trim());
}

#[test]
fn body_custom_tag_end_phase_keeps_start_locals() {
    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r#"<cf_wrap returnContentVariable="result">body</cf_wrap><cfoutput>#result#</cfoutput>"#
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "tags/wrap.cfm".to_string(),
        r#"
<cfif thisTag.executionMode EQ "start">
    <cfset prefix = "start:" />
</cfif>

<cfif thisTag.executionMode EQ "end">
    <cfset caller[attributes.returnContentVariable] = prefix & thisTag.generatedContent />
    <cfset thisTag.generatedContent = "" />
</cfif>
"#
        .as_bytes()
        .to_vec(),
    );

    assert_eq!("start:body", run_page(files).trim());
}

#[test]
fn body_custom_tag_generated_content_precedes_end_phase_output() {
    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r#"<cf_layout><cfoutput>body</cfoutput></cf_layout>"#
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "tags/layout.cfm".to_string(),
        r#"
<cfif thisTag.executionMode EQ "start">
    <cfoutput><html><body></cfoutput>
<cfelse>
    <cfoutput></body></html></cfoutput>
</cfif>
"#
        .as_bytes()
        .to_vec(),
    );

    let output = run_page(files);
    let body_start = output.find("<body>").unwrap();
    let body_content = output.find("body").unwrap();
    let body_end = output.find("</body>").unwrap();

    assert!(body_start < body_content);
    assert!(body_content < body_end);
}

#[test]
fn custom_tag_template_has_local_scope() {
    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r#"<cf_localtest returnContentVariable="result" /><cfoutput>#result#</cfoutput>"#
            .as_bytes()
            .to_vec(),
    );
    files.insert(
        "tags/localtest.cfm".to_string(),
        r#"
<cfif thisTag.executionMode EQ "start">
    <cfset local.payload = {message = "ok"} />
    <cfset caller[attributes.returnContentVariable] = local.payload.message />
</cfif>
"#
        .as_bytes()
        .to_vec(),
    );

    assert_eq!("ok", run_page(files).trim());
}
