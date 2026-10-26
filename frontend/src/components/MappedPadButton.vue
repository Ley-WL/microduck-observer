<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { longPress } from '../longPress';
import { heldRoll } from '../heldRoll';
const props = defineProps<{ action: string; enabled: boolean; reset: number }>();
const emit = defineEmits<{ action: [value: string] }>();
const roll=heldRoll(()=>emit('action','roulade'),()=>props.enabled && props.action==='roulade');
const hold = longPress(() => { if (props.enabled && props.action === 'getup') emit('action', props.action); }, 560);
function start(event: PointerEvent) { if (event.button !== 0 || !props.enabled || !['getup','roulade'].includes(props.action)) return; (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId); if(props.action==='roulade')roll.start();else hold.start(); }
function click(event:MouseEvent) { if (props.enabled && props.action !== 'none' && props.action !== 'getup' && (props.action!=='roulade' || event.detail===0)) emit('action', props.action); }
function keydown(event: KeyboardEvent) { if (!['getup','roulade'].includes(props.action) || ![' ', 'Enter'].includes(event.key)) return; event.preventDefault(); if (props.enabled && !event.repeat) {if(props.action==='roulade')roll.start();else hold.start();} }
function keyup(event:KeyboardEvent) {if (['getup','roulade'].includes(props.action) && [' ','Enter'].includes(event.key))event.preventDefault();cancel();}
function cancel() { hold.cancel();roll.cancel(); }
function hidden() { if (document.hidden) cancel(); }
watch(() => [props.action, props.enabled], cancel);
watch(() => props.reset,()=>{if(props.action!=='roulade')cancel();});
onMounted(() => { window.addEventListener('blur', cancel); document.addEventListener('visibilitychange', hidden); });
onUnmounted(() => { cancel(); window.removeEventListener('blur', cancel); document.removeEventListener('visibilitychange', hidden); });
</script>
<template><button :disabled="!enabled || action === 'none'" @click="click" @pointerdown="start" @pointerup="cancel" @pointercancel="cancel" @lostpointercapture="cancel" @keydown="keydown" @keyup="keyup" @blur="cancel"><slot /></button></template>
