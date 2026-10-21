<cfset session.hits = (session.hits ?: 0) + 1><cfoutput>cfid=#session.cfid# hits=#session.hits#</cfoutput>
