<cfscript>
suiteBegin("Core: engine temporaries stay frame-local, never in a component's variables (GH ##493)");

// ============================================================
// Background (GH #493)
// ============================================================
// Many tag/script lowerings keep hidden state in a synthesised variable: the
// value a `switch` dispatches on, a file loop's handle, cfmail/cfstoredproc/
// cfinvoke accumulators their child tags append to, the capture of a
// `savecontent` with no `variable=`, the hoisted array/list/collection/query
// of a script `loop`. Stored unscoped and undeclared, those names landed in
// the component's `variables` scope. On an instance shared between requests
// (application.service = new Service()) every concurrent call then read and
// wrote the SAME temporary: a `switch` dispatched on another request's value
// (thousands of wrong cases per 15k calls under 6 parallel requests), and
// two file loops would have shared one handle. The race needs parallel
// requests; what this suite pins is its cause: no `__`-prefixed name may
// reach `variables` after any of these constructs runs. `__static` is
// RustCFML's own static-scope slot and is excluded. A `savecontent` with no
// `variable=` is a Lucee PARSE error, so that case lives in the
// RustCFML-only test_engine_temps_savecontent_novar.cfm.
// ============================================================

function leakedKeys( keys ) {
	leaked = [];
	for ( k in listToArray( keys ) ) {
		if ( left( k, 2 ) == "__" && k != "__static" ) arrayAppend( leaked, k );
	}
	return arrayToList( leaked );
}

s = new EngineTempLeakFixture();
fixturePath = getDirectoryFromPath( getCurrentTemplatePath() ) & "../tags/cfloop_file_fixture.txt";

assert( "switch picks the right case", s.viaSwitch( "b" ), "B" );
assert( "switch falls back when nothing matches", s.viaSwitch( "z" ), "?" );
assertTrue( "file loop reads the fixture", s.countLines( fixturePath ) > 0 );
assert( "script cfinvoke with cfinvokeargument body", s.invokeWithBody(), "a" );
assert( "script cfmail with cfmailparam body", s.mailWithBody(), "ok" );
assert( "script loop array item+index", s.loopArrayItemIndex(), 6 );
assert( "script loop list item+index", s.loopListItemIndex(), 3 );
assert( "script loop collection item+index", s.loopCollectionItemIndex(), 3 );
assert( "script loop query (expression)", s.loopQueryExpr(), 2 );
assert( "script loop query (string name, as the tag form)", s.loopQueryStringName(), 2 );
assert( "script fixture: no engine temporary in variables", leakedKeys( s.variablesKeys() ), "" );

t = new EngineTempLeakTagFixture();
assert( "cfexecute variable=", t.executeTag(), "ok" );
assert( "cfmail with control flow around cfmailparam", t.mailWithControlFlow(), "ok" );
assert( "cfstoredproc with control flow around cfprocparam", t.storedProcWithControlFlow(), "ok" );
assert( "tag fixture: no engine temporary in variables", leakedKeys( t.variablesKeys() ), "" );

// A switch at template level must still work (its temp is declared in the
// page frame, as the for-in loop's are).
switch ( 3 ) {
	case 3: pageResult = "three"; break;
	default: pageResult = "none";
}
assert( "switch at page level", pageResult, "three" );

suiteEnd();
</cfscript>
