<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { longPress } from '../longPress';
const props = defineProps<{ action: string; enabled: boolean; reset: number }>();
const emit = defineEmits<{ action: [value: string] }>();
const hold = longPress(() => { if (props.enabled && props.action === 'getup') emit('action', props.action); }, 560);
function start(event: PointerEvent) { if (event.button !== 0 || !props.enabled || props.action !== 'getup') return; (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId); hold.start(); }
function click() { if (props.enabled && props.action !== 'none' && props.action !== 'getup') emit('action', props.action); }
function keydown(event: KeyboardEvent) { if (props.action !== 'getup' || ![' ', 'Enter'].includes(event.key)) return; event.preventDefault(); if (props.enabled && !event.repeat) hold.start(); }
function cancel() { hold.cancel(); }
function hidden() { if (document.hidden) cancel(); }
watch(() => [props.action, props.enabled, props.reset], cancel);
onMounted(() => { window.addEventListener('blur', cancel); document.addEventListener('visibilitychange', hidden); });
onUnmounted(() => { cancel(); window.removeEventListener('blur', cancel); document.removeEventListener('visibilitychange', hidden); });
</script>
<template><button :disabled="!enabled || action === 'none'" @click="click" @pointerdown="start" @pointerup="cancel" @pointercancel="cancel" @lostpointercapture="cancel" @keydown="keydown" @keyup="cancel" @blur="cancel"><slot /></button></template>
