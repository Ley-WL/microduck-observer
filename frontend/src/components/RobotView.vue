<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
const props = defineProps<{
  quaternion: number[] | null;
  paused: boolean;
}>();
const host = ref<HTMLDivElement>(),
  problem = ref(""),
  loaded = ref(false);
let renderer: THREE.WebGLRenderer,
  controls: OrbitControls,
  camera: THREE.PerspectiveCamera;
let scene: THREE.Scene,
  root: THREE.Group,
  frame = 0,
  observer: ResizeObserver,
  disposed = false;
const target = new THREE.Quaternion();
if (props.quaternion) target.fromArray(props.quaternion).normalize();
watch(
  () => props.quaternion,
  (q) => {
    if (q && !props.paused) target.fromArray(q).normalize();
  },
);
function home() {
  // The model faces +X; use a front view with a 20-degree azimuth offset.
  const azimuth = THREE.MathUtils.degToRad(20);
  const distance = 0.69;
  camera?.position.set(distance * Math.cos(azimuth), -distance * Math.sin(azimuth), 0.32);
  controls?.target.set(0, 0, 0.12);
  controls?.update();
}
defineExpose({ home });
onMounted(async () => {
  try {
    scene = new THREE.Scene();
    camera = new THREE.PerspectiveCamera(36, 1, 0.005, 20);
    camera.up.set(0, 0, 1);
    renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
    renderer.setClearColor(0, 0);
    host.value!.appendChild(renderer.domElement);
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.minDistance = 0.25;
    controls.maxDistance = 2;
    home();
    scene.add(new THREE.HemisphereLight(0xffffff, 0x658979, 2.6));
    const key = new THREE.DirectionalLight(0xffffff, 3);
    key.position.set(1, -2, 3);
    scene.add(key);
    const grid = new THREE.GridHelper(1.4, 28, 0xacc1b5, 0xdce5df);
    grid.rotation.x = Math.PI / 2;
    scene.add(grid);
    root = new THREE.Group();
    root.position.z = 0.12;
    scene.add(root);
    root.add(new THREE.AxesHelper(0.12));
    observer = new ResizeObserver(() => {
      const w = host.value!.clientWidth,
        h = host.value!.clientHeight;
      renderer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    });
    observer.observe(host.value!);
    const render = () => {
      if (!props.paused) root.quaternion.slerp(target, 0.22);
      controls.update();
      renderer.render(scene, camera);
      frame = requestAnimationFrame(render);
    };
    render();
    const [modelResponse, binResponse] = await Promise.all([
      fetch("/model/model.json"),
      fetch("/model/meshes.bin"),
    ]);
    if (!modelResponse.ok || !binResponse.ok) throw new Error("模型文件不可用");
    const model = await modelResponse.json(),
      bin = await binResponse.arrayBuffer();
    if (disposed) return;
    const geometries: Record<string, THREE.BufferGeometry> = {};
    for (const [name, value] of Object.entries(model.meshes)) {
      const m = value as any,
        quantized = new Int16Array(bin, m.voff, m.nverts * 3),
        positions = new Float32Array(m.nverts * 3);
      for (let i = 0; i < m.nverts; i++)
        for (let k = 0; k < 3; k++)
          positions[i * 3 + k] =
            m.bbox_min[k] +
            ((quantized[i * 3 + k] + 32768) / 65535) *
              (m.bbox_max[k] - m.bbox_min[k] || 1);
      const indices =
        m.index_bytes === 2
          ? new Uint16Array(bin, m.ioff, m.ntris * 3)
          : new Uint32Array(bin, m.ioff, m.ntris * 3);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.BufferAttribute(positions, 3));
      g.setIndex(new THREE.BufferAttribute(indices, 1));
      g.computeVertexNormals();
      geometries[name] = g;
    }
    const nodes: THREE.Group[] = [];
    const quat = (v: number[]) => new THREE.Quaternion(v[1], v[2], v[3], v[0]);
    for (const body of model.bodies) {
      const node = new THREE.Group();
      node.position.fromArray(body.pos);
      node.quaternion.copy(quat(body.quat));
      // Rotate about the trunk, not about the model's placement in the world.
      if (body.parent < 0) node.position.set(0, 0, 0);
      for (const geom of body.geoms) {
        const mesh = new THREE.Mesh(
          geometries[geom.mesh],
          new THREE.MeshStandardMaterial({
            color: new THREE.Color(
              ...(geom.rgba.slice(0, 3) as [number, number, number]),
            ),
            roughness: 0.65,
            metalness: 0.1,
            transparent: geom.rgba[3] < 1,
            opacity: geom.rgba[3],
          }),
        );
        mesh.position.fromArray(geom.pos);
        mesh.quaternion.copy(quat(geom.quat));
        node.add(mesh);
      }
      (body.parent < 0 ? root : nodes[body.parent]).add(node);
      nodes.push(node);
    }
    loaded.value = true;
  } catch (e) {
    problem.value = String(e);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  cancelAnimationFrame(frame);
  observer?.disconnect();
  controls?.dispose();
  scene?.traverse((o) => {
    if (o instanceof THREE.Mesh) {
      o.geometry.dispose();
      for (const m of Array.isArray(o.material) ? o.material : [o.material])
        m.dispose();
    }
  });
  renderer?.dispose();
});
</script>
<template>
  <div ref="host" class="robot-canvas">
    <div v-if="problem" class="canvas-message">3D 无法加载 · {{ problem }}</div>
    <div v-else-if="!loaded" class="canvas-message">正在加载机械模型…</div>
  </div>
</template>
