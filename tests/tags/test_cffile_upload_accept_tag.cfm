<!--- Tag-form half of test_cffile_upload_accept.cfm: included so the tags run at
      page level, where <cffile action="upload"> writes its `cffile` struct. --->
<cffile action="upload" fileField="f9" destination="#zzupDest#" accept="image/png" nameConflict="overwrite">
<cfset zzupPart("f9b", "tagpic2.png", "image/png")>
<cffile action="upload" fileField="f9b" destination="#zzupDest#" accept="image/png" nameConflict="overwrite" result="zzupTagResult">
<cfset zzupPart("f9c", "tagshell.cfm", "image/png")>
<cfset zzupTagRejected = false>
<cftry>
	<cffile action="upload" fileField="f9c" destination="#zzupDest#" accept="image/png" nameConflict="overwrite">
	<cfcatch type="application"><cfset zzupTagRejected = true></cfcatch>
</cftry>
