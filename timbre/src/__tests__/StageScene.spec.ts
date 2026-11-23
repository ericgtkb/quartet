import { describe, expect, it } from 'vitest'
import { InstancedMesh, Matrix4, Vector3 } from 'three'
import { StageScene } from '@/stage/StageScene'
import type { StageColors } from '@/stage/colors'
import { makeFrame } from './fixtures'

const colors: StageColors = {
  background: '#000000',
  floor: '#111111',
  grid: '#222222',
  voices: ['#ff0000', '#00ff00', '#0000ff', '#ffff00'],
  mote: '#ffffff',
  static: '#ff00ff',
  chord: '#00ffff',
}

function meshes(scene: StageScene) {
  return scene.group.children.filter((c): c is InstancedMesh => c instanceof InstancedMesh)
}

describe('StageScene', () => {
  it('lays out every slot, mote, and static field', () => {
    const scene = new StageScene(colors)
    const frame = makeFrame(1, [10, 20, 30, 40])
    frame.quartetlings.singing[1] = 1
    scene.update(null, frame, 1, 0)

    const counts = meshes(scene).map((m) => m.count)
    // motes, statics, bodies, halos
    expect(counts).toEqual([2, 1, 4, 4])

    const bodies = meshes(scene)[2]!
    const m = new Matrix4()
    bodies.getMatrixAt(0, m)
    const p = new Vector3().setFromMatrixPosition(m)
    // World (10, 50) in a 100 × 100 arena maps to (-40, y, 0).
    expect(p.x).toBeCloseTo(-40)
    expect(p.z).toBeCloseTo(0)
    scene.dispose()
  })
})
