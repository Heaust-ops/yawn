<script setup>
import { computed } from "vue";

const props = defineProps({
  id: { type: String, required: true },
  title: { type: String, required: true },
  description: { type: String, default: "Run this recipe against the real Yawn worker." },
});

const query = computed(() => encodeURIComponent(props.id));
</script>

<template>
  <section class="yawn-playground">
    <div class="yawn-playground__header">
      <div>
        <span>LIVE PLAYGROUND</span>
        <strong>{{ title }}</strong>
        <p>{{ description }}</p>
      </div>
      <a :href="`/playground/?recipe=${query}`">Edit and run ↗</a>
    </div>
    <ClientOnly>
      <iframe
        :src="`/playground/runner.html?recipe=${query}&embed=1`"
        :title="`${title} live preview`"
        loading="lazy"
      />
    </ClientOnly>
  </section>
</template>
