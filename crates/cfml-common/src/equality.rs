//! Deep CFML value equality, shared by the stdlib (`arrayFind`,
//! `arrayContains`, …) and the VM's member-function dispatch (`indexOf`,
//! `lastIndexOf`) so the two cannot drift. It lives here rather than in
//! `cfml-stdlib` because the VM does not depend on that crate.

use std::collections::HashSet;

use crate::dynamic::CfmlValue;
use crate::stack_guard;
use crate::vm::CfmlError;

/// Cycle and depth bookkeeping for a recursive equality walk over two value
/// graphs. Every walker that descends into a pair of containers (this
/// module's [`deep_equal`], the VM's Java-style `.equals()`) calls
/// [`DeepEqGuard::enter`] with the two backing pointers first.
///
/// Identical backing handles are equal without a walk. Beyond that, a pair of
/// DISTINCT containers that is met a second time on the walk is already being
/// (or has been) compared, so it is taken as equal: two separate
/// self-referential graphs (`a.self = a; b.self = b`) are bisimilar, and
/// without this the walk followed the two cycles in lock-step until the
/// native stack overflowed — an abort of the whole server, not an error. A
/// pair that proved UNEQUAL never needs re-visiting: that `false` already
/// ended the whole comparison.
///
/// Depth is bounded by the thread's actual native stack rather than a count:
/// a non-cyclic nest hundreds of thousands of levels deep cannot be compared
/// on any build, and the walker gets a catchable error instead of an abort.
#[derive(Default)]
pub struct DeepEqGuard {
    seen: HashSet<(usize, usize)>,
}

impl DeepEqGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that the containers at `a` and `b` are about to be compared.
    /// `Ok(true)` means the pair is settled (same handle, or already on the
    /// walk) and the caller answers "equal" without descending; `Ok(false)`
    /// means descend. `Err` when the native stack is nearly exhausted.
    pub fn enter(&mut self, a: usize, b: usize) -> Result<bool, CfmlError> {
        if a == b {
            return Ok(true);
        }
        if !stack_guard::has_headroom(stack_guard::WALK_RED_ZONE) {
            return Err(CfmlError::runtime(
                "Cannot compare the values: they are nested too deeply (the native stack is exhausted)"
                    .to_string(),
            ));
        }
        Ok(!self.seen.insert((a, b)))
    }
}

/// Structural equality with CFML semantics: structs and arrays compare by
/// contents (cycle-safe, see [`DeepEqGuard`]), queries by columns and cells,
/// component instances by reference, and everything else by string form.
/// `Err` only when the two values are nested too deeply for the native stack.
pub fn deep_equal(a: &CfmlValue, b: &CfmlValue, nocase: bool) -> Result<bool, CfmlError> {
    let mut guard = DeepEqGuard::new();
    deep_equal_with(a, b, nocase, &mut guard)
}

/// [`deep_equal`] continuing an enclosing walk's bookkeeping.
pub fn deep_equal_with(
    a: &CfmlValue,
    b: &CfmlValue,
    nocase: bool,
    guard: &mut DeepEqGuard,
) -> Result<bool, CfmlError> {
    match (a, b) {
        // Two strings compare BORROWED. They would otherwise fall to the
        // `as_string()` arm at the bottom, which allocates a fresh `String` for
        // each side on every comparison — so a scan like
        // `arrayFindNoCase( widgets, name )` over N strings allocated 2N of
        // them before it could answer. Same semantics as that arm, which also
        // uses `eq_ignore_ascii_case` for the nocase form.
        (CfmlValue::String(x), CfmlValue::String(y)) => Ok(if nocase {
            x.as_str().eq_ignore_ascii_case(y.as_str())
        } else {
            x.as_str() == y.as_str()
        }),
        (CfmlValue::Struct(sa), CfmlValue::Struct(sb)) => {
            // Identity short-circuit: two references to the SAME backing handle
            // are equal without walking their contents. This is both correct (a
            // value equals itself) and essential for cycle safety — a
            // self-referential struct graph (e.g. a Wheels model with a circular
            // association: `profile.author = author; author.profile = profile`)
            // would otherwise recurse forever here. Lucee compares CFC instances
            // by reference, so `arrayContains(visited, obj)` detects an
            // already-seen object by identity, which is exactly how Wheels'
            // `allErrors(includeAssociations=true)` breaks the cycle. (Wheels
            // model.errorsSpec "handles circular reference" stack-overflowed the
            // whole TestBox suite without this.) Two DISTINCT cyclic graphs are
            // the guard's job.
            if guard.enter(sa.backing_ptr(), sb.backing_ptr())? {
                return Ok(true);
            }
            if sa.len() != sb.len() {
                return Ok(false);
            }
            for (k, va) in sa.iter() {
                match sb.get_ci(&k) {
                    Some(vb) => {
                        if !deep_equal_with(&va, &vb, nocase, guard)? {
                            return Ok(false);
                        }
                    }
                    None => return Ok(false),
                }
            }
            Ok(true)
        }
        (CfmlValue::Array(aa), CfmlValue::Array(ab)) => {
            // Same identity short-circuit as Struct (above) — same backing
            // handle is equal without recursing, so a cyclic array graph is safe.
            if guard.enter(aa.backing_ptr(), ab.backing_ptr())? {
                return Ok(true);
            }
            let sa = aa.snapshot();
            let sb = ab.snapshot();
            if sa.len() != sb.len() {
                return Ok(false);
            }
            for (x, y) in sa.iter().zip(sb.iter()) {
                if !deep_equal_with(x, y, nocase, guard)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        // Queries compare STRUCTURALLY: same columns (case-insensitively, in
        // order) and the same cell values. Falling through to the `as_string()`
        // arm below made every query equal to every other one — every query
        // stringifies the same way — so `arrayFind(arr, someQuery)` matched the
        // first query in the array whatever it held, and Preside's
        // `getFolderAncestors()` cycle guard fired on the first parent (GH #473).
        (CfmlValue::Query(qa), CfmlValue::Query(qb)) => {
            if guard.enter(qa.backing_ptr(), qb.backing_ptr())? {
                return Ok(true);
            }
            qa.with_read(|da| {
                qb.with_read(|db| {
                    if da.columns.len() != db.columns.len()
                        || da.row_count() != db.row_count()
                    {
                        return Ok(false);
                    }
                    if !da
                        .columns
                        .iter()
                        .zip(db.columns.iter())
                        .all(|(x, y)| x.eq_ignore_ascii_case(y))
                    {
                        return Ok(false);
                    }
                    for ci in 0..da.columns.len() {
                        for ri in 0..da.row_count() {
                            if !deep_equal_with(&da.data[ci][ri], &db.data[ci][ri], nocase, guard)? {
                                return Ok(false);
                            }
                        }
                    }
                    Ok(true)
                })
            })
        }
        // A complex value never equals a scalar (or a struct-vs-array mismatch).
        (CfmlValue::Struct(_), _) | (_, CfmlValue::Struct(_)) => Ok(false),
        (CfmlValue::Array(_), _) | (_, CfmlValue::Array(_)) => Ok(false),
        (CfmlValue::Query(_), _) | (_, CfmlValue::Query(_)) => Ok(false),
        // Flyweight component instances compare by REFERENCE (Lucee/ACF parity —
        // same as the marker backing-ptr identity above). Without this an Instance
        // fell to the `_ => as_string()` arm below, where EVERY component
        // stringifies to "<Component>" → any two components compared EQUAL, so
        // `arrayContains(seen, obj)` matched the first component of any class
        // (breaking Wheels' circular-association cycle detection).
        _ if a.as_component().is_some_and(|c| c.is_instance_backed())
            || b.as_component().is_some_and(|c| c.is_instance_backed()) =>
        {
            Ok(crate::component::same_component_instance(a, b))
        }
        _ => Ok(if nocase {
            a.as_string().eq_ignore_ascii_case(&b.as_string())
        } else {
            a.as_string() == b.as_string()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dynamic::ValueMap;

    fn cyclic() -> CfmlValue {
        let s = CfmlValue::strukt(ValueMap::default());
        if let CfmlValue::Struct(inner) = &s {
            inner.insert("self", s.clone());
        }
        s
    }

    #[test]
    fn two_distinct_cyclic_structs_are_equal_not_a_stack_overflow() {
        assert!(deep_equal(&cyclic(), &cyclic(), false).unwrap());
    }

    #[test]
    fn cycles_still_compare_their_other_keys() {
        let a = cyclic();
        let b = cyclic();
        if let CfmlValue::Struct(s) = &b {
            s.insert("extra", CfmlValue::Int(1));
        }
        assert!(!deep_equal(&a, &b, false).unwrap());
    }
}
