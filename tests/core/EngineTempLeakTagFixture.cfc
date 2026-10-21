<cfcomponent>
	<!--- Tag-form constructs whose lowering synthesises a hidden variable (GH #493). --->
	<cffunction name="executeTag" returntype="string">
		<cfset var out = "">
		<cftry>
			<cfexecute name="/bin/echo" arguments="hi" variable="out" timeout="5"></cfexecute>
			<cfcatch></cfcatch>
		</cftry>
		<cfreturn "ok">
	</cffunction>
	<cffunction name="mailWithControlFlow" returntype="string">
		<cftry>
			<cfmail to="a@b.c" from="a@b.c" subject="s" server="127.0.0.1" port="1" timeout="1"><cfif true><cfmailparam name="X-A" value="1"></cfif>body</cfmail>
			<cfcatch></cfcatch>
		</cftry>
		<cfreturn "ok">
	</cffunction>
	<cffunction name="storedProcWithControlFlow" returntype="string">
		<cftry>
			<cfstoredproc procedure="p" datasource="no_such_datasource_493" result="local.spr"><cfif true><cfprocparam value="1" cfsqltype="cf_sql_integer"></cfif></cfstoredproc>
			<cfcatch></cfcatch>
		</cftry>
		<cfreturn "ok">
	</cffunction>
	<cffunction name="variablesKeys" returntype="string">
		<cfreturn structKeyList( variables )>
	</cffunction>
</cfcomponent>
