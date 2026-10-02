<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { TransformControls } from "three/addons/controls/TransformControls.js";
import { useBoardCalibration } from "../boardCalibration";
import { sensorToTrunk } from "../imuInstallation";
import { useJointPose } from "../jointPose";
import { createJointNode, applyJointAngle, smoothJointAngle } from "../jointModel";
import { servoHousing, servoHighlight } from "../servoSelection";
const board = useBoardCalibration();
const jointPose = useJointPose();
const jointNodes = new Map<number, { pivot: THREE.Group; axis: THREE.Vector3; angle: number }>();
const servoMaterials = new Map<number, THREE.MeshStandardMaterial>();
const highlight = servoHighlight();
let selectionMarker: THREE.Mesh | undefined;
function updateSelection() {
  highlight(servoMaterials.get(jointPose.selected));
  const pivot = jointNodes.get(jointPose.selected)?.pivot;
  if (selectionMarker && pivot) { pivot.add(selectionMarker); selectionMarker.position.set(0, 0, 0); }
}
watch(() => jointPose.selected, updateSelection);
const props = defineProps<{
  quaternion: number[] | null;
  paused: boolean;
  previewAngles?: Record<number, number>;
  installation?: { positionMm: number[]; quaternion: number[] };
  editMode?: 'translate' | 'rotate';
}>();
const emit = defineEmits<{ installationChange: [value: { positionMm: number[]; quaternion: number[] }] }>();
let marker: THREE.Group | undefined, transform: TransformControls | undefined;
function updateMarker() {
  if (!marker || transform?.dragging) return;
  marker.position.fromArray((props.installation?.positionMm ?? board.data?.mounting?.positionMm ?? [0,0,0]).map((n: number) => n/1000));
  marker.quaternion.fromArray(props.installation?.quaternion ?? sensorToTrunk(board.data?.imu, board.data?.mounting?.yaw ?? -90));
}
watch(() => [props.installation, board.data], updateMarker, { deep:true });
watch(() => props.editMode, mode => { if (transform && mode) { transform.setMode(mode); transform.setSpace(mode==='translate' ? 'world' : 'local'); } });
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
let grid: THREE.GridHelper;
function fitPreview() {
  if (!loaded.value || !props.previewAngles || !camera || !root) return;
  root.quaternion.copy(target);
  for (const [id, joint] of jointNodes) {
    joint.angle = props.previewAngles[id] ?? 0;
    applyJointAngle(joint.pivot, joint.axis, joint.angle);
  }
  root.updateMatrixWorld(true);
  const bounds = new THREE.Box3().setFromObject(root);
  if (bounds.isEmpty()) return;
  const sphere = bounds.getBoundingSphere(new THREE.Sphere());
  const halfVertical = THREE.MathUtils.degToRad(camera.fov / 2);
  const halfHorizontal = Math.atan(Math.tan(halfVertical) * camera.aspect);
  const distance = sphere.radius * 1.18 / Math.sin(Math.min(halfVertical, halfHorizontal));
  // Look down across the body rather than along the soles of a supine robot.
  const direction = new THREE.Vector3(.65, -1, 1.4).normalize();
  controls.target.copy(sphere.center);
  camera.position.copy(sphere.center).addScaledVector(direction, distance);
  controls.maxDistance = Math.max(2, distance * 2);
  grid.position.z = bounds.min.z - .01;
  controls.update();
}
if (props.quaternion) target.fromArray(props.quaternion).normalize();
watch(
  () => props.quaternion,
  (q) => {
    if (q && !props.paused) target.fromArray(q).normalize();
    fitPreview();
  },
);
watch(() => props.previewAngles, fitPreview, { deep: true });
function home() {
  if (props.previewAngles && loaded.value) { fitPreview(); return; }
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
    grid = new THREE.GridHelper(1.4, 28, 0xacc1b5, 0xdce5df);
    grid.rotation.x = Math.PI / 2;
    scene.add(grid);
    root = new THREE.Group();
    root.position.z = 0.12;
    scene.add(root);
    root.add(new THREE.AxesHelper(0.12));
    marker = new THREE.Group();
    const chip = new THREE.Mesh(new THREE.BoxGeometry(.023,.016,.005), new THREE.MeshStandardMaterial({color:0x16a6ac,emissive:0x083a3c,depthTest:false,transparent:true,opacity:.85}));
    chip.renderOrder=20; marker.add(chip);
    const axes = new THREE.AxesHelper(.045); axes.renderOrder=21;
    (axes.material as THREE.Material).depthTest=false;
    marker.add(axes);root.add(marker);updateMarker();
    if (props.editMode) {
      transform = new TransformControls(camera, renderer.domElement);
      transform.setMode(props.editMode); transform.setSpace(props.editMode==='translate'?'world':'local');
      transform.setSize(.8);transform.attach(marker);scene.add(transform.getHelper());
      transform.addEventListener('dragging-changed', event => { controls.enabled=!event.value; });
      transform.addEventListener('objectChange', () => {
        if (!marker) return;
        marker.position.clampScalar(-.3,.3);
        emit('installationChange',{positionMm:marker.position.toArray().map(n=>n*1000),quaternion:marker.quaternion.toArray()});
      });
    }
    observer = new ResizeObserver(() => {
      const w = host.value!.clientWidth,
        h = host.value!.clientHeight;
      if (w <= 0 || h <= 0) return;
      renderer.setSize(w, h);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      fitPreview();
    });
    observer.observe(host.value!);
    let lastFrame = performance.now();
    const render = () => {
      const now = performance.now();
      const dt = Math.min((now - lastFrame) / 1000, 0.1);
      lastFrame = now;
      if (!props.paused) {
        root.quaternion.slerp(target, 1 - Math.exp(-dt / 0.067));
        for (const [id, joint] of jointNodes) {
          joint.angle = smoothJointAngle(joint.angle, (props.previewAngles ?? jointPose.angles)[id], dt);
          applyJointAngle(joint.pivot, joint.axis, joint.angle);
        }
      }
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
      const { node, inner, axis } = createJointNode(body);
      if (body.joint && axis) jointNodes.set(body.joint.id, { pivot: inner, axis, angle: 0 });
      // Rotate about the trunk, not about the model's placement in the world.
      if (body.parent < 0) node.position.set(0, 0, 0);
      for (const [geomIndex, geom] of body.geoms.entries()) {
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
        inner.add(mesh);
        const id = Object.keys(servoHousing).map(Number).find(id => {
          const entry = servoHousing[id]!;
          return entry[0] === body.name && entry[1] === geomIndex;
        });
        if (id !== undefined) servoMaterials.set(id, mesh.material);
      }
      (body.parent < 0 ? root : nodes[body.parent]).add(node);
      nodes.push(inner);
    }
    selectionMarker = new THREE.Mesh(new THREE.SphereGeometry(.018, 12, 8), new THREE.MeshBasicMaterial({
      color: '#ffad32', wireframe: true, transparent: true, opacity: .55, depthTest: false, depthWrite: false,
    }));
    selectionMarker.renderOrder = 10;
    updateSelection();
    loaded.value = true;
    fitPreview();
  } catch (e) {
    problem.value = String(e);
  }
});
onBeforeUnmount(() => {
  disposed = true;
  cancelAnimationFrame(frame);
  observer?.disconnect();
  transform?.dispose();
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
