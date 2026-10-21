<cfscript>
// Every recursion the engine can be driven into must end in a catchable CFML
// error, never a native stack overflow (which aborts the whole server). Each
// input here sits just over the limit it exercises so the suite stays fast.
suiteBegin("Recursion and depth limits");

// ---- 1. A template that includes itself ----
assertThrows("recursive include is a catchable error", function() {
    include "recursive_include_fixture.cfm";
});
assertThrows("recursive cfinclude is a catchable error", function() {
    include "recursive_include_fixture.cfm";
});

// ---- 2. Circular extends ----
assertThrows("component extending itself is a catchable error", function() {
    x = new core.CyclicExtendsSelf();
});
assertThrows("A <-> B extends loop is a catchable error", function() {
    x = new core.CyclicExtendsA();
});
assertThrows("createObject on a self-extending component is a catchable error", function() {
    x = createObject("component", "core.CyclicExtendsSelf");
});
// Lucee reports the loop as a bare `java.lang.StackOverflowError`; only
// RustCFML names the chain.
if (isRustCFML()) {
    try {
        x = new core.CyclicExtendsA();
    } catch (any e) {
        assertTrue("cyclic extends error names the chain", findNoCase("CyclicExtendsA", e.message) > 0 && findNoCase("CyclicExtendsB", e.message) > 0);
    }
}

// ---- 3. Equality over cyclic / very deep graphs ----
// Lucee's own Struct.equals recurses forever on two DISTINCT self-referential
// structs (a StackOverflowError), so the cyclic answers are RustCFML's.
cycA = {}; cycA.self = cycA;
cycB = {}; cycB.self = cycB;
if (isRustCFML()) {
    assert("two distinct cyclic structs are equal (bisimilar)", cycA.equals(cycB), true);
    cycC = {x: 1}; cycC.self = cycC;
    cycD = {x: 2}; cycD.self = cycD;
    assert("cyclic structs still compare their other keys", cycC.equals(cycD), false);
    assert("arrayContains over cyclic structs terminates", arrayContains([cycA], cycB), true);
    assert("arrayFind over cyclic structs terminates", arrayFind([cycA], cycB), 1);
    // Two very deep (acyclic) nests: a release build walks 50k levels and
    // answers; a debug build's larger frames hit the stack guard first and get
    // a catchable error. Neither may overflow the native stack. (Deeper than
    // this a DEBUG build aborts while DROPPING the nest — Rust's recursive
    // drop glue, nothing the comparison can guard — so the depth stays here.)
    deepX = {}; curX = deepX;
    deepY = {}; curY = deepY;
    for (i = 1; i <= 50000; i++) {
        curX.n = {}; curX = curX.n;
        curY.n = {}; curY = curY.n;
    }
    try {
        assert("comparing two 50k-deep nests answers", deepX.equals(deepY), true);
    } catch (any e) {
        assertTrue("comparing two 50k-deep nests is refused by the stack guard, catchably", findNoCase("nested too deeply", e.message) > 0);
    }
    deepX = 0; deepY = 0; curX = 0; curY = 0;
}
// Lucee throws "Can't cast Complex Object Type [Struct] to String" on `==`
// between two structs; RustCFML answers false.
if (isRustCFML()) {
    assert("== on two distinct structs is false, not a hang", cycA == cycB, false);
}

// ---- 4. structFindKey / structFindValue over a cyclic struct ----
// Lucee's own walker recurses forever here (an uncatchable
// java.lang.StackOverflowError that aborts the whole file), so the cyclic
// answers are RustCFML's; the shared-struct case below is cross-engine.
if (isRustCFML()) {
    cyc = {k: 1}; cyc.self = cyc; cyc.arr = [cyc];
    rk = structFindKey(cyc, "k", "all");
    assertTrue("structFindKey 'all' on a cyclic struct returns", arrayLen(rk) >= 1);
    assert("structFindKey 'all' on a cyclic struct finds the key once", arrayLen(rk), 1);
    rv = structFindValue(cyc, "1", "all");
    assertTrue("structFindValue 'all' on a cyclic struct returns", arrayLen(rv) >= 1);
    assert("structFindKey 'one' on a cyclic struct", arrayLen(structFindKey(cyc, "k", "one")), 1);
}
shared = {t: 1};
twice = {p: shared, q: shared};
assert("a struct shared by two keys is reported under both paths", arrayLen(structFindKey(twice, "t", "all")), 2);

// ---- 5. Query-of-queries parser nesting ----
q = queryNew("a", "integer", [[1]]);
r = queryExecute("SELECT * FROM q WHERE " & repeatString("(", 100) & "a = 1" & repeatString(")", 100), {}, {dbtype: "query"});
assert("QoQ parses 100 nested parentheses", r.recordCount, 1);
if (isRustCFML()) {
    // Lucee has no fixed cap here (its parser recurses until the JVM stack
    // gives out); RustCFML refuses past 512 levels with a parse error.
    assertThrows("QoQ with 600 nested parentheses is a catchable parse error", function() {
        queryExecute("SELECT * FROM q WHERE " & repeatString("(", 600) & "a = 1" & repeatString(")", 600), {}, {dbtype: "query"});
    });
}

// ---- 6. Long flat operator chains ----
assert("2,000-term addition chain evaluates", evaluate("1" & repeatString(" + 1", 2000)), 2001);
assert("2,000-term concatenation chain evaluates", len(evaluate("'a'" & repeatString(" & 'a'", 2000))), 2001);
assert("2,000-term OR chain evaluates", evaluate("false" & repeatString(" or false", 1999) & " or true"), true);
assert("2,000-term AND chain evaluates", evaluate("true" & repeatString(" and true", 1999) & " and false"), false);
if (isRustCFML()) {
    // Lucee compiles an expression to one JVM method and fails a chain this
    // long with "code too large"; the RustCFML limit is 20,000 operands.
    assert("20,000-term addition chain evaluates", evaluate("1" & repeatString(" + 1", 20000)), 20001);
    assert("20,000-term OR chain evaluates", evaluate("true" & repeatString(" or false", 20000)), true);
    assertThrows("20,001-term chain is a catchable parse error", function() {
        evaluate("1" & repeatString(" + 1", 20001));
    });
    try {
        evaluate("1" & repeatString(" + 1", 20001));
    } catch (any e) {
        assertTrue("chain-limit error says nesting too deep", findNoCase("expression nesting too deep", e.message) > 0);
    }
}

// ---- 7. Legitimate deep self-recursion vs runaway recursion ----
function countDown(n) {
    if (n == 0) return 0;
    return 1 + countDown(n - 1);
}
function forever(n) {
    return forever(n + 1);
}
assert("100-deep self-recursion runs", countDown(100), 100);
// A release build (and Lucee) runs 300 and 2,400 frames; a DEBUG build's
// native frames are ~300 KB each, so its 64 MB thread holds only ~180 and the
// stack guard refuses the call with a catchable error. Either way the old
// "Likely infinite recursion" heuristic, which refused `f(n-1)` at depth 256,
// must not be what stops it: only a stack guard's message is allowed (ours
// names the native stack; Lucee's is its java.lang.StackOverflowError).
try {
    assert("300-deep self-recursion runs", countDown(300), 300);
} catch (any e) {
    assertTrue("300-deep self-recursion is only ever refused by the native-stack guard", findNoCase("native stack", e.message) > 0 || findNoCase("StackOverflow", e.message) > 0);
}
try {
    assert("2,400-deep self-recursion runs", countDown(2400), 2400);
} catch (any e) {
    assertTrue("2,400-deep self-recursion is only ever refused by the native-stack guard", findNoCase("native stack", e.message) > 0 || findNoCase("StackOverflow", e.message) > 0);
}
assertThrows("runaway recursion is a catchable error", function() {
    forever(1);
});
assertThrows("runaway recursion via a closure is a catchable error", function() {
    loop = function(n) { return loop(n + 1); };
    loop(1);
});

// ---- 8. ^ is left-associative ----
assert("2^3^2 is (2^3)^2 = 64", 2^3^2, 64);
assert("2^3 is 8", 2^3, 8);
assert("2*3^2 is 18", 2*3^2, 18);
assert("2^3*2 is 16", 2^3*2, 16);
assert("2^2^2^2 is 256", 2^2^2^2, 256);

suiteEnd();
</cfscript>
