<cfscript>
// cfheader / cfcookie / cfcontent / cflocation / getPageContext().getResponse()
// validate what they put on the wire: a status code outside 100-599, a header
// name that is not an RFC 7230 token, or a CR/LF (any control character) in a
// header value, cookie attribute, content type or redirect URL is a CFML
// `application` error. Before this, `<cfheader statusCode="1000">` and
// `value="a#chr(13)##chr(10)#X-Injected: 1"` reached the server's response
// builder and PANICKED the worker (and the CR/LF is response splitting).
//
// RustCFML-only: Lucee passes these straight to the servlet container, which
// does not throw at the tag (statusCode=1000 is accepted by Tomcat), so the
// assertThrows below do not hold there.
suiteBegin("Tags: cfheader/cfcookie/cflocation reject invalid status codes and CR/LF");

CR = chr(13); LF = chr(10);

// ---- valid values are accepted (CLI mode records them; nothing is sent) ----
cfheader(name="X-Nesting-Test", value="ok");
cfheader(name="X-Tab-Allowed", value="a" & chr(9) & "b");
cfheader(statusCode=204, statusText="No Content");
cfheader(statusCode="404");
cfcookie(name="ck_valid", value="v", path="/x");
cfcontent(type="text/plain; charset=utf-8");
// Put the response back: this file runs inside tests/runner.cfm, whose served
// response must stay 200 text/html or the serve gate reads a passing run as a
// failed request (a 404 with a green SUMMARY in its body).
cfheader(statusCode=200, statusText="OK");
cfcontent(type="text/html; charset=utf-8");
assertTrue("valid header/cookie/content values are accepted", true);

// ---- status code ----
assertThrows("cfheader statusCode=1000 throws", function() { cfheader(statusCode=1000); });
assertThrows("cfheader statusCode=99 throws", function() { cfheader(statusCode=99); });
assertThrows("cfheader statusCode=600 throws", function() { cfheader(statusCode=600); });
assertThrows("cfheader statusCode=-1 throws", function() { cfheader(statusCode=-1); });
try {
    cfheader(statusCode=1000);
    assertTrue("status error names the value", false);
} catch (any e) {
    assertTrue("status error names the value", find("1000", e.message) > 0);
}
assertThrows("response.setStatus(1000) throws", function() {
    getPageContext().getResponse().setStatus(1000);
});
assertThrows("cflocation statusCode=1000 throws", function() {
    location(url="/somewhere", addToken=false, statusCode=1000);
});

// ---- header name ----
assertThrows("cfheader name with a space throws", function() { cfheader(name="bad name", value="1"); });
assertThrows("cfheader name with CR/LF throws", function() { cfheader(name="x" & CR & LF & "y", value="1"); });
assertThrows("cfheader name with a colon throws", function() { cfheader(name="x:y", value="1"); });
assertThrows("cfheader empty name throws", function() { cfheader(name="", value="1"); });

// ---- header value (response splitting) ----
assertThrows("cfheader value with CRLF throws", function() {
    cfheader(name="x", value="a" & CR & LF & "X-Injected: 1");
});
assertThrows("cfheader value with a bare LF throws", function() { cfheader(name="x", value="a" & LF & "b"); });
assertThrows("cfheader value with a bare CR throws", function() { cfheader(name="x", value="a" & CR & "b"); });
assertThrows("cfheader value with NUL throws", function() { cfheader(name="x", value="a" & chr(0) & "b"); });
assertThrows("cfheader statusText with CRLF throws", function() {
    cfheader(statusCode=200, statusText="OK" & CR & LF & "X-Injected: 1");
});
assertThrows("response.setHeader value with CRLF throws", function() {
    getPageContext().getResponse().setHeader("x", "a" & CR & LF & "b");
});
assertThrows("response.addHeader name with a space throws", function() {
    getPageContext().getResponse().addHeader("bad name", "1");
});

// ---- cfcookie ----
assertThrows("cfcookie name with LF throws", function() { cfcookie(name="c" & LF & "x", value="1"); });
assertThrows("cfcookie value with CR throws", function() { cfcookie(name="c", value="1" & CR & "x"); });
assertThrows("cfcookie path with CRLF throws", function() { cfcookie(name="c", value="1", path="/" & CR & LF & "x"); });
assertThrows("cfcookie domain with LF throws", function() { cfcookie(name="c", value="1", domain="a" & LF & "b"); });
assertThrows("cfcookie name with a semicolon throws", function() { cfcookie(name="c;x", value="1"); });
assertThrows("cfcookie empty name throws", function() { cfcookie(name="", value="1"); });

// ---- cfcontent / cflocation ----
assertThrows("cfcontent type with LF throws", function() {
    cfcontent(type="text/plain" & LF & "X-Injected: 1");
});
assertThrows("cflocation url with LF throws", function() {
    location(url="/x" & LF & "X-Injected: 1", addToken=false);
});
assertThrows("response.sendRedirect url with CRLF throws", function() {
    getPageContext().getResponse().sendRedirect("/x" & CR & LF & "X-Injected: 1");
});

// ---- serve mode: the server survives and answers with an error, not a dead worker ----
serverPort = structKeyExists(cgi, "server_port") ? trim(cgi.server_port) : "";
if (serverPort == "" || serverPort == "0") {
    writeOutput(chr(10) & "  skipped cfheader HTTP subtests (no cgi.server_port - run via rustcfml --serve)" & chr(10));
} else {
    function hitTarget(required string testName) {
        var candidate = "";
        var lastResult = {};
        var bases = ["http://127.0.0.1:" & serverPort, "http://localhost:" & serverPort];
        for (candidate in bases) {
            http url=candidate & "/tests/tags/cfheader_validation_target.cfm?test=" & arguments.testName method="GET" result="httpResult" throwOnError="false";
            lastResult = httpResult;
            if (left(trim(httpResult.statuscode), 3) != "" && !findNoCase("Connection Failure", httpResult.statuscode)) {
                return httpResult;
            }
        }
        return lastResult;
    }
    okResult = hitTarget("ok");
    assert("target is reachable", left(trim(okResult.statuscode), 3), "200");
    assert("target body", trim(okResult.filecontent), "ok");

    for (bad in ["status1000", "crlfvalue", "crlfcookie", "crlflocation"]) {
        r = hitTarget(bad);
        code = left(trim(r.statuscode), 3);
        // The tag throws inside the request: an error response, and NOT the
        // injected header. A dead worker shows up as a connection failure.
        assertTrue(bad & ": request completes with an HTTP status (" & r.statuscode & ")", isNumeric(code));
        assertFalse(bad & ": the injected header is not emitted",
            structKeyExists(r.responseheader, "X-Injected"));
        assertFalse(bad & ": target body not reached", trim(r.filecontent) == "ok");
    }
    // …and the server is still serving afterwards.
    after = hitTarget("ok");
    assert("server still answers after the rejected requests", left(trim(after.statuscode), 3), "200");
}

suiteEnd();
</cfscript>
