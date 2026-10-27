<script setup lang="ts">
const props = defineProps<{ message: string; count?: number }>();
const emit = defineEmits<{ change: [value: string] }>();
defineSlots<{ default(props: { label: string }): unknown }>();
</script>
<template><button @click="emit('change', props.message)"><slot :label="props.message" /></button></template>
