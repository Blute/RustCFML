<cfscript>
suiteBegin("Tags: cffile upload accept / strict / extension lists / path containment");

// ============================================================
// Background
// ============================================================
// fileUpload() and <cffile action="upload"> read `accept` and discarded it, took
// no `strict`, and joined the client filename straight onto `destination`. A
// `.cfm` posted with Content-Type image/png against accept="image/png" was saved
// and then served. The upload part is synthesised here as the form.<field>
// struct the multipart parser builds (tempFilePath / clientFile / contentType),
// which is why this file is RustCFML-only: Lucee reads its own FormItems.
// ============================================================

zzupBase = getTempDirectory() & "zzcffileupload_" & createUUID();
directoryCreate(zzupBase);
zzupDest = zzupBase & "/dest";

// A fresh temp "upload" for each attempt, registered under form.<field>.
function zzupPart(required string field, required string clientFile, required string contentType) {
	var tmp = zzupBase & "/part_" & createUUID() & ".tmp";
	fileWrite(tmp, "payload for " & arguments.clientFile);
	form[arguments.field] = {
		tempFilePath: tmp,
		clientFile: arguments.clientFile,
		serverFile: arguments.clientFile,
		contentType: arguments.contentType,
		fileSize: 12,
		fileWasSaved: true
	};
	return tmp;
}

try {
	// ---- (1) accept by MIME, declared and extension agree ----
	zzupPart("f1", "photo.png", "image/png");
	r = fileUpload(zzupDest, "f1", "image/png", "overwrite");
	assertTrue("png accepted by image/png", r.fileWasSaved);
	assert("saved under the client name", r.serverFile, "photo.png");
	assert("serverFileExt", r.serverFileExt, "png");
	assert("contentType split", r.contentType, "image");
	assert("contentSubType split", r.contentSubType, "png");
	assertTrue("file is in the destination", fileExists(zzupDest & "/photo.png"));
	assertTrue("temp part removed", !fileExists(form.f1.tempFilePath));

	// ---- (2) the proven bypass: .cfm declared as image/png ----
	zzupPart("f2", "shell.cfm", "image/png");
	assertThrows("cfm declared as png is rejected (strict default)", function() {
		fileUpload(zzupDest, "f2", "image/png", "overwrite");
	});
	assertTrue("cfm was not saved", !fileExists(zzupDest & "/shell.cfm"));

	// It is also rejected with no accept at all: Lucee's default block list.
	zzupPart("f2b", "shell2.cfm", "text/plain");
	try {
		fileUpload(zzupDest, "f2b", "", "overwrite");
		assertTrue("default block list refused .cfm", false);
	} catch (application e) {
		assertTrue("default block list throws type=application", true);
		assertTrue("message names the extension", findNoCase("[cfm]", e.message) > 0);
	}
	assertTrue("cfm not saved via block list", !fileExists(zzupDest & "/shell2.cfm"));

	// ---- (3) strict: declared type must agree with the extension ----
	zzupPart("f3", "notes.txt", "image/png");
	assertThrows("txt declared as png rejected under strict", function() {
		fileUpload(zzupDest, "f3", "image/png", "overwrite", true);
	});
	zzupPart("f3b", "notes.txt", "image/png");
	r = fileUpload(zzupDest, "f3b", "image/png", "overwrite", false);
	assertTrue("strict=false trusts the declared type", r.fileWasSaved);

	// ---- (4) MIME wildcard ----
	zzupPart("f4", "pic.jpg", "image/jpeg");
	r = fileUpload(zzupDest, "f4", "image/*", "overwrite");
	assertTrue("image/* accepts a jpeg", r.fileWasSaved);
	zzupPart("f4b", "doc.pdf", "application/pdf");
	assertThrows("image/* rejects a pdf", function() {
		fileUpload(zzupDest, "f4b", "image/*", "overwrite");
	});

	// ---- (5) accept by extension ----
	zzupPart("f5", "pic.JPG", "application/octet-stream");
	r = fileUpload(zzupDest, "f5", ".jpg, *.png", "overwrite");
	assertTrue("extension entry matches case-insensitively", r.fileWasSaved);
	zzupPart("f5b", "pic.gif", "image/gif");
	assertThrows("extension list rejects an unlisted extension", function() {
		fileUpload(zzupDest, "f5b", ".jpg,.png", "overwrite");
	});

	// ---- (6) allowedExtensions / blockedExtensions ----
	zzupPart("f6", "data.csv", "text/csv");
	assertThrows("allowedExtensions refuses what it does not list", function() {
		fileUpload(destination=zzupDest, fileField="f6", nameConflict="overwrite", allowedExtensions="txt,pdf");
	});
	zzupPart("f6b", "data.csv", "text/csv");
	r = fileUpload(destination=zzupDest, fileField="f6b", nameConflict="overwrite", allowedExtensions=".csv");
	assertTrue("allowedExtensions admits a listed extension", r.fileWasSaved);
	zzupPart("f6c", "page.html", "text/html");
	r = fileUpload(destination=zzupDest, fileField="f6c", nameConflict="overwrite", allowedExtensions="html");
	assertTrue("allowedExtensions overrides the default block list", r.fileWasSaved);
	zzupPart("f6d", "tool.exe", "application/octet-stream");
	assertThrows("blockedExtensions refuses a listed extension", function() {
		fileUpload(destination=zzupDest, fileField="f6d", nameConflict="overwrite", blockedExtensions="exe,bat");
	});

	// ---- (7) client filename must be a bare name ----
	zzupPart("f7", "../escaped.txt", "text/plain");
	assertThrows("../ client filename rejected", function() {
		fileUpload(zzupDest, "f7", "", "overwrite");
	});
	assertTrue("nothing written above the destination", !fileExists(zzupBase & "/escaped.txt"));
	zzupPart("f7b", "sub/inner.txt", "text/plain");
	assertThrows("client filename with a separator rejected", function() {
		fileUpload(zzupDest, "f7b", "", "overwrite");
	});
	assertTrue("no subdirectory was created", !directoryExists(zzupDest & "/sub"));

	// ---- (8) nameConflict ----
	zzupPart("f8", "photo.png", "image/png");
	assertThrows("nameConflict=error (the default) throws on an existing file", function() {
		fileUpload(zzupDest, "f8", "image/png");
	});
	zzupPart("f8b", "photo.png", "image/png");
	r = fileUpload(zzupDest, "f8b", "image/png", "skip");
	assertTrue("skip: fileExisted", r.fileExisted);
	assertTrue("skip: not saved", !r.fileWasSaved);
	zzupPart("f8c", "photo.png", "image/png");
	r = fileUpload(zzupDest, "f8c", "image/png", "makeunique");
	assertTrue("makeunique: saved", r.fileWasSaved);
	assertTrue("makeunique: renamed", r.fileWasRenamed);
	assertTrue("makeunique: new name differs", r.serverFile != "photo.png");
	assertTrue("makeunique: keeps the extension", right(r.serverFile, 4) == ".png");
	assertTrue("makeunique: file exists", fileExists(zzupDest & "/" & r.serverFile));
	zzupPart("f8d", "photo.png", "image/png");
	r = fileUpload(zzupDest, "f8d", "image/png", "overwrite");
	assertTrue("overwrite: fileWasOverwritten", r.fileWasOverwritten);

	// ---- (9) the tag form reaches the same checks and sets cffile/result ----
	zzupPart("f9", "tagpic.png", "image/png");
	cffile = "";
	include "test_cffile_upload_accept_tag.cfm";
	assertTrue("tag form saved via cffile struct", isStruct(cffile) && cffile.fileWasSaved);
	assert("tag form serverFile", cffile.serverFile, "tagpic.png");
	assertTrue("tag form result= variable", isStruct(zzupTagResult) && zzupTagResult.fileWasSaved);
	assertTrue("tag form rejected the cfm", zzupTagRejected);
	assertTrue("tag form rejected cfm was not saved", !fileExists(zzupDest & "/tagshell.cfm"));

	// ---- (10) script-call form ----
	zzupPart("f10", "script.png", "image/png");
	cffile(action="upload", destination=zzupDest, fileField="f10", accept="image/png", nameConflict="overwrite", result="zzupScriptResult");
	assertTrue("script-call form saves", isStruct(zzupScriptResult) && zzupScriptResult.fileWasSaved);

	// ---- (11) fileUploadAll applies the policy to every part ----
	structClear(form);
	zzupPart("a1", "one.png", "image/png");
	zzupPart("a2", "two.png", "image/png");
	arr = fileUploadAll(zzupDest, "image/png", "overwrite");
	assert("fileUploadAll saved both", arrayLen(arr), 2);
	structClear(form);
	zzupPart("a3", "three.png", "image/png");
	zzupPart("a4", "evil.cfm", "image/png");
	assertThrows("fileUploadAll throws when any part is rejected", function() {
		fileUploadAll(zzupDest, "image/png", "overwrite");
	});
	assertTrue("fileUploadAll rejected part not saved", !fileExists(zzupDest & "/evil.cfm"));
} finally {
	structClear(form);
	directoryDelete(zzupBase, true);
}

suiteEnd();
</cfscript>
