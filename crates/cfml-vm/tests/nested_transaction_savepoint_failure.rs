//! A nested `transaction { }` whose savepoint cannot be taken must leave the
//! ENCLOSING transaction rollable-back.
//!
//! Every bundled driver supports savepoints, so this path only fires for a
//! driver supplied by an extension — DuckDB, for one, has no `SAVEPOINT`. It
//! went wrong in the worst way: `cftransaction_start` incremented
//! `transaction_depth` *before* issuing the savepoint, so when the savepoint
//! failed the depth stayed incremented and the outer block's rollback was
//! treated as a nested one. The outer transaction then kept work it meant to
//! discard, with no error beyond the one the inner block raised.
//!
//! The hooks are plain function pointers on the VM, so the failure is driven
//! here with stubs rather than a real database.

use std::sync::atomic::{AtomicUsize, Ordering};

use cfml_codegen::{compiler::CfmlCompiler, BytecodeProgram};
use cfml_common::dynamic::CfmlValue;
use cfml_common::vfs::{EmbeddedFs, Vfs};
use cfml_common::vm::{CfmlError, CfmlResult};
use cfml_compiler::{parser::Parser, tag_parser};
use cfml_stdlib::builtins::{get_builtin_functions, get_builtins};
use cfml_vm::CfmlVirtualMachine;
use std::collections::HashMap;
use std::sync::Arc;

const VROOT: &str = "/app";

/// What the stub driver was told to do, in order.
///
/// The stub hooks are plain `fn` pointers, so their bookkeeping has to be
/// process-global — which means the tests in this file cannot run at the same
/// time as each other. `cargo test` runs them in parallel by default, and one
/// test resetting the counters mid-way through another reads as a failure in
/// whichever lost the race. Each test takes this gate first.
static COMMITS: AtomicUsize = AtomicUsize::new(0);
static ROLLBACKS: AtomicUsize = AtomicUsize::new(0);
static GATE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Hold the gate and start from a known count.
fn begin_test() -> std::sync::MutexGuard<'static, ()> {
    let guard = GATE.lock().unwrap_or_else(|e| e.into_inner());
    COMMITS.store(0, Ordering::SeqCst);
    ROLLBACKS.store(0, Ordering::SeqCst);
    guard
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

fn begin(_ds: &str) -> Result<Box<dyn std::any::Any>, CfmlError> {
    Ok(Box::new(0u8))
}
fn commit(_c: &mut Box<dyn std::any::Any>) -> Result<(), CfmlError> {
    COMMITS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}
fn rollback(_c: &mut Box<dyn std::any::Any>) -> Result<(), CfmlError> {
    ROLLBACKS.fetch_add(1, Ordering::SeqCst);
    Ok(())
}
/// The driver has no savepoints — exactly what DuckDB reports.
fn savepoint(_c: &mut Box<dyn std::any::Any>, _n: &str) -> Result<(), CfmlError> {
    Err(CfmlError::database(
        "this driver does not support savepoints".to_string(),
    ))
}
fn exec(
    _c: &mut Box<dyn std::any::Any>,
    _sql: &str,
    _p: &CfmlValue,
    _rt: &str,
) -> CfmlResult {
    Ok(CfmlValue::Null)
}

#[test]
fn a_failed_nested_savepoint_still_rolls_the_outer_transaction_back() {
    let _gate = begin_test();

    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r##"
<cfscript>
caught = "";
try {
    transaction {
        queryExecute( "insert into t values (1)", [], { datasource = "stub" } );
        transaction {
            queryExecute( "insert into t values (2)", [], { datasource = "stub" } );
        }
    }
} catch ( any e ) {
    caught = e.message;
}
writeOutput( caught );
</cfscript>
"##
        .as_bytes()
        .to_vec(),
    );

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
    vm.txn_begin = Some(begin);
    vm.txn_commit = Some(commit);
    vm.txn_rollback = Some(rollback);
    vm.txn_savepoint = Some(savepoint);
    vm.txn_execute = Some(exec);

    vm.execute().unwrap();

    assert!(
        vm.get_output().contains("savepoint"),
        "the inner block should report the savepoint refusal, got: {}",
        vm.get_output()
    );
    assert_eq!(
        1,
        ROLLBACKS.load(Ordering::SeqCst),
        "the OUTER transaction must be rolled back exactly once"
    );
    assert_eq!(
        0,
        COMMITS.load(Ordering::SeqCst),
        "nothing may be committed when the nested block failed"
    );
}

#[test]
fn nesting_before_the_outer_block_has_a_connection_is_refused_too() {
    // Lucee/ACF defer BEGIN to the first statement, so an outer block that has
    // not run one yet has NO connection — and the nested block could not take
    // its savepoint even to find out whether the driver has them. It recorded
    // "no savepoint" and carried on, so on a savepoint-less driver the nesting
    // was silently allowed and the inner rollback became a no-op: the inner
    // block kept the work it asked to discard.
    //
    // The same shape with one statement ahead of it (the test above) was
    // correctly refused the whole time, which is exactly why this went unseen.
    let _gate = begin_test();

    let mut files = HashMap::new();
    files.insert(
        "index.cfm".to_string(),
        r##"
<cfscript>
caught = "";
try {
    transaction {
        // No statement here, so the transaction is still PENDING.
        transaction {
            queryExecute( "insert into t values (2)", [], { datasource = "stub" } );
        }
    }
} catch ( any e ) {
    caught = e.message;
}
writeOutput( caught );
</cfscript>
"##
        .as_bytes()
        .to_vec(),
    );

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
    vm.txn_begin = Some(begin);
    vm.txn_commit = Some(commit);
    vm.txn_rollback = Some(rollback);
    vm.txn_savepoint = Some(savepoint);
    vm.txn_execute = Some(exec);

    vm.execute().unwrap();

    assert!(
        vm.get_output().contains("savepoint"),
        "a nested block must report the savepoint refusal even when the outer \
         transaction had not begun yet, got: {}",
        vm.get_output()
    );
    assert_eq!(
        0,
        COMMITS.load(Ordering::SeqCst),
        "nothing may be committed when the nested block failed"
    );
}
