<script setup lang="ts">
import { computed } from "vue"

import type { Feedback } from "../auth-submit"

const props = defineProps<{
  feedback: Feedback
}>()

const className = computed(() => {
  if (!props.feedback) {
    return ""
  }

  if (props.feedback.tone === "success") {
    return "notice-success"
  }

  if (props.feedback.tone === "error") {
    return "notice-error"
  }

  return "notice-info"
})
</script>

<template>
  <div v-if="feedback" :class="className">
    {{ feedback.message }}
  </div>
</template>
