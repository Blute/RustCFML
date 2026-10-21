component {
    this.name = "server-hardening-test";
    this.sessionManagement = true;
    function onRequest(targetPage) { include "#targetPage#"; }
}
