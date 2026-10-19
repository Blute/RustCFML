<cfscript>
suiteBegin("A query cell copied into a struct stays a simple value through Duplicate() and ObjectSave()/ObjectLoad()");

// Found via Preside's full page cache on a resource library page. The resource detail handler builds a
// dummy site tree page from a query: event.initializeDummyPresideSiteTreePage( id=resourceDetail.id, ... ).
// That function keeps Duplicate( arguments ) as the page struct, the cache store deep-copies it with
// Duplicate() again, then ColdBox's DiskStore writes it with objectSave() and reads it back with
// objectLoad(). On Lucee the id comes back as the string it went in as. On RustCFML it came back as a
// one-element array, ["68C1D636-..."], and every cached request then failed in getCurrentPageId() with
// "Cannot cast Object type [Array] to a value of type [string]".
//
// What this file shows (RustCFML v0.738.1): the cell is a simple value in the struct, and it survives
// Duplicate() and serializeJson(), but objectSave() writes the cell reference as the WHOLE column array,
// not the current row's value. Inside a query loop every row saves as ["first","second"]. A literal string,
// or the cell forced to a string with `q.id & ""`, is fine, so the bug is in objectSave()'s handling of a
// query cell reference stored in a struct.

q = queryNew( "id,title", "varchar,varchar", [ [ "68C1D636-BA5B-4D0C-845A2C97EE780638", "A resource" ] ] );

// ---- 1. the cell is a simple value wherever it lands ----
holder = { id = q.id, title = q.title };
assertTrue( "a cell assigned to a struct key is a simple value", isSimpleValue( holder.id ) );
assertFalse( "it is not an array", isArray( holder.id ) );
assert( "it compares as the string", holder.id, "68C1D636-BA5B-4D0C-845A2C97EE780638" );

// ---- 2. through a function's arguments scope, duplicated (what Preside does) ----
page = function( id, title ) { return duplicate( arguments ); };
dummy = page( id = q.id, title = q.title );
assertTrue( "Duplicate( arguments ) keeps the cell a simple value", isSimpleValue( dummy.id ) );
assertFalse( "Duplicate( arguments ) does not turn it into an array", isArray( dummy.id ) );
assert( "the duplicated argument is the string", dummy.id, "68C1D636-BA5B-4D0C-845A2C97EE780638" );

// ---- 3. a second Duplicate() of the outer struct (the cache's deep copy) ----
copy = duplicate( { presidePage = dummy } );
assertFalse( "a second Duplicate() keeps it a simple value", isArray( copy.presidePage.id ) );
assert( "and the value is unchanged", copy.presidePage.id, "68C1D636-BA5B-4D0C-845A2C97EE780638" );

// ---- 4. serialised and loaded back, as the DiskStore does ----
loaded = objectLoad( objectSave( copy ) );
assertTrue( "objectLoad( objectSave() ) gives a struct back", isStruct( loaded ) );
assertTrue( "the page id survives the round trip as a simple value", isSimpleValue( loaded.presidePage.id ?: [] ) );
assertFalse( "the page id is not a one-element array after the round trip", isArray( loaded.presidePage.id ?: [] ) );
// compared as JSON so a wrong type is reported as a failure rather than aborting the file
assert( "the page id is still the string", serializeJson( loaded.presidePage.id ?: "" ), '"68C1D636-BA5B-4D0C-845A2C97EE780638"' );
assert( "the title survives too", serializeJson( loaded.presidePage.title ?: "" ), '"A resource"' );

// ---- 5. the same cell straight into a struct, with no arguments scope involved ----
direct = objectLoad( objectSave( { id = q.id } ) );
assertFalse( "a bare cell in a struct also survives objectSave()/objectLoad()", isArray( direct.id ?: [] ) );
assert( "with its value", serializeJson( direct.id ?: "" ), '"68C1D636-BA5B-4D0C-845A2C97EE780638"' );

// ---- 5b. a plain string literal, to show the serialiser itself is fine with ordinary strings ----
plain = objectLoad( objectSave( { id = "68C1D636-BA5B-4D0C-845A2C97EE780638" } ) );
assert( "a literal string survives objectSave()/objectLoad()", serializeJson( plain.id ?: "" ), '"68C1D636-BA5B-4D0C-845A2C97EE780638"' );

// ---- 5c. the cell forced to a string first (the workaround Preside sites need today) ----
forced = objectLoad( objectSave( { id = q.id & "" } ) );
assert( "the cell concatenated with an empty string survives", serializeJson( forced.id ?: "" ), '"68C1D636-BA5B-4D0C-845A2C97EE780638"' );

// ---- 6. JSON, for comparison ----
json = serializeJson( { id = q.id } );
assert( "serializeJson writes the cell as a string", deserializeJson( json ).id, "68C1D636-BA5B-4D0C-845A2C97EE780638" );
assertFalse( "not as an array", isArray( deserializeJson( json ).id ) );

// ---- 7. the query itself stays a query, and the loop cursor is respected ----
multi = queryNew( "id", "varchar", [ [ "first" ], [ "second" ] ] );
seen = [];
cfloop( query = multi ) {
	seen.append( serializeJson( objectLoad( objectSave( { id = multi.id } ) ).id ) );
}
assert( "inside a loop each row's cell is what gets saved", arrayToList( seen, "|" ), '"first"|"second"' );

suiteEnd();
</cfscript>
