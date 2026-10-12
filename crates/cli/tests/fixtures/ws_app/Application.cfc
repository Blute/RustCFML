component {
	this.name = "wsFixtureApp";

	function onApplicationStart() {
		application.booted    = true;
		application.bootCount = ( application.bootCount ?: 0 ) + 1;
		return true;
	}
}
