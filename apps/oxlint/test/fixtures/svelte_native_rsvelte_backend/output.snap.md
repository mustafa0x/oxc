# Exit code
1

# stdout
```
  ! eslint(no-debugger): `debugger` statement is not allowed
   ,-[files/App.svelte:2:1]
 1 | <script>
 2 | debugger;
   : ^^^^^^^^^
 3 | </script>
   `----
  help: Remove the debugger statement

  x svelte(script_invalid_context): script_invalid_context: If the context attribute is supplied, its value must be "module"
  | https://svelte.dev/e/script_invalid_context
   ,-[files/Invalid.svelte:1:9]
 1 | <script context="not-module"></script>
   :         ^^^^^^^^^^|^^^^^^^^^
   :                   `-- Svelte compiler error
   `----

Found 1 warning and 1 error.
Finished in Xms on 2 files with 96 rules using X threads.
```

# stderr
```
```
