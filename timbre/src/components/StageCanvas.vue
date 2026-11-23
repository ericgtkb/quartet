<script setup lang="ts">
import { computed, onBeforeUnmount, shallowRef, watch } from 'vue'
import { TresCanvas } from '@tresjs/core'
import { OrbitControls } from '@tresjs/cientos'
import type { Frame } from '@/api/types'
import StageContent from '@/components/StageContent.vue'
import { useTheme } from '@/composables/useTheme'
import { stageColors } from '@/stage/colors'
import { StageScene } from '@/stage/StageScene'

const props = defineProps<{
  buffer: { prev: Frame | null; curr: Frame | null; arrivedAt: number }
  beatMs: number
  /** Arena size, for framing the camera. */
  size: number
}>()

const colors = shallowRef(stageColors())
const scene = new StageScene(colors.value)
const { theme } = useTheme()
watch(theme, () => {
  // data-theme is already applied, so the tokens resolve to the new theme.
  colors.value = stageColors()
  scene.setColors(colors.value)
})
onBeforeUnmount(() => scene.dispose())

const cameraPosition = computed<[number, number, number]>(() => [
  0,
  props.size * 0.85,
  props.size * 0.8,
])
</script>

<template>
  <div class="stage-canvas">
    <TresCanvas :clear-color="colors.background">
      <TresPerspectiveCamera :position="cameraPosition" :look-at="[0, 0, 0]" :fov="45" />
      <OrbitControls
        :max-polar-angle="Math.PI / 2.15"
        :min-distance="10"
        :max-distance="size * 3"
      />
      <TresAmbientLight :intensity="1.4" />
      <TresDirectionalLight :position="[30, 80, 20]" :intensity="2.2" />
      <StageContent :scene="scene" :buffer="buffer" :beat-ms="beatMs" />
    </TresCanvas>
  </div>
</template>

<style scoped>
.stage-canvas {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 320px;
  border-radius: var(--radius-md);
  overflow: hidden;
  border: 1px solid var(--color-border);
}
</style>
