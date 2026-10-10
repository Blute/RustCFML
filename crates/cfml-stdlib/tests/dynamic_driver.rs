//! A datasource supplied by a driver rather than by a bundled engine.
//!
//! This is the half of the extension-datasource path that needs no extension:
//! once a `DynamicDbDriver` is in the registry, `queryExecute`, `cftransaction`
//! and `cfdbinfo` must reach it exactly as they reach sqlite/mysql/postgres/
//! mssql. The C-ABI half — getting a `.rcx`'s driver into that registry — is
//! covered by `cfml-vm`'s `foreign_abi` tests.
//!
//! No database and no network: the driver here answers from memory, which is
//! the point. Every assertion is about the engine's routing, not about SQL.

use std::sync::{Arc, Mutex};

use cfml_common::dynamic::{CfmlValue, ValueMap};
use cfml_common::vm::{CfmlError, CfmlResult};
use cfml_stdlib::{DynamicDbDriver, DynamicTransaction};

/// Records what it was asked to do, so a test can assert the engine routed a
/// statement here rather than to a SQLite file of the same name.
#[derive(Default)]
struct Log {
    statements: Vec<String>,
    committed: bool,
    rolled_back: bool,
}

struct FakeDriver {
    log: Arc<Mutex<Log>>,
    /// Whether `begin()` works, so one test can cover a driver that has no
    /// transactions without needing a second type.
    transactional: bool,
}

struct FakeTxn {
    log: Arc<Mutex<Log>>,
}

fn one_row_query(column: &str, value: &str) -> CfmlResult {
    cfml_stdlib::build_query_result_columnar(
        vec![column.to_string()],
        vec![vec![CfmlValue::string(value.to_string())]],
        "",
        "query",
    )
}

impl DynamicDbDriver for FakeDriver {
    fn execute(&self, sql: &str, _params: &CfmlValue, _return_type: &str) -> CfmlResult {
        self.log.lock().unwrap().statements.push(sql.to_string());
        one_row_query("answered_by", "fake")
    }

    fn begin(&self, _isolation: &str) -> Result<Box<dyn DynamicTransaction>, CfmlError> {
        if !self.transactional {
            return Err(CfmlError::database("no transactions here".to_string()));
        }
        Ok(Box::new(FakeTxn { log: Arc::clone(&self.log) }))
    }

    fn dbinfo(&self, info_type: &str, _attrs: &CfmlValue) -> Option<CfmlResult> {
        // Answers `version` only — the subset a driver is expected to start
        // from — and declines everything else.
        if info_type.eq_ignore_ascii_case("version") {
            Some(one_row_query("database_productname", "FakeDB"))
        } else {
            None
        }
    }
}

impl DynamicTransaction for FakeTxn {
    fn execute(&mut self, sql: &str, _params: &CfmlValue, _return_type: &str) -> CfmlResult {
        self.log.lock().unwrap().statements.push(format!("txn:{}", sql));
        one_row_query("answered_by", "fake-txn")
    }
    fn commit(&mut self) -> Result<(), CfmlError> {
        self.log.lock().unwrap().committed = true;
        Ok(())
    }
    fn rollback(&mut self) -> Result<(), CfmlError> {
        self.log.lock().unwrap().rolled_back = true;
        Ok(())
    }
}

/// The registry is process-global with no reset and test threads share a
/// process, so every test uses its own datasource name.
fn register(name: &str, transactional: bool) -> Arc<Mutex<Log>> {
    let log = Arc::new(Mutex::new(Log::default()));
    cfml_stdlib::register_dynamic_datasource(
        name,
        Arc::new(FakeDriver { log: Arc::clone(&log), transactional }),
    );
    log
}

fn opts(datasource: &str) -> CfmlValue {
    let mut m = ValueMap::default();
    m.insert("datasource".to_string(), CfmlValue::string(datasource.to_string()));
    CfmlValue::strukt(m)
}

fn query_via(datasource: &str, sql: &str) -> CfmlResult {
    cfml_stdlib::fn_query_execute(vec![
        CfmlValue::string(sql.to_string()),
        CfmlValue::Array(cfml_common::dynamic::CfmlArray::empty()),
        opts(datasource),
    ])
}

#[test]
fn a_query_reaches_the_driver_rather_than_a_sqlite_file_of_that_name() {
    let log = register("fakeds_query", false);
    let r = query_via("fakeds_query", "select 1").expect("the driver should answer");
    let q = match r {
        CfmlValue::Query(q) => q,
        other => panic!("expected a query, got {:?}", other),
    };
    assert_eq!(q.column_values_ci("answered_by").unwrap()[0].as_string(), "fake");
    assert_eq!(log.lock().unwrap().statements, vec!["select 1".to_string()]);
    cfml_stdlib::unregister_dynamic_datasource("fakeds_query");
}

#[test]
fn a_transaction_routes_its_statements_to_the_driver_and_commits() {
    let log = register("fakeds_txn", true);

    let mut conn = cfml_stdlib::builtins::txn_begin_boxed("fakeds_txn").expect("begin");
    cfml_stdlib::builtins::txn_execute_boxed(
        &mut conn,
        "insert into t values (1)",
        &CfmlValue::Array(cfml_common::dynamic::CfmlArray::empty()),
        "query",
    )
    .expect("statement inside the transaction");
    cfml_stdlib::builtins::txn_commit_boxed(&mut conn).expect("commit");

    let l = log.lock().unwrap();
    assert_eq!(l.statements, vec!["txn:insert into t values (1)".to_string()]);
    assert!(l.committed, "the engine must have committed through the driver");
    drop(l);
    cfml_stdlib::unregister_dynamic_datasource("fakeds_txn");
}

#[test]
fn a_transaction_rolls_back_through_the_driver() {
    let log = register("fakeds_rollback", true);
    let mut conn = cfml_stdlib::builtins::txn_begin_boxed("fakeds_rollback").expect("begin");
    cfml_stdlib::builtins::txn_rollback_boxed(&mut conn).expect("rollback");
    assert!(log.lock().unwrap().rolled_back);
    cfml_stdlib::unregister_dynamic_datasource("fakeds_rollback");
}

#[test]
fn a_driver_without_transactions_says_so_instead_of_running_unprotected() {
    register("fakeds_notxn", false);
    let e = cfml_stdlib::builtins::txn_begin_boxed("fakeds_notxn")
        .expect_err("this driver has no transactions, so begin must fail");
    // `database`-typed, not a generic runtime error: Lucee/ACF raise a
    // catchable `database` exception for datasource problems, and application
    // probes (Preside's `_dsnExists()`) branch on exactly that.
    assert!(
        matches!(&e.error_type, cfml_common::vm::CfmlErrorType::Custom(t) if t.eq_ignore_ascii_case("database")),
        "expected a database-typed error, got {:?}",
        e.error_type
    );
    cfml_stdlib::unregister_dynamic_datasource("fakeds_notxn");
}

#[test]
fn dbinfo_asks_the_driver_first_and_falls_back_when_it_declines() {
    register("fakeds_dbinfo", false);

    let mut attrs = ValueMap::default();
    attrs.insert("type".to_string(), CfmlValue::string("version".to_string()));
    attrs.insert("datasource".to_string(), CfmlValue::string("fakeds_dbinfo".to_string()));
    let r = cfml_stdlib::dbinfo::fn_dbinfo_impl(vec![CfmlValue::strukt(attrs)])
        .expect("the driver answers type=version");
    match r {
        CfmlValue::Query(q) => assert_eq!(
            q.column_values_ci("database_productname").unwrap()[0].as_string(),
            "FakeDB"
        ),
        other => panic!("expected a query, got {:?}", other),
    }

    // A type the driver declines must NOT be silently answered with SQLite
    // pragma SQL aimed at a foreign database, which is what happened before
    // the driver was consulted at all.
    let mut attrs = ValueMap::default();
    attrs.insert("type".to_string(), CfmlValue::string("tables".to_string()));
    attrs.insert("datasource".to_string(), CfmlValue::string("fakeds_dbinfo".to_string()));
    let e = cfml_stdlib::dbinfo::fn_dbinfo_impl(vec![CfmlValue::strukt(attrs)])
        .expect_err("a declined type must be reported, not guessed at");
    assert!(e.message.contains("does not answer type"), "got: {}", e.message);

    cfml_stdlib::unregister_dynamic_datasource("fakeds_dbinfo");
}
