component mcp="life" version="1.0.0" description="Application lifecycle probe" {

	function state() tool="state" description="Report the application's boot state" readOnly=true {
		return {
			  "booted"    = structKeyExists( application, "booted" )
			, "bootCount" = application.bootCount ?: 0
			, "appName"   = application.applicationName ?: "(none)"
		};
	}

	// The example from docs/mcp.md's "Execution model" section, verbatim.
	function counter() tool="counter" description="Count calls in application scope" {
		application.hits = ( application.hits ?: 0 ) + 1;
		return application.hits;
	}
}
