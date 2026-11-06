<script lang="ts">
  let { items = $bindable(), placeholder = "" }: { items: string[]; placeholder?: string } = $props();
</script>

<div class="list">
  {#each items as _, i}
    <div class="row">
      <input type="text" class="grow" {placeholder} bind:value={items[i]} />
      <button aria-label="Remove" onclick={() => items.splice(i, 1)}>−</button>
    </div>
  {/each}
  <div><button onclick={() => items.push("")}>Add</button></div>
</div>

<style>
  .list { display: flex; flex-direction: column; gap: 4px; }
</style>
