# Exit code
0

# stdout
```
Type-aware Svelte linting is not supported yet; `.svelte` files are excluded from TypeScript type-aware checks and use rsvelte syntax plus native script linting instead.

  ! eslint(no-debugger): `debugger` statement is not allowed
   ,-[files/App.svelte:2:1]
 1 | <script lang="ts">
 2 | debugger;
   : ^^^^^^^^^
 3 | let count: number = 0;
   `----
  help: Remove the debugger statement

Found 1 warning and 0 errors.
Finished in Xms on 1 file with 111 rules using X threads.
```

# stderr
```
```
