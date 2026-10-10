//! Pluggable database driver registry, for embedders that supply their own
//! query backend (Cloudflare D1, custom HTTP gateways, etc.).
//!
//! This sits alongside — not in place of — the built-in
//! sqlite/mysql/postgres/mssql dispatch in `builtins.rs`. The built-in path
//! reaches a backend through a connection URL; the dynamic registry here
//! reaches a backend through a **name** registered by the host before any
//! CFML executes. The dispatch in `fn_query_execute` checks the dynamic
//! registry first (keyed by the literal `datasource="..."` attribute) and
//! only falls through to the URL-based path if no dynamic driver matches.
//!
//! ## Scope
//!
//! - Sync `execute()` only. Hosts wrapping an async backend (D1) are expected
//!   to block on their own runtime — single-threaded Worker isolates can use
//!   `wasm_bindgen_futures` for this.
//! - No transactions in v1. cftransaction with a dynamic-driver datasource
//!   will surface "transactions not supported" at the call-site.

use cfml_common::dynamic::CfmlValue;
use cfml_common::vm::{CfmlError, CfmlResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// A pluggable database backend.
///
/// `params_arg` follows the same shape `fn_query_execute` already builds:
/// a `CfmlValue::Array` of plain values (parameter struct normalization has
/// already happened upstream).
///
/// `return_type` is one of `"query"`, `"array"`, `"struct:<columnKey>"`. The
/// implementation should match the existing per-driver behaviour in
/// `cfml-stdlib::builtins::execute_sqlite` and friends.
pub trait DynamicDbDriver: Send + Sync + 'static {
    fn execute(
        &self,
        sql: &str,
        params_arg: &CfmlValue,
        return_type: &str,
    ) -> CfmlResult;

    /// Open a transaction. The default refuses, which is what a driver that
    /// has not implemented transactions should do — and keeps every existing
    /// implementation compiling unchanged.
    ///
    /// `isolation` is `<cftransaction isolation="...">`, empty when unset.
    fn begin(&self, _isolation: &str) -> Result<Box<dyn DynamicTransaction>, CfmlError> {
        Err(dynamic_tx_unsupported("this datasource"))
    }

    /// Answer a `cfdbinfo` request. `None` declines and lets the engine's own
    /// metadata handling run, so a driver can answer `version`/`tables`/
    /// `columns` and ignore the rest.
    fn dbinfo(&self, _info_type: &str, _attrs: &CfmlValue) -> Option<CfmlResult> {
        None
    }
}

/// An open transaction owned by a [`DynamicDbDriver`].
///
/// Object-safe on purpose: the engine holds it as `Box<dyn DynamicTransaction>`
/// inside the same type-erased slot the bundled drivers use, which is why
/// nothing in `cfml-vm` has to know a dynamic driver exists — nesting,
/// deferred BEGIN and multi-datasource transactions all keep working.
pub trait DynamicTransaction: Send + 'static {
    fn execute(&mut self, sql: &str, params_arg: &CfmlValue, return_type: &str) -> CfmlResult;
    fn commit(&mut self) -> Result<(), CfmlError>;
    fn rollback(&mut self) -> Result<(), CfmlError>;

    /// Whether this transaction can take savepoints. `false` makes a NESTED
    /// `transaction { }` an error rather than letting it silently flatten into
    /// the outer one and commit work the inner block meant to roll back.
    fn supports_savepoints(&self) -> bool {
        false
    }
    fn savepoint(&mut self, _name: &str) -> Result<(), CfmlError> {
        Err(savepoints_unsupported())
    }
    fn release_savepoint(&mut self, _name: &str) -> Result<(), CfmlError> {
        Err(savepoints_unsupported())
    }
    fn rollback_to_savepoint(&mut self, _name: &str) -> Result<(), CfmlError> {
        Err(savepoints_unsupported())
    }
}

fn savepoints_unsupported() -> CfmlError {
    CfmlError::database(
        "cftransaction: this datasource's driver does not support savepoints, so a nested \
         transaction cannot be isolated from the one around it"
            .to_string(),
    )
}

static DYNAMIC_DRIVERS: OnceLock<Mutex<HashMap<String, Arc<dyn DynamicDbDriver>>>> =
    OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, Arc<dyn DynamicDbDriver>>> {
    DYNAMIC_DRIVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a dynamic driver under `name` (case-insensitive). Re-registering
/// the same name replaces the previous entry.
pub fn register_dynamic_datasource(name: &str, driver: Arc<dyn DynamicDbDriver>) {
    if let Ok(mut m) = registry().lock() {
        m.insert(name.to_lowercase(), driver);
    }
}

/// Remove a dynamic driver registration.
pub fn unregister_dynamic_datasource(name: &str) {
    if let Ok(mut m) = registry().lock() {
        m.remove(&name.to_lowercase());
    }
}

/// Look up a dynamic driver by name.
pub fn lookup_dynamic_datasource(name: &str) -> Option<Arc<dyn DynamicDbDriver>> {
    registry().lock().ok()?.get(&name.to_lowercase()).cloned()
}

/// Returns true if a dynamic driver is registered under `name`. Cheaper than
/// `lookup_dynamic_datasource` when the caller only needs to branch.
pub fn has_dynamic_datasource(name: &str) -> bool {
    registry()
        .lock()
        .ok()
        .map_or(false, |m| m.contains_key(&name.to_lowercase()))
}

/// Helper for cftransaction: produce a uniform "not supported" error.
pub fn dynamic_tx_unsupported(name: &str) -> CfmlError {
    // `database`-typed, not `runtime`: Lucee/ACF raise a catchable `database`
    // exception for datasource problems and application probes rely on it.
    CfmlError::database(format!(
        "cftransaction: datasource '{}' is provided by a dynamic driver that does not yet support transactions",
        name
    ))
}
