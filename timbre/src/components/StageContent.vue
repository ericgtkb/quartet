<script setup lang="ts">
// Lives inside <TresCanvas> so it can hook into the render loop.
import { useLoop } from '@tresjs/core'
import type { Frame } from '@/api/types'
import type { StageScene } from '@/stage/StageScene'
import { alphaFor } from '@/stage/interpolate'

const props = defineProps<{
  scene: StageScene
  buffer: { prev: Frame | null; curr: Frame | null; arrivedAt: number }
  beatMs: number
}>()

const { onBeforeRender } = useLoop()
onBeforeRender(({ elapsed }) => {
  const { prev, curr, arrivedAt } = props.buffer
  if (!curr) return
  props.scene.update(prev, curr, alphaFor(performance.now() - arrivedAt, props.beatMs), elapsed)
})
</script>

<template>
  <primitive :object="scene.group" />
</template>
