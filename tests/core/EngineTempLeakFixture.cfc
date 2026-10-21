component {
	// Script-form constructs whose lowering synthesises a hidden variable.
	// Each method must leave NO `__`-prefixed key in `variables` (GH #493).
	public string function viaSwitch( required string x ) {
		switch ( arguments.x ) {
			case "a": return "A";
			case "b": return "B";
		}
		return "?";
	}
	public numeric function countLines( required string path ) {
		var n = 0;
		cfloop( file = arguments.path, index = "local.ln" ) { n++; }
		return n;
	}
	public string function invokeWithBody() {
		cfinvoke( component = this, method = "echoArgs", returnvariable = "local.rv" ) {
			cfinvokeargument( name = "a", value = "1" );
		}
		return local.rv;
	}
	public string function echoArgs() { return structKeyList( arguments ); }
	public string function mailWithBody() {
		try {
			cfmail( to = "a@b.c", from = "a@b.c", subject = "s", server = "127.0.0.1", port = "1", timeout = "1" ) {
				cfmailparam( name = "X-A", value = "1" );
				writeOutput( "body" );
			}
		} catch ( any e ) {}
		return "ok";
	}
	public numeric function loopArrayItemIndex() {
		var n = 0;
		loop array = [ 1, 2, 3 ] item = "local.it" index = "local.ix" { n += local.it; }
		return n;
	}
	public numeric function loopListItemIndex() {
		var n = 0;
		loop list = "a,b,c" item = "local.it" index = "local.ix" { n++; }
		return n;
	}
	public numeric function loopCollectionItemIndex() {
		var n = 0;
		loop collection = { a = 1, b = 2 } item = "local.v" index = "local.k" { n += local.v; }
		return n;
	}
	public numeric function loopQueryExpr() {
		var q = queryNew( "col", "varchar", [ [ "a" ], [ "b" ] ] );
		var n = 0;
		loop query = q { n++; }
		return n;
	}
	public numeric function loopQueryStringName() {
		local.q = queryNew( "col", "varchar", [ [ "a" ], [ "b" ] ] );
		var n = 0;
		loop query = "local.q" { n++; }
		return n;
	}
	public string function variablesKeys() { return structKeyList( variables ); }
}
