<cfscript>
// One round trip on the PostgreSQL datasource named by RUSTCFML_TEST_PG_URL.
q = queryExecute("SELECT 1 AS one", [], { datasource: server.system.environment.RUSTCFML_TEST_PG_URL });
writeOutput("QUERY-OK " & q.one);
</cfscript>
