<!---
    <cfhttpparam type="body"> with a binary value must send the bytes as the raw
    request body, byte-for-byte.

    The everyday shape is uploading a file to object storage (S3, R2) with a
    presigned PUT URL, where the request body must be exactly the file's bytes:

        <cfhttp url="#presignedUrl#" method="put">
            <cfhttpparam type="header" name="Content-Type" value="image/png">
            <cfhttpparam type="body" value="#fileReadBinary(path)#">
        </cfhttp>

    Lucee sends the binary value unchanged. RustCFML sends the 8-byte string
    "<Binary>" (the binary value's display form) in place of the bytes, in the
    tag form and the script form alike, whatever the bytes are. A string body
    is sent correctly (control leg).

    Like the other cfhttp round-trip suites this runs only when served
    (cgi.server_port present) and sends to a local target,
    cfhttpparam_binary_body_target.cfm, which reports the length, SHA-256 and
    hex of the request body it received. Run without a server it skips.
--->
<cfscript>
suiteBegin("cfhttpparam type=body sends a binary value byte-for-byte");

serverPort = structKeyExists(cgi, "server_port") ? trim(cgi.server_port) : "";
skip = serverPort == "" || serverPort == "0";

if (skip) {
    assertTrue("cfhttpparam binary body skipped (no cgi.server_port)", true);
} else {
    hbbUrl = "http://127.0.0.1:" & serverPort & "/tests/tags/cfhttpparam_binary_body_target.cfm";

    // Six bytes including 0x00 and 0xFF, with bytes after the NUL.
    hbbSmall = binaryDecode("00FF10414280", "hex");
    // A 1x1 PNG (70 bytes): what a presigned PUT upload actually carries.
    hbbPng = binaryDecode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==", "base64");
    hbbPngSha = "cdb30873bdf16770bfea1fe86e44db7476e504c2dca1542b0660b20f47f523a7";
    // The same six bytes, read back from a file.
    hbbFile = getTempDirectory() & "/rustcfml_hbb_" & createUUID() & ".bin";
    fileWrite(hbbFile, hbbSmall);

    // What the target received, or the error/fileContent when it cannot be read.
    function hbbReceived(required struct result) {
        if (!isJSON(arguments.result.fileContent ?: "")) {
            return { "len": -1, "sha256": "", "hex": "unreadable: " & (arguments.result.fileContent ?: "") };
        }
        return deserializeJSON(arguments.result.fileContent);
    }
}
</cfscript>

<cfif NOT skip>
    <!--- Control: a string body arrives as sent. --->
    <cfhttp url="#hbbUrl#" method="put" result="hbbString">
        <cfhttpparam type="header" name="Content-Type" value="application/octet-stream" />
        <cfhttpparam type="body" value="hello" />
    </cfhttp>

    <!--- Tag form, six bytes with 0x00 and 0xFF. --->
    <cfhttp url="#hbbUrl#" method="put" result="hbbTagSmall">
        <cfhttpparam type="header" name="Content-Type" value="application/octet-stream" />
        <cfhttpparam type="body" value="#hbbSmall#" />
    </cfhttp>

    <!--- Tag form, a PNG. --->
    <cfhttp url="#hbbUrl#" method="put" result="hbbTagPng">
        <cfhttpparam type="header" name="Content-Type" value="image/png" />
        <cfhttpparam type="body" value="#hbbPng#" />
    </cfhttp>

    <!--- Tag form, the bytes of a file (fileReadBinary). --->
    <cfhttp url="#hbbUrl#" method="post" result="hbbTagFile">
        <cfhttpparam type="header" name="Content-Type" value="application/octet-stream" />
        <cfhttpparam type="body" value="#fileReadBinary(hbbFile)#" />
    </cfhttp>

    <cfscript>
    // Script form.
    cfhttp(url = hbbUrl, method = "put", result = "hbbScript") {
        cfhttpparam(type = "header", name = "Content-Type", value = "application/octet-stream");
        cfhttpparam(type = "body", value = hbbSmall);
    }

    got = hbbReceived(hbbString);
    assert("control: a string body arrives as sent", got.hex, "68656c6c6f");

    got = hbbReceived(hbbTagSmall);
    assert("tag: binary body 00FF10414280 arrives byte-for-byte", got.hex, "00ff10414280");

    got = hbbReceived(hbbTagPng);
    assert("tag: a 70-byte PNG body arrives with all 70 bytes", got.len, 70);
    assert("tag: a PNG body arrives with the same SHA-256", got.sha256, hbbPngSha);

    got = hbbReceived(hbbTagFile);
    assert('tag: value="##fileReadBinary(path)##" arrives byte-for-byte', got.hex, "00ff10414280");

    got = hbbReceived(hbbScript);
    assert("script: cfhttpparam(type=""body"", value=binary) arrives byte-for-byte", got.hex, "00ff10414280");

    try { fileDelete(hbbFile); } catch (any e) {}
    </cfscript>
</cfif>

<cfscript>
suiteEnd();
</cfscript>
