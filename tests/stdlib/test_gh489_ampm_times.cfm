<!---
  GitHub #489: `isValid("time", "12:30 AM")` was true, and `timeFormat()` on the
  same string then threw "Invalid date". A string cannot be a valid time and an
  unreadable one at once.

  The cause was two parsers. `isValid`/`isDate` carried their own private list
  of time formats that included `%I:%M %p`; the central date parser behind
  `parseDateTime`/`timeFormat` carried an AM/PM entry only after `%m/%d/%Y`, so
  a bare `12:30 AM` had nothing to match. The fix reads the time of day in one
  place, which is also why the date half below is exercised in every form: on
  Lucee ANY date form composes with ANY time form.

  Lucee does not use AM/PM to constrain the hour to 1-12 — it folds whatever
  hour was written (`13:30 AM` is 01:30, `23:30 PM` is 23:30), and accepts hour
  24 as rolling into the next day. Every expectation here was measured against
  Lucee 7.1.0+204.
--->
<cfscript>
suiteBegin("AM/PM times parse wherever they are valid (GitHub 489)");

function fmt( required string s ) {
	try { return dateTimeFormat( parseDateTime( arguments.s ), "yyyy-mm-dd HH:nn:ss.lll" ); }
	catch ( any e ) { return "ERR"; }
}

// --- the reported case, in both directions ---------------------------------
assertTrue( "12:30 AM is a valid time", isValid( "time", "12:30 AM" ) );
assert( "and formats", timeFormat( "12:30 AM", "HH:mm:ss" ), "00:30:00" );
assert( "12:30 AM parses", fmt( "12:30 AM" ), "1899-12-30 00:30:00.000" );
assert( "12:30 PM parses", fmt( "12:30 PM" ), "1899-12-30 12:30:00.000" );

// --- the marker's spelling --------------------------------------------------
assert( "lower case", fmt( "1:05 am" ), "1899-12-30 01:05:00.000" );
assert( "mixed case", fmt( "12:30 Am" ), "1899-12-30 00:30:00.000" );
assert( "no space", fmt( "12:30AM" ), "1899-12-30 00:30:00.000" );
assert( "extra space", fmt( "12:30  PM" ), "1899-12-30 12:30:00.000" );
assert( "bare initial a", fmt( "12:30 a" ), "1899-12-30 00:30:00.000" );
assert( "bare initial p", fmt( "12:30 p" ), "1899-12-30 12:30:00.000" );
assert( "dotted A.M. is not a marker", fmt( "12:30 A.M." ), "ERR" );
assert( "M alone is not a marker", fmt( "12:30 M" ), "ERR" );
assert( "a marker must end the string", fmt( "12:30 PM GMT" ), "ERR" );
assert( "a marker is not a prefix", fmt( "AM 12:30" ), "ERR" );

// --- the hour is FOLDED, not constrained to 1-12 ----------------------------
assert( "0 AM", fmt( "0:30 AM" ), "1899-12-30 00:30:00.000" );
assert( "0 PM", fmt( "0:30 PM" ), "1899-12-30 12:30:00.000" );
assert( "12 AM is midnight", fmt( "12:00 AM" ), "1899-12-30 00:00:00.000" );
assert( "12 PM is noon", fmt( "12:00 PM" ), "1899-12-30 12:00:00.000" );
assert( "13 AM", fmt( "13:30 AM" ), "1899-12-30 01:30:00.000" );
assert( "13 PM", fmt( "13:30 PM" ), "1899-12-30 13:30:00.000" );
assert( "23 AM", fmt( "23:30 AM" ), "1899-12-30 11:30:00.000" );
assert( "23 PM", fmt( "23:30 PM" ), "1899-12-30 23:30:00.000" );

// --- hour 24 rolls into the next day ----------------------------------------
assert( "24:00", fmt( "24:00" ), "1899-12-31 00:00:00.000" );
assert( "24:30", fmt( "24:30" ), "1899-12-31 00:30:00.000" );
assert( "24:00:00", fmt( "24:00:00" ), "1899-12-31 00:00:00.000" );
// With a marker, 24 folds to 12 rather than to 0 — so AM is noon and PM is the
// next midnight. Lucee's own reading; neither is a shape to write on purpose.
assert( "24 AM is noon", fmt( "24:00 AM" ), "1899-12-30 12:00:00.000" );
assert( "24 PM is the next midnight", fmt( "24:00 PM" ), "1899-12-31 00:00:00.000" );
assert( "25 is not an hour", fmt( "25:00" ), "ERR" );
assert( "the roll carries the date too", fmt( "1/2/2020 24:00" ), "2020-01-03 00:00:00.000" );

// --- seconds and fractions ---------------------------------------------------
assert( "seconds with a marker", fmt( "12:30:45 AM" ), "1899-12-30 00:30:45.000" );
assert( "one-digit fields", fmt( "1:2:3 PM" ), "1899-12-30 13:02:03.000" );
assert( "a fraction is padded, not parsed", fmt( "12:30:45.5 PM" ), "1899-12-30 12:30:45.500" );
assert( "60 minutes is not a time", fmt( "12:60 AM" ), "ERR" );
assertFalse( "60 seconds is not a time either", isValid( "time", "12:30:60" ) );
assert( "an hour alone is not a time", fmt( "12 PM" ), "ERR" );
assert( "99 is not an hour", fmt( "99:30 AM" ), "ERR" );

// --- ANY date form composes with ANY time form -------------------------------
assert( "slash date", fmt( "1/2/2020 12:30 AM" ), "2020-01-02 00:30:00.000" );
assert( "slash date, no space", fmt( "1/2/2020 12:30AM" ), "2020-01-02 00:30:00.000" );
assert( "ISO date", fmt( "2020-01-02 12:30 AM" ), "2020-01-02 00:30:00.000" );
assert( "ISO date with T", fmt( "2020-01-02T12:30 AM" ), "2020-01-02 00:30:00.000" );
assert( "ISO date, slashes", fmt( "2020/01/02 12:30 AM" ), "2020-01-02 00:30:00.000" );
assert( "dotted date", fmt( "1.2.2020 12:30 AM" ), "2020-01-02 00:30:00.000" );
assert( "month-name date", fmt( "Jan 2 2020 12:30 AM" ), "2020-01-02 00:30:00.000" );
assert( "trailing junk is refused", fmt( "12:30 PM extra" ), "ERR" );
assert( "two times are refused", fmt( "12:30 AM 12:30" ), "ERR" );

// --- what already worked still works -----------------------------------------
assert( "24-hour time", fmt( "12:30" ), "1899-12-30 12:30:00.000" );
assert( "with seconds", fmt( "12:30:45" ), "1899-12-30 12:30:45.000" );
assert( "surrounding space", fmt( "  12:30 AM  " ), "1899-12-30 00:30:00.000" );
assert( "a date alone is still midnight", fmt( "2020-01-02" ), "2020-01-02 00:00:00.000" );
assertTrue( "a date string is a valid time on Lucee", isValid( "time", "2020-01-02" ) );
assertFalse( "a bare number is not", isValid( "time", "1" ) );

suiteEnd();
</cfscript>
