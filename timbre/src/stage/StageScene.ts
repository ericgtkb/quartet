import {
  CircleGeometry,
  Color,
  ConeGeometry,
  DynamicDrawUsage,
  Group,
  InstancedMesh,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshStandardMaterial,
  Object3D,
  PlaneGeometry,
  RingGeometry,
  SphereGeometry,
  type Material,
  type BufferGeometry,
} from 'three'
import type { Frame } from '@/api/types'
import type { StageColors } from './colors'
import { poseAt } from './interpolate'

/** Body radius per voice, matching harmony's base `dynamics`. */
const VOICE_SIZE = [0.7, 0.9, 1.1, 1.3] as const

const MAX_MOTES = 4096
const MAX_STATICS = 32

/**
 * The 3D stage, managed imperatively for speed: a few instanced meshes whose
 * per-instance matrices are rewritten every rendered frame. World (x, y) maps
 * to three's (x, 0, z), centered on the arena.
 */
export class StageScene {
  readonly group = new Group()

  // Unlit, so the arena reads clearly against the page background.
  private readonly floorMaterial = new MeshBasicMaterial()
  private readonly floor = new Mesh(new PlaneGeometry(1, 1), this.floorMaterial)
  private readonly bodyMaterial = new MeshStandardMaterial({ roughness: 0.4, metalness: 0.1 })
  private readonly haloMaterial = new MeshBasicMaterial({ transparent: true, opacity: 0.55 })
  private readonly moteMaterial = new MeshStandardMaterial({ emissiveIntensity: 0.8 })
  private readonly staticMaterial = new MeshBasicMaterial({ transparent: true, opacity: 0.18 })

  private bodies: InstancedMesh | null = null
  private halos: InstancedMesh | null = null
  private readonly motes: InstancedMesh
  private readonly statics: InstancedMesh
  private readonly geometries: BufferGeometry[] = []
  private readonly dummy = new Object3D()
  private readonly hidden = new Matrix4().makeScale(0, 0, 0)
  private colors: StageColors
  private size = { width: 0, height: 0 }

  constructor(colors: StageColors) {
    this.colors = colors
    this.floor.rotation.x = -Math.PI / 2
    this.group.add(this.floor)

    const mote = this.track(new SphereGeometry(0.35, 8, 6))
    this.motes = new InstancedMesh(mote, this.moteMaterial, MAX_MOTES)
    this.motes.instanceMatrix.setUsage(DynamicDrawUsage)
    this.motes.count = 0
    this.group.add(this.motes)

    const disc = this.track(new CircleGeometry(1, 48))
    disc.rotateX(-Math.PI / 2)
    this.statics = new InstancedMesh(disc, this.staticMaterial, MAX_STATICS)
    this.statics.count = 0
    this.group.add(this.statics)

    this.setColors(colors)
  }

  private track<G extends BufferGeometry>(geometry: G): G {
    this.geometries.push(geometry)
    return geometry
  }

  /** (Re)build per-slot meshes when the slot count changes. */
  private ensureSlots(slots: number) {
    if (this.bodies?.count === slots) return
    for (const mesh of [this.bodies, this.halos]) {
      if (mesh) {
        this.group.remove(mesh)
        mesh.dispose()
      }
    }
    // A cone pointing along +x: the quartetling's heading.
    const body = this.track(new ConeGeometry(1, 2.4, 12))
    body.rotateZ(-Math.PI / 2)
    this.bodies = new InstancedMesh(body, this.bodyMaterial, slots)
    this.bodies.instanceMatrix.setUsage(DynamicDrawUsage)
    const ring = this.track(new RingGeometry(1.6, 2.1, 32))
    ring.rotateX(-Math.PI / 2)
    this.halos = new InstancedMesh(ring, this.haloMaterial, slots)
    this.halos.instanceMatrix.setUsage(DynamicDrawUsage)
    this.group.add(this.bodies, this.halos)
    this.recolorSlots()
  }

  private recolorSlots() {
    if (!this.bodies || !this.halos) return
    const color = new Color()
    for (let i = 0; i < this.bodies.count; i++) {
      // Slots cycle through the voices (harmony assigns voice = slot % 4).
      color.set(this.colors.voices[i % 4]!)
      this.bodies.setColorAt(i, color)
    }
    if (this.bodies.instanceColor) this.bodies.instanceColor.needsUpdate = true
  }

  setColors(colors: StageColors) {
    this.colors = colors
    this.floorMaterial.color.set(colors.floor)
    this.moteMaterial.color.set(colors.mote)
    this.moteMaterial.emissive.set(colors.mote)
    this.staticMaterial.color.set(colors.static)
    this.haloMaterial.color.set(colors.chord)
    this.recolorSlots()
  }

  /** Lay out the frame, interpolating quartetlings `alpha` of the way from `prev`. */
  update(prev: Frame | null, curr: Frame, alpha: number, elapsed: number) {
    const q = curr.quartetlings
    const slots = q.id.length
    this.ensureSlots(slots)
    const bodies = this.bodies!
    const halos = this.halos!

    if (curr.width !== this.size.width || curr.height !== this.size.height) {
      this.size = { width: curr.width, height: curr.height }
      this.floor.scale.set(curr.width, curr.height, 1)
    }
    const ox = curr.width / 2
    const oz = curr.height / 2
    const d = this.dummy

    for (let i = 0; i < slots; i++) {
      const pose = poseAt(prev, curr, i, alpha)
      const size = VOICE_SIZE[q.voice[i] ?? 0]
      const singing = q.singing[i] === 1
      // Singers swell slightly with a pulse; chord members pulse together.
      const pulse = singing ? 1 + 0.12 * Math.sin(elapsed * 10 + (q.inChord[i] ? 0 : i)) : 1
      d.position.set(pose.x - ox, size, pose.y - oz)
      d.rotation.set(0, -pose.heading, 0)
      d.scale.setScalar(size * pulse)
      d.updateMatrix()
      bodies.setMatrixAt(i, d.matrix)

      if (singing) {
        d.position.y = 0.05
        d.rotation.set(0, 0, 0)
        d.scale.setScalar(size * (q.inChord[i] ? 1.6 : 1))
        d.updateMatrix()
        halos.setMatrixAt(i, d.matrix)
      } else {
        halos.setMatrixAt(i, this.hidden)
      }
    }
    bodies.instanceMatrix.needsUpdate = true
    halos.instanceMatrix.needsUpdate = true

    const motes = Math.min(curr.motes.x.length, MAX_MOTES)
    for (let i = 0; i < motes; i++) {
      d.position.set(
        curr.motes.x[i]! - ox,
        0.6 + 0.15 * Math.sin(elapsed * 3 + i),
        curr.motes.y[i]! - oz,
      )
      d.rotation.set(0, 0, 0)
      d.scale.setScalar(1)
      d.updateMatrix()
      this.motes.setMatrixAt(i, d.matrix)
    }
    this.motes.count = motes
    this.motes.instanceMatrix.needsUpdate = true

    const statics = Math.min(curr.statics.x.length, MAX_STATICS)
    for (let i = 0; i < statics; i++) {
      d.position.set(curr.statics.x[i]! - ox, 0.02, curr.statics.y[i]! - oz)
      d.rotation.set(0, 0, 0)
      d.scale.setScalar(curr.statics.radius[i]!)
      d.updateMatrix()
      this.statics.setMatrixAt(i, d.matrix)
    }
    this.statics.count = statics
    this.statics.instanceMatrix.needsUpdate = true
  }

  dispose() {
    for (const g of this.geometries) g.dispose()
    this.floor.geometry.dispose()
    const materials: Material[] = [
      this.floorMaterial,
      this.bodyMaterial,
      this.haloMaterial,
      this.moteMaterial,
      this.staticMaterial,
    ]
    for (const m of materials) m.dispose()
    for (const mesh of [this.bodies, this.halos, this.motes, this.statics]) mesh?.dispose()
  }
}
