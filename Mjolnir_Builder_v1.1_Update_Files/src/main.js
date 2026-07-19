import './styles.css';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';

const app = document.querySelector('#app');
app.innerHTML = `<div class="app"><aside class="panel"><h1 class="brand">Mjolnir Modular Builder</h1><p class="sub">Assemble, preview and export a pre-coloured multi-plate Bambu 3MF.</p>
<div class="group"><label>Handle construction</label><select id="style"><option value="flat">Flat / smooth core</option><option value="simple">Simple printed-leather detail</option><option value="detailed" selected>Detailed printed-leather detail</option></select></div>
<div class="group"><label>Handle length</label><select id="length"><option value="170">Short comic — 170 mm</option><option value="195">Medium classic — 195 mm</option><option value="220">Long prop — 220 mm</option></select></div>
<div class="group"><label>Handle thickness</label><select id="thickness"></select></div>
<div class="group"><label>Lower end</label><select id="lower"><option value="pommel">Classic comic pommel</option><option value="no_pommel">No pommel</option></select></div>
<div class="group"><label>Wrist strap</label><select id="strap"><option value="none">None</option><option value="detailed_tpu">Detailed printable TPU</option><option value="plain_tpu">Plain printable TPU</option><option value="real">Real leather — preview only</option></select></div>
<div class="group"><label>Colours</label><div class="colors"><div><small>Metal</small><input id="silver" type="color" value="#a6a6a6"></div><div><small>Handle</small><input id="leather" type="color" value="#6b3e26"></div><div><small>Strap</small><input id="strapColor" type="color" value="#3e2115"></div></div></div>
<div class="group"><div id="summary" class="summary"></div><span class="badge">Bambu Lab P2S · 0.4 mm</span></div>
<button id="export" class="primary">Export chosen combination as 3MF</button><button id="reset" class="secondary">Reset camera</button><div id="status" class="status">Loading embedded model library…</div></aside><main class="viewport"><canvas id="canvas"></canvas><div class="hint">Drag to rotate · wheel to zoom · right-drag to pan</div></main></div>`;

const $ = (id) => document.getElementById(id);
const scene = new THREE.Scene();
const renderer = new THREE.WebGLRenderer({ canvas: $('canvas'), antialias: true, alpha: true });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;

const camera = new THREE.PerspectiveCamera(38, 1, 0.1, 2000);
camera.position.set(330, -380, 220);
const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;
controls.target.set(0, 0, -75);

scene.add(new THREE.HemisphereLight(0xffffff, 0x26313d, 2.4));
const key = new THREE.DirectionalLight(0xffffff, 3.0);
key.position.set(250, -200, 350);
scene.add(key);
const fill = new THREE.DirectionalLight(0x9ec7ff, 1.2);
fill.position.set(-220, 120, 80);
scene.add(fill);

const grid = new THREE.GridHelper(500, 20, 0x48535f, 0x29323c);
grid.rotation.x = Math.PI / 2;
grid.position.z = -250;
scene.add(grid);

const modelGroup = new THREE.Group();
scene.add(modelGroup);
let requestNumber = 0;

function combination() {
  return {
    handle_style: $('style').value,
    length: Number($('length').value),
    thickness: Number($('thickness').value),
    lower_end: $('lower').value,
    strap: $('strap').value,
  };
}

function colours() {
  return {
    silver: $('silver').value,
    leather: $('leather').value,
    strap: $('strapColor').value,
  };
}

function updateThickness() {
  const style = $('style').value;
  const values = style === 'flat' ? [25, 28, 31] : [31, 34, 37];
  const oldValue = Number($('thickness').value) || values[1];
  const suffix = style === 'flat' ? 'core' : 'max';
  const labels = ['Standard', 'Comic thick', 'Extra thick'];

  $('thickness').innerHTML = values
    .map((value, index) => `<option value="${value}">${labels[index]} — ${value} mm ${suffix}</option>`)
    .join('');
  $('thickness').value = values.includes(oldValue) ? oldValue : values[1];
}

function describe() {
  const chosen = combination();
  const styles = {
    flat: 'flat/smooth core',
    simple: 'simple printed-leather detail',
    detailed: 'detailed printed-leather detail',
  };
  const end = chosen.lower_end === 'pommel' ? 'classic comic pommel' : 'no pommel';
  const straps = {
    none: 'no strap',
    real: 'real leather strap preview',
    plain_tpu: 'plain TPU strap',
    detailed_tpu: 'detailed TPU strap',
  };

  $('summary').textContent = `${chosen.length} mm ${styles[chosen.handle_style]}, ${chosen.thickness} mm, ${end}, ${straps[chosen.strap]}.`;
}

function clearModel() {
  while (modelGroup.children.length) {
    const object = modelGroup.children.pop();
    object.geometry?.dispose();
    object.material?.dispose();
  }
}

async function refresh() {
  const thisRequest = ++requestNumber;
  describe();
  $('status').textContent = 'Updating preview…';

  try {
    const parts = await invoke('preview_combination', { combo: combination() });
    if (thisRequest !== requestNumber) return;

    clearModel();
    const chosenColours = colours();
    for (const part of parts) {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute('position', new THREE.Float32BufferAttribute(part.vertices, 3));
      geometry.setIndex(part.indices);
      geometry.computeVertexNormals();

      const material = new THREE.MeshStandardMaterial({
        color: chosenColours[part.material],
        roughness: part.material === 'silver' ? 0.42 : 0.78,
        metalness: part.material === 'silver' ? 0.48 : 0,
        side: THREE.DoubleSide,
      });
      modelGroup.add(new THREE.Mesh(geometry, material));
    }
    $('status').textContent = 'Ready.';
  } catch (error) {
    $('status').textContent = `Preview error: ${error}`;
  }
}

async function exportCombination() {
  try {
    const path = await save({
      defaultPath: 'Custom_Mjolnir_Combination.3mf',
      filters: [{ name: '3MF project', extensions: ['3mf'] }],
    });
    if (!path) return;

    $('status').textContent = 'Exporting 3MF…';
    const result = await invoke('export_combination', {
      path,
      combo: combination(),
      colours: colours(),
    });
    $('status').textContent = `Exported ${result.plates} build plates successfully.`;
  } catch (error) {
    $('status').textContent = `Export failed: ${error}`;
  }
}

function resize() {
  const container = renderer.domElement.parentElement;
  renderer.setSize(container.clientWidth, container.clientHeight, false);
  camera.aspect = container.clientWidth / container.clientHeight;
  camera.updateProjectionMatrix();
}

function animate() {
  requestAnimationFrame(animate);
  controls.update();
  renderer.render(scene, camera);
}

window.addEventListener('resize', resize);
resize();
animate();

$('style').addEventListener('change', () => {
  updateThickness();
  refresh();
});
['length', 'thickness', 'lower', 'strap', 'silver', 'leather', 'strapColor'].forEach((id) => {
  $(id).addEventListener('change', refresh);
});
$('export').addEventListener('click', exportCombination);
$('reset').addEventListener('click', () => {
  camera.position.set(330, -380, 220);
  controls.target.set(0, 0, -75);
  controls.update();
});

updateThickness();
refresh();
