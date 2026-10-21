<cfscript>
suiteBegin("Core: savecontent without variable= keeps its capture frame-local (GH ##493, RustCFML-only)");

// Lucee requires `variable=` on savecontent, so this superset form cannot be
// in the cross-engine suite. See test_engine_temps_not_in_variables.cfm.
f = new EngineTempLeakSaveContentFixture();
assert( "script savecontent without variable", f.scriptNoVar(), "ok" );
assert( "tag-style savecontent without variable", f.tagNoVar(), "ok" );
leaked = [];
for ( k in listToArray( f.variablesKeys() ) ) {
	if ( left( k, 2 ) == "__" && k != "__static" ) arrayAppend( leaked, k );
}
assert( "no engine temporary in variables", arrayToList( leaked ), "" );

suiteEnd();
</cfscript>
