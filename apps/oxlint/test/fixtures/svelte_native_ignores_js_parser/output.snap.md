# Exit code
0

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

  ! svelte(a11y_invalid_attribute): 'javascript:void(0)' is not a valid href attribute
  | https://svelte.dev/e/a11y_invalid_attribute
   ,-[files/App.svelte:5:1]
 4 | 
 5 | <a href="javascript:void(0)">unsafe</a>
   : ^^^^^^^^^^^^^^^^^^^|^^^^^^^^^^^^^^^^^^^
   :                    `-- Svelte compiler warning
   `----

Found 2 warnings and 0 errors.
Finished in Xms on 1 file using X threads.
```

# stderr
```
```
