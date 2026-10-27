<script setup lang="ts">
import Child from './Child.vue';
const props = defineProps<{ title: string }>();
</script>
<template><Child :message="props.title" @change="value => value.toUpperCase()"><template #default="{ label }">{{ label.toUpperCase() }}</template></Child></template>
