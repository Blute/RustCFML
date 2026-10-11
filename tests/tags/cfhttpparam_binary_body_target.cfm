<cfsetting enablecfoutputonly="true" />
<!---
    Target for test_cfhttpparam_binary_body.cfm. Reports the raw request body
    exactly as it arrived (byte count, SHA-256 and hex of the bytes), so the
    caller can assert that a binary <cfhttpparam type="body"> value was sent
    byte-for-byte. The caller sends Content-Type: application/octet-stream, so
    getHTTPRequestData().content is binary on both engines.
--->
<cfscript>
tgtBody = getHTTPRequestData().content;
if (!isBinary(tgtBody)) {
    tgtBody = charsetDecode(tgtBody, "utf-8");
}
writeOutput(serializeJSON({
    "len": len(tgtBody),
    "sha256": lCase(hash(tgtBody, "SHA-256")),
    "hex": lCase(binaryEncode(tgtBody, "hex"))
}));
</cfscript>
