import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue'

/** The element's content width, tracked with ResizeObserver (fallback when absent, e.g. jsdom). */
export function useWidth(el: Ref<HTMLElement | null>, fallback = 480) {
  const width = ref(fallback)
  let observer: ResizeObserver | null = null
  onMounted(() => {
    if (!el.value || typeof ResizeObserver === 'undefined') return
    observer = new ResizeObserver(([entry]) => {
      if (entry && entry.contentRect.width > 0) width.value = entry.contentRect.width
    })
    observer.observe(el.value)
  })
  onBeforeUnmount(() => observer?.disconnect())
  return width
}
