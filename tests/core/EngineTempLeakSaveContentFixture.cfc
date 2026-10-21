component {
	// `savecontent` / `<cfsavecontent>` with no `variable=` is RustCFML-only
	// (Lucee rejects it at parse time). Its capture must still be frame-local
	// (GH #493), hence this fixture sits in a rustcfmlOnly test.
	public string function scriptNoVar() {
		savecontent { writeOutput( "captured" ); }
		return "ok";
	}
	public string function tagNoVar() {
		cfsavecontent() { writeOutput( "captured" ); }
		return "ok";
	}
	public string function variablesKeys() { return structKeyList( variables ); }
}
