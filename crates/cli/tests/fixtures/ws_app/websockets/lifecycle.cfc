component socket="/lifecycle" encoding="json" {

	function onMessage( socket, message ) {
		var seen = structKeyExists( application, "fromSocket" ) ? application.fromSocket : 0;
		application[ "fromSocket" ] = seen + 1;
		return {
			  "booted"     = structKeyExists( application, "booted" )
			, "bootCount"  = application.bootCount ?: 0
			, "appName"    = application.applicationName ?: "(none)"
			, "fromSocket" = application.fromSocket
		};
	}
}
