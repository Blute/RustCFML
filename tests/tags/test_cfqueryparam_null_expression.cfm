<!---
    cfqueryparam's null attribute must be evaluated, not just read as a literal.

    The everyday shape is "store NULL when the field is blank":

        <cfqueryparam cfsqltype="varchar" value="#x#" null="#len(x) EQ 0#">

    Lucee binds SQL NULL whenever the null attribute evaluates to true, whether
    it is written as a literal (null="true", null="yes") or as an expression
    (null="#true#", null="#someFlag#", null="#len(x) EQ 0#", null="#NOT len(x)#").
    RustCFML honours only the literal form: with an expression the value is
    bound instead, so a blank field is stored as '' where the code asked for NULL.

    The same computed flag already binds NULL through every other route
    (attributeCollection, script cfqueryparam(), a queryExecute param struct);
    those legs are controls, as are the literal forms and the false cases.

    Driven on an inline SQLite JDBC datasource, so no server is needed. Lucee
    ships no SQLite JDBC driver, so there the suite records one informational
    pass (the test_dml_returns_empty_query convention); with sqlite-jdbc on
    Lucee's classpath every leg runs and is green on Lucee 7.
--->
<cfscript>
suiteBegin("cfqueryparam: null attribute given as an expression binds NULL");

qpnFile = getTempDirectory() & "/rustcfml_qpnull_" & createUUID() & ".db";
qpnDs = { class: "org.sqlite.JDBC", connectionString: "jdbc:sqlite:" & qpnFile };

qpnDriver = true;
try {
    queryExecute("CREATE TABLE contacts (id INTEGER PRIMARY KEY, phone VARCHAR(20))", [], { datasource: qpnDs });
} catch (any e) {
    qpnDriver = false;
}

qpnBlank = "";
qpnFilled = "07 5444 0000";
qpnIsBlank = true;
</cfscript>

<cfif NOT qpnDriver>
    <cfscript>assertTrue("skipped: no SQLite JDBC driver on this engine", true);</cfscript>
<cfelse>

<!--- Controls: the literal forms bind NULL. --->
<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="true">) IS NULL AS isn
</cfquery>
<cfset assert('control: literal null="true" binds NULL', qpnQ.isn[1], 1)>

<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="yes">) IS NULL AS isn
</cfquery>
<cfset assert('control: literal null="yes" binds NULL', qpnQ.isn[1], 1)>

<!--- The gap: the same true flag, written as an expression. --->
<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="#true#">) IS NULL AS isn
</cfquery>
<cfset assert('null="##true##" binds NULL', qpnQ.isn[1], 1)>

<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="#qpnIsBlank#">) IS NULL AS isn
</cfquery>
<cfset assert('null="##booleanVariable##" (true) binds NULL', qpnQ.isn[1], 1)>

<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="#len(qpnBlank) EQ 0#">) IS NULL AS isn
</cfquery>
<cfset assert('null="##len(x) EQ 0##" with x="" binds NULL', qpnQ.isn[1], 1)>

<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="#NOT len(qpnBlank)#">) IS NULL AS isn
</cfquery>
<cfset assert('null="##NOT len(x)##" with x="" binds NULL', qpnQ.isn[1], 1)>

<!--- The real-world shape: a blank field saved through null="#len(x) EQ 0#" is stored as NULL, not ''. --->
<cfquery datasource="#qpnDs#">
    INSERT INTO contacts (id, phone)
    VALUES (1, <cfqueryparam cfsqltype="varchar" value="#qpnBlank#" null="#len(qpnBlank) EQ 0#">)
</cfquery>
<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT COUNT(*) AS n FROM contacts WHERE id = 1 AND phone IS NULL
</cfquery>
<cfset assert('INSERT with null="##len(x) EQ 0##" and x="" stores NULL, not an empty string', qpnQ.n[1], 1)>

<!--- Controls: a false flag, literal or computed, binds the value. --->
<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT <cfqueryparam cfsqltype="varchar" value="#qpnFilled#" null="false"> AS v
</cfquery>
<cfset assert('control: literal null="false" binds the value', qpnQ.v[1], qpnFilled)>

<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT <cfqueryparam cfsqltype="varchar" value="#qpnFilled#" null="#false#"> AS v
</cfquery>
<cfset assert('control: null="##false##" binds the value', qpnQ.v[1], qpnFilled)>

<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT <cfqueryparam cfsqltype="varchar" value="#qpnFilled#" null="#len(qpnFilled) EQ 0#"> AS v
</cfquery>
<cfset assert('control: null="##len(x) EQ 0##" with x filled binds the value', qpnQ.v[1], qpnFilled)>

<!--- Controls: the same computed flag through the other routes already binds NULL. --->
<cfset qpnAttrs = { cfsqltype: "varchar", value: qpnBlank, null: len(qpnBlank) EQ 0 }>
<cfquery name="qpnQ" datasource="#qpnDs#">
    SELECT (<cfqueryparam attributeCollection="#qpnAttrs#">) IS NULL AS isn
</cfquery>
<cfset assert("control: attributeCollection with a computed null binds NULL", qpnQ.isn[1], 1)>

<cfscript>
cfquery(name = "qpnQ", datasource = qpnDs) {
    writeOutput("SELECT (");
    cfqueryparam(cfsqltype = "varchar", value = qpnBlank, null = len(qpnBlank) EQ 0);
    writeOutput(") IS NULL AS isn");
}
assert("control: script cfqueryparam() with a computed null binds NULL", qpnQ.isn[1], 1);

qpnQ = queryExecute("SELECT (:p) IS NULL AS isn",
    { p: { value: qpnBlank, cfsqltype: "varchar", null: len(qpnBlank) EQ 0 } }, { datasource: qpnDs });
assert("control: queryExecute param struct with a computed null binds NULL", qpnQ.isn[1], 1);
</cfscript>

</cfif>

<cfscript>
try { fileDelete(qpnFile); } catch (any e) {}
suiteEnd();
</cfscript>
