# Exit code
1

# stdout
```
  x Error running JS plugin.
  | File path: <fixture>/files/throwInAfter/1.js
  | Error: `after` hook threw
  |     at after (<fixture>/plugin.ts:120:15)

  x create-once-errors-plugin(before-and-after-late): after hook:
  | id: create-once-errors-plugin/before-and-after-late
  | filename: <fixture>/files/throwInAfter/2.js
  | events: {
  |   "1.js": [
  |     "after: before-and-after",
  |     "after: before-and-after-late",
  |     "after: throw-in-after",
  |     "before: before-and-after",
  |     "before: before-and-after-late",
  |     "before: throw-in-after",
  |     "visit: before-and-after",
  |     "visit: before-and-after-late",
  |     "visit: throw-in-after"
  |   ],
  |   "2.js": [
  |     "after: before-and-after-late",
  |     "before: before-and-after-late",
  |     "visit: before-and-after-late"
  |   ]
  | }
   ,-[files/throwInAfter/2.js:1:1]
 1 | let y;
   : ^
   `----

  x Error running JS plugin.
  | File path: <fixture>/files/throwInBefore/1.js
  | Error: `before` hook threw
  |     at before (<fixture>/plugin.ts:82:15)

  x create-once-errors-plugin(before-and-after-late): after hook:
  | id: create-once-errors-plugin/before-and-after-late
  | filename: <fixture>/files/throwInBefore/2.js
  | events: {
  |   "1.js": [
  |     "after: before-and-after",
  |     "before: before-and-after",
  |     "before: throw-in-before"
  |   ],
  |   "2.js": [
  |     "after: before-and-after-late",
  |     "before: before-and-after-late",
  |     "visit: before-and-after-late"
  |   ]
  | }
   ,-[files/throwInBefore/2.js:1:1]
 1 | let y;
   : ^
   `----

  x Error running JS plugin.
  | File path: <fixture>/files/throwInVisit/1.js
  | Error: `Identifier` visit function threw
  |     at Identifier (<fixture>/plugin.ts:124:15)

  x create-once-errors-plugin(before-and-after-late): after hook:
  | id: create-once-errors-plugin/before-and-after-late
  | filename: <fixture>/files/throwInVisit/2.js
  | events: {
  |   "1.js": [
  |     "after: before-and-after",
  |     "after: before-and-after-late",
  |     "after: throw-in-visit",
  |     "before: before-and-after",
  |     "before: before-and-after-late",
  |     "before: throw-in-visit",
  |     "visit: before-and-after",
  |     "visit: throw-in-visit"
  |   ],
  |   "2.js": [
  |     "after: before-and-after-late",
  |     "before: before-and-after-late"
  |   ]
  | }
   ,-[files/throwInVisit/2.js:1:1]
 1 | let y;
   : ^
   `----

Found 0 warnings and 6 errors.
Finished in Xms on 6 files using X threads.
```

# stderr
```
```
