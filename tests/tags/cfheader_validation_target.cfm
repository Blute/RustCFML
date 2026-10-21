<cfscript>
// Target for tests/tags/test_cfheader_validation.cfm (serve mode). Each case
// sets an invalid header/status/cookie/redirect; the tag must throw, so the
// body "ok" is never reached and no "X-Injected" header is ever emitted.
param name="url.test" default="ok";
CR = chr(13); LF = chr(10);
switch (url.test) {
    case "status1000":
        cfheader(statusCode=1000);
        break;
    case "crlfvalue":
        cfheader(name="X-Probe", value="a" & CR & LF & "X-Injected: 1");
        break;
    case "crlfcookie":
        cfcookie(name="probe", value="a" & CR & LF & "X-Injected: 1");
        break;
    case "crlflocation":
        location(url="/x" & CR & LF & "X-Injected: 1", addToken=false);
        break;
}
cfcontent(type="text/plain");
writeOutput("ok");
</cfscript>
