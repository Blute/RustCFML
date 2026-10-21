<cfscript>
// Deeply nested UNTRUSTED input must be a catchable CFML error, never a native
// stack overflow (an abort() of the whole process — one request took the
// multi-threaded server down). Limits: JSON 512 container levels
// (JSON_MAX_DEPTH), XML 1024 element levels (XML_MAX_DEPTH); CFML recursion and
// the recursive value walkers (serializeJSON, duplicate, writeDump) stop on a
// native-stack headroom check (cfml_common::stack_guard).
//
// RustCFML-only: Lucee has no fixed depth limit (its JSON/XML parsers run until
// the JVM stack overflows, far past these sizes), so the "just over the limit"
// inputs below do not throw there.
suiteBegin("Nesting limits: deep JSON / XML / recursion are errors, not aborts");

function nested(required string open, required string close, required numeric depth) {
    return repeatString(arguments.open, arguments.depth) & repeatString(arguments.close, arguments.depth);
}

// ---- deserializeJSON ----
ok512 = deserializeJSON(nested("[", "]", 512));
assertTrue("deserializeJSON accepts 512 nested arrays", isArray(ok512));
assertThrows("deserializeJSON rejects 513 nested arrays", function() {
    deserializeJSON(nested("[", "]", 513));
});
assertThrows("deserializeJSON rejects 513 nested objects", function() {
    deserializeJSON(repeatString('{"a":', 513) & "1" & repeatString("}", 513));
});
try {
    deserializeJSON(nested("[", "]", 513));
    assertTrue("deep JSON error names the limit", false);
} catch (any e) {
    assertTrue("deep JSON error names the limit", findNoCase("nesting too deep", e.message) > 0);
}
assertThrows("deserializeJSON rejects 20000 nested arrays", function() {
    deserializeJSON(nested("[", "]", 20000));
});
// mixed nesting over the limit (object inside array inside object …)
assertThrows("deserializeJSON rejects 600 mixed nested levels", function() {
    deserializeJSON(repeatString('[{"k":', 300) & "1" & repeatString("}]", 300));
});
// over the limit but the lenient fallback syntax (single quotes / comments)
assertThrows("lenient JSON fallback is bounded too", function() {
    deserializeJSON("/* c */ " & repeatString("{'a':", 513) & "1" & repeatString("}", 513));
});

// ---- isJSON ----
assertTrue("isJSON true at 512 nested arrays", isJSON(nested("[", "]", 512)));
assertFalse("isJSON false at 513 nested arrays", isJSON(nested("[", "]", 513)));
assertFalse("isJSON false at 20000 nested arrays", isJSON(nested("[", "]", 20000)));

// ---- xmlParse / isXML ----
x1024 = xmlParse(nested("<a>", "</a>", 1024));
assertTrue("xmlParse accepts 1024 nested elements", isXmlDoc(x1024));
assertThrows("xmlParse rejects 1025 nested elements", function() {
    xmlParse(nested("<a>", "</a>", 1025));
});
try {
    xmlParse(nested("<a>", "</a>", 1025));
    assertTrue("deep XML error names the limit", false);
} catch (any e) {
    assertTrue("deep XML error names the limit", findNoCase("nesting too deep", e.message) > 0);
}
assertThrows("xmlParse rejects 20000 nested elements", function() {
    xmlParse(nested("<a>", "</a>", 20000));
});
assertTrue("isXML true at 1024 nested elements", isXML(nested("<a>", "</a>", 1024)));
assertFalse("isXML false at 1025 nested elements", isXML(nested("<a>", "</a>", 1025)));
assertFalse("isXML false at 20000 nested elements", isXML(nested("<a>", "</a>", 20000)));

// Walking a document at the limit must work (toString / xmlSearch recurse per level).
assertTrue("toString(xml) at the depth limit", len(toString(x1024)) > 1024 * 7);
assertTrue("xmlSearch at the depth limit", arrayLen(xmlSearch(x1024, "//a")) == 1024);

// ---- a deep value BUILT in CFML (not parsed): the walkers must not abort ----
// Iterative construction has no recursion, so this nests far past anything a
// parser accepts. serializeJSON / duplicate either succeed (release build: the
// frames are small) or throw a CFML error (debug build) — never abort.
deep = []; cur = deep;
for (i = 1; i <= 60000; i++) { n = []; arrayAppend(cur, n); cur = n; }
serializeOutcome = "";
try { serializeJSON(deep); serializeOutcome = "ok"; } catch (any e) { serializeOutcome = "threw: " & e.message; }
assertTrue("serializeJSON of a 60000-deep array does not abort (" & serializeOutcome & ")",
    serializeOutcome == "ok" || findNoCase("nesting too deep", serializeOutcome) > 0);
duplicateOutcome = "";
try { duplicate(deep); duplicateOutcome = "ok"; } catch (any e) { duplicateOutcome = "threw: " & e.message; }
assertTrue("duplicate of a 60000-deep array does not abort (" & duplicateOutcome & ")",
    duplicateOutcome == "ok" || findNoCase("nesting too deep", duplicateOutcome) > 0);
deep = ""; cur = "";

// ---- runaway CFML recursion ----
function noBaseCase(n) { return noBaseCase(n + 1); }
assertThrows("unbounded recursion is a CFML error", function() { noBaseCase(1); });
// Mutual recursion through a closure (not caught by the same-name cycle check).
function viaClosure(n) {
    var a = [n];
    return arrayMap(a, function(v) { return viaClosure(v + 1); });
}
assertThrows("unbounded recursion through a closure is a CFML error", function() { viaClosure(1); });

// ---- parser: deeply nested expression is a parse error, not an abort ----
assertThrows("50000-deep parenthesised expression is a parse error", function() {
    evaluate(repeatString("(", 50000) & "1" & repeatString(")", 50000));
});

suiteEnd();
</cfscript>
