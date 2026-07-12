<svelte:options runes={false} />

<script>
  const props = $$props;
  const rest = $$restProps;
  const slots = $$slots;
</script>

<p>{props.id} {rest.id} {slots.default}</p>
