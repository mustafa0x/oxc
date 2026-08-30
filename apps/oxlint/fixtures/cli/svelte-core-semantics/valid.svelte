<script module>
  const moduleValue = 1;
</script>

<script lang="ts">
  import Widget from './Widget.svelte';
  import { writable } from 'svelte/store';

  const count = writable(0);
  const current = $count;
  const repeated = $count ? ($count as number) : 0;
  const state = $state(moduleValue);
  const local = configuredGlobal;
  $effect(() => void state);
</script>

<Widget value={current + repeated + state + local} />
