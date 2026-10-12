component {
	this.name = "mcpFixtureApp";

	// Counts starts so a test can prove the handler ran ONCE, not per call.
	function onApplicationStart() {
		application.booted    = true;
		application.bootCount = ( application.bootCount ?: 0 ) + 1;
		return true;
	}
}
