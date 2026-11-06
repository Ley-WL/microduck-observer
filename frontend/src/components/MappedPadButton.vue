<script setup lang="ts">
import { onMounted, onUnmounted, watch } from 'vue';
import { longPress } from '../longPress';
import { shortLongPress } from '../shortLongPress';
import { heldRoll } from '../heldRoll';
const props = defineProps<{ action: string; enabled: boolean; reset: number; longAction?: string }>();
const emit = defineEmits<{ action: [value: string] }>();
const choice=shortLongPress(()=>{if(props.enabled)emit('action',props.action);},()=>{if(props.enabled && props.longAction)emit('action',props.longAction);});
const roll=heldRoll(()=>emit('action','roulade'),()=>props.enabled && props.action==='roulade');
const hold = longPress(() => { if (props.enabled && props.action === 'getup') emit('action', props.action); }, 560);
function start(event: PointerEvent) { if (event.button !== 0 || !props.enabled || (!props.longAction && !['getup','roulade'].includes(props.action))) return; (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId); if(props.longAction)choice.start();else if(props.action==='roulade')roll.start();else hold.start(); }
function click(event:MouseEvent) { if (!props.longAction && props.enabled && props.action !== 'none' && props.action !== 'getup' && (props.action!=='roulade' || event.detail===0)) emit('action', props.action); }
function keydown(event: KeyboardEvent) { if ((!props.longAction && !['getup','roulade'].includes(props.action)) || ![' ', 'Enter'].includes(event.key)) return; event.preventDefault(); if (props.enabled && !event.repeat) {if(props.longAction)choice.start();else if(props.action==='roulade')roll.start();else hold.start();} }
function release(){choice.release();hold.cancel();roll.cancel();}
function keyup(event:KeyboardEvent) {if ((props.longAction || ['getup','roulade'].includes(props.action)) && [' ','Enter'].includes(event.key))event.preventDefault();release();}
function cancel() { choice.cancel();hold.cancel();roll.cancel(); }
function hidden() { if (document.hidden) cancel(); }
watch(() => [props.action, props.enabled, props.longAction], cancel);
watch(() => props.reset,()=>{if(props.action!=='roulade')cancel();});
onMounted(() => { window.addEventListener('blur', cancel); document.addEventListener('visibilitychange', hidden); });
onUnmounted(() => { cancel(); window.removeEventListener('blur', cancel); document.removeEventListener('visibilitychange', hidden); });
</script>
<template><button :disabled="!enabled || action === 'none'" @click="click" @pointerdown="start" @pointerup="release" @pointercancel="cancel" @lostpointercapture="cancel" @keydown="keydown" @keyup="keyup" @blur="cancel"><slot /></button></template>
