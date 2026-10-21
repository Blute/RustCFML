<cfscript>
// Count the posted parts only: the server adds `fieldNames` to the form scope.
parts = arrayFilter( structKeyArray( form ), function( k ) { return compareNoCase( k, "fieldnames" ) != 0; } );
writeOutput( "formKeyCount=" & arrayLen( parts ) );
</cfscript>
