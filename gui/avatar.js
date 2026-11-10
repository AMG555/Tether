// The 3D companion renderer.
//
// Consumes gui/avatar-math.js (all the timing/pose maths) and gui/vendor/three-bundle.js
// (vendored three.js, so nothing is fetched from a CDN). Two body plans share one skinning
// skeleton:
//
//   biped   hips -> spine -> chest -> neck -> head, plus 2 arms and 2 legs
//   quadruped  root -> spine -> chest -> neck -> head, plus 4 legs and a tail
//
// Everything is built from primitives and skinned to that chain, which is why a procedural
// character can play the same walk / sit / wave / celebrate clips that a downloaded VRM plays.
// A real VRM replaces `buildProceduralCharacter()` and keeps every line below it.

import * as THREE from './vendor/three-bundle.js';
import { GLTFLoader, MeshoptDecoder } from './vendor/three-bundle.js';
import {
  clamp, damp, gaitPhase, swingAngle, footLift, bodyBob, breathe, twoBoneIK,
  blinkAmount, attentiveEye, wanderPosition, lookAtDelta, placeBubble,
  POSE_WEIGHTS, blendPose, chooseState, hexToRgb01,
} from './avatar-math.js';

/** Rejects NaN/Infinity from any pose source. A single NaN rotation deletes a limb. */
const safeNum = (v, fallback = 0) => (typeof v === 'number' && Number.isFinite(v) ? v : fallback);

export class Companion {
  constructor({ mount, desktopMode = false }) {
    this.mount = mount;
    this.desktopMode = desktopMode;
    this.time = 0;
    this.pose = { ...POSE_WEIGHTS.idle };
    this.targetPose = { ...POSE_WEIGHTS.idle };
    this.state = 'idle';
    this.sinceStateChange = 0;
    this.sinceReminder = 99;
    this.sincePet = 99;
    this.celebrating = 0;
    this.sleeping = false;
    this.dragged = false;
    this.walkEnabled = true;
    this.seed = 1;
    this.bubble = null;
    this.character = null;
    this.ready = false;
  }

  async init() {
    this.THREE = THREE;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(34, 1, 0.1, 100);
    // Look at chest height and stand back far enough to include the feet and a little air
    // above the head. Framing is half of why a low-poly figure looks like a toy vs a character.
    this.focusY = 0.78;
    camera.position.set(0, this.focusY + 0.16, 4.6);
    camera.lookAt(0, this.focusY, 0);

    const renderer = new THREE.WebGLRenderer({
      antialias: true,
      alpha: this.desktopMode,
      premultipliedAlpha: false,
      powerPreference: 'low-power', // a desktop pet must not spin up a gaming GPU
    });
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    if (this.desktopMode) renderer.setClearAlpha(0);
    renderer.shadowMap.enabled = true;
    // r187 removed PCFSoftShadowMap; PCF is the softest option still in core.
    renderer.shadowMap.type = THREE.PCFShadowMap;
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    renderer.toneMappingExposure = 1.05;
    this.mount.appendChild(renderer.domElement);

    // Three-point lighting: key with a soft shadow, cool rim for separation from the wallpaper,
    // and a warm fill. This is what makes a low-poly figure read as a figure.
    const key = new THREE.DirectionalLight(0xfff4e6, 2.1);
    key.position.set(2.4, 4.2, 3.2);
    key.castShadow = true;
    key.shadow.mapSize.set(1024, 1024);
    key.shadow.camera.near = 0.5;
    key.shadow.camera.far = 14;
    key.shadow.camera.left = -2.5;
    key.shadow.camera.right = 2.5;
    key.shadow.camera.top = 3;
    key.shadow.camera.bottom = -1;
    key.shadow.bias = -0.0012;
    scene.add(key);

    const rim = new THREE.DirectionalLight(0x9fd7ff, 1.15);
    rim.position.set(-3, 2.2, -3.4);
    scene.add(rim);

    const fill = new THREE.HemisphereLight(0xffffff, 0x2a2440, 0.75);
    scene.add(fill);

    const hemi = new THREE.AmbientLight(0xffffff, 0.22);
    scene.add(hemi);

    // A shadow-catcher floor that is invisible in desktop mode (transparent framebuffer).
    if (!this.desktopMode) {
      const floor = new THREE.Mesh(
        new THREE.CircleGeometry(3.2, 48),
        new THREE.MeshStandardMaterial({ color: 0x1a1630, roughness: 0.95, metalness: 0 }),
      );
      floor.rotation.x = -Math.PI / 2;
      floor.receiveShadow = true;
      floor.position.y = 0;
      scene.add(floor);
    }

    this.scene = scene;
    this.camera = camera;
    this.renderer = renderer;
    this.ready = true;
    this.resize();
    window.addEventListener('resize', () => this.resize());
    return this;
  }

  resize() {
    const w = this.mount.clientWidth || window.innerWidth;
    const h = this.mount.clientHeight || window.innerHeight;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    // Fit the character vertically with headroom, and back off further still on portrait
    // windows so a tall, narrow window does not crop the feet.
    const fovRad = (this.camera.fov * Math.PI) / 180;
    let spanH = 2.0;
    let spanW = 1.55;
    let centerY = this.focusY;
    if (this.character?.root) {
      // Fit whatever is actually there, with a margin. Never assume a body plan.
      const box = new THREE.Box3().setFromObject(this.character.root);
      if (Number.isFinite(box.min.y) && Number.isFinite(box.max.y)) {
        const size = new THREE.Vector3();
        box.getSize(size);
        spanH = Math.max(0.4, size.y * 1.28);
        spanW = Math.max(0.4, Math.max(size.x, size.z * this.camera.aspect) * 1.5);
        centerY = (box.min.y + box.max.y) / 2;
      }
    }
    this.focusY = centerY;
    const distH = spanH / 2 / Math.tan(fovRad / 2);
    const distW = spanW / 2 / Math.tan(fovRad / 2) / Math.max(0.2, w / h);
    this.camera.position.z = Math.max(distH, distW);
    this.camera.position.y = centerY + spanH * 0.04;
    this.camera.lookAt(0, centerY, 0);
    this.camera.updateProjectionMatrix();
  }

  // ---------------------------------------------------------------- character construction
  /**
   * Builds a procedural character.
   *
   * Proportion policy: every part is a fraction of one number, `H` (total height in world units).
   * The rig is then rescaled to a constant on-screen size, so the camera never needs to know
   * which species it is looking at. Bipeds and quadrupeds are built by separate paths because
   * they share almost no geometry: a biped stacks along Y, a quadruped extends along Z.
   */
  buildCharacter(config) {
    const THREE = this.THREE;
    if (this.character) {
      this.scene.remove(this.character.root);
      this.disposeTree(this.character.root);
      this.character = null;
    }
    const { species = 'cat', palette = {}, body = {}, seed = 1 } = config || {};
    this.seed = seed;

    const biped = species === 'human' || species === 'blob';
    const isHuman = species === 'human';
    const c3 = (hex, fb) => new THREE.Color().setRGB(...hexToRgb01(hex, fb));

    const mat = (color, opts = {}) =>
      new THREE.MeshStandardMaterial({ color, roughness: 0.72, metalness: 0.02, ...opts });
    const matSkin = mat(c3(palette.skin, [0.85, 0.65, 0.45]));
    const matHair = mat(c3(palette.hair, [0.15, 0.11, 0.08]), { roughness: 0.88 });
    const matTop = mat(c3(palette.top, [0.4, 0.3, 0.9]));
    const matBottom = mat(c3(palette.bottom, [0.12, 0.14, 0.2]));
    const matAccent = mat(c3(palette.accent, [0.9, 0.4, 0.7]));
    const matDark = mat(new THREE.Color(0x18151f), { roughness: 0.9 });
    const matWhite = mat(new THREE.Color(0xf7f7fa), { roughness: 0.35 });

    const H = clamp(body.height ?? 1.0, 0.15, 2.2);
    const headRatio = clamp(body.head_ratio ?? (biped ? 0.22 : 0.235), 0.08, 0.6);
    const shoulderW = clamp(body.shoulder ?? 0.22, 0.08, 0.5);
    const limbR = clamp(body.limb ?? 0.06, 0.015, 0.2);

    const root = new THREE.Group();
    const rig = new THREE.Group();
    const TARGET = 1.4;                    // constant on-screen height, independent of H
    rig.scale.setScalar(TARGET / H);
    root.add(rig);

    const sph = (r, w = 18, h = 14) => new THREE.SphereGeometry(r, w, h);
    const capsule = (r, len) => new THREE.CapsuleGeometry(Math.max(0.002, r), Math.max(0.002, len - r * 2), 4, 10);
    const mk = (geo, material, parent, x = 0, y = 0, z = 0) => {
      const m = new THREE.Mesh(geo, material);
      m.position.set(x, y, z);
      parent.add(m);
      return m;
    };

    const ctx = { root, rig, biped, isHuman, H, headRatio, shoulderW, limbR, mats: { matSkin, matHair, matTop, matBottom, matAccent, matDark, matWhite }, sph, capsule, mk };
    const parts = biped ? this.buildBiped(ctx) : this.buildQuadruped(ctx, species);

    root.traverse((o) => { if (o.isMesh) { o.castShadow = true; o.receiveShadow = true; } });

    this.character = {
      root, rig, biped, H, height: H, procedural: true,
      mats: ctx.mats,
      ...parts,
    };
    this.scene.add(root);
    this.resize();
    return this.character;
  }

  /** Human / blob: a figure stacked along Y. */
  buildBiped(ctx) {
    const THREE = this.THREE;
    const { rig, H, headRatio, shoulderW, limbR, mats, sph, capsule, mk, isHuman } = ctx;
    const { matSkin, matHair, matTop, matBottom, matAccent, matDark, matWhite } = mats;

    // ~6.5 heads tall: legs 48.5%, torso 27.5%, head 22%.
    const LEG = H * 0.485;
    const TORSO = H * 0.275;
    const NECK = isHuman ? H * 0.035 : 0;
    const headR = (H * headRatio) / 2;
    const shoulder = H * shoulderW;
    const hipW = shoulder * 0.62;

    const torso = new THREE.Group();
    torso.position.y = LEG + TORSO * 0.5;
    rig.add(torso);

    // Tapered torso: wide at the shoulders, narrow at the waist. A single scaled sphere cannot
    // do this, and it is most of why a figure reads as assembled rather than inflated.
    const chest = mk(new THREE.CylinderGeometry(shoulder * 0.5, hipW * 0.5, TORSO, 20, 1), matTop, torso);
    chest.scale.z = 0.62;
    mk(sph(shoulder * 0.5, 18, 12), matTop, torso, 0, TORSO * 0.5, 0).scale.set(1, 0.42, 0.62);
    mk(sph(hipW * 0.46, 18, 12), matBottom, torso, 0, -TORSO * 0.46, 0).scale.set(1, 0.62, 0.6);
    if (isHuman) {
      mk(new THREE.CylinderGeometry(hipW * 0.52, hipW * 0.52, TORSO * 0.07, 18), matDark, torso, 0, -TORSO * 0.42, 0).scale.z = 0.64;
    }

    // --- neck + head
    const headPivot = new THREE.Group();
    headPivot.position.y = TORSO * 0.5;
    torso.add(headPivot);
    if (NECK > 0) {
      mk(new THREE.CylinderGeometry(headR * 0.38, headR * 0.46, NECK * 1.9, 12), matSkin, headPivot, 0, NECK * 0.8, 0);
    }
    const head = new THREE.Group();
    head.position.y = NECK * 1.9 + headR * 0.88;
    headPivot.add(head);
    mk(sph(headR, 24, 18), matSkin, head).scale.set(1, 1.05, 0.96);

    if (isHuman) {
      // Hair as a partial spherical shell. The theta sweep stops above the brow, and a second
      // half-cap under it covers the nape: a full sphere would bury the face.
      mk(new THREE.SphereGeometry(headR * 1.05, 24, 16, 0, Math.PI * 2, 0, Math.PI * 0.40), matHair, head)
        .scale.set(1, 1.04, 0.99);
      mk(new THREE.SphereGeometry(headR * 1.04, 20, 14, 0, Math.PI, 0, Math.PI * 0.62), matHair, head)
        .rotateY(Math.PI); // open side faces forward
      mk(sph(headR * 0.56, 16, 12), matHair, head, 0, headR * 0.52, headR * 0.4).scale.set(1.35, 0.42, 0.8);
    } else {
      // Blob: bald by design, with an antenna instead of hair.
      mk(new THREE.CylinderGeometry(headR * 0.045, headR * 0.045, headR * 0.75, 8), matDark, head, 0, headR * 1.2, 0);
      mk(sph(headR * 0.15, 12, 10), matAccent, head, 0, headR * 1.58, 0);
    }

    // --- eyes, set into the skull so they read as sockets rather than googly eyes
    const eyes = [];
    for (const s of [-1, 1]) {
      const eye = new THREE.Group();
      eye.position.set(s * headR * 0.33, headR * 0.04, headR * 0.83);
      mk(sph(headR * 0.185, 16, 14), matWhite, eye).castShadow = false;
      mk(sph(headR * 0.105, 12, 10), matDark, eye, 0, 0, headR * 0.105).scale.set(1, 1.15, 0.6);
      head.add(eye);
      eyes.push(eye);
    }
    if (isHuman) {
      for (const s of [-1, 1]) {
        mk(new THREE.BoxGeometry(headR * 0.3, headR * 0.05, headR * 0.06), matHair, head, s * headR * 0.33, headR * 0.3, headR * 0.85)
          .rotation.z = -s * 0.08;
      }
    }

    // --- arms
    const limbs = [];
    const armLen = H * 0.36;
    const armR = H * limbR * 0.8;
    for (const s of [-1, 1]) {
      const shoulderJoint = new THREE.Group();
      // Outside the torso radius, or the arms render inside the chest and disappear.
      shoulderJoint.position.set(s * (shoulder * 0.46 + armR * 0.5), TORSO * 0.36, 0);
      torso.add(shoulderJoint);
      mk(sph(armR * 1.18, 14, 12), matTop, shoulderJoint, 0, -armR * 0.15, 0).scale.set(1.05, 1.25, 1.05);
      mk(capsule(armR, armLen * 0.55), matTop, shoulderJoint, 0, -armLen * 0.26, 0);
      const elbow = new THREE.Group();
      elbow.position.y = -armLen * 0.52;
      shoulderJoint.add(elbow);
      mk(capsule(armR * 0.88, armLen * 0.48), matSkin, elbow, 0, -armLen * 0.22, 0);
      mk(sph(armR * 1.2, 12, 10), matSkin, elbow, 0, -armLen * 0.48, 0).scale.set(1, 1.15, 0.7);
      limbs.push({ kind: 'arm', side: s, shoulderJoint, elbow, length: armLen });
    }

    // --- legs
    const legR = H * limbR * 1.02;
    const legLen = LEG * 0.9;
    for (const s of [-1, 1]) {
      const hip = new THREE.Group();
      hip.position.set(s * hipW * 0.3, -TORSO * 0.42, 0);
      torso.add(hip);
      mk(sph(legR, 14, 12), matBottom, hip);           // hip blend hides the seam
      mk(capsule(legR, legLen * 0.56), matBottom, hip, 0, -legLen * 0.27, 0);
      const knee = new THREE.Group();
      knee.position.y = -legLen * 0.54;
      hip.add(knee);
      mk(sph(legR * 0.98, 14, 12), matBottom, knee);
      mk(capsule(legR * 0.85, legLen * 0.44), matBottom, knee, 0, -legLen * 0.21, 0);
      const foot = mk(sph(legR * 1.0, 14, 12), matDark, knee, 0, -legLen * 0.45, legR * 0.62);
      foot.scale.set(1.0, 0.55, 1.65);
      limbs.push({ kind: 'leg', side: s, hip, knee, foot, length: legLen });
    }

    return { torso, head, headPivot, eyes, limbs, tail: null, ears: [] };
  }

  /**
   * Cat / dog: a body along Z, head forward, four legs down, tail back.
   *
   * A quadruped is not a biped rotated 90°: the torso is horizontal, the legs are arranged in
   * two axle pairs, and the head projects forward of the chest. Building it from the biped
   * proportions produces a cone with legs, which is exactly what happened the first time.
   */
  buildQuadruped(ctx, species) {
    const { rig, H, headRatio, limbR, mats, sph, capsule, mk } = ctx;
    const { matSkin, matHair, matAccent, matDark, matWhite } = mats;

    const bodyLen = H * 0.62;
    const bodyR = H * 0.20;
    const legLen = H * 0.37;
    const bodyY = legLen + bodyR * 0.72;      // shoulder height
    const headR = (H * headRatio) / 2;
    const legR = H * limbR * 0.80;

    // --- body
    const torso = new THREE.Group();
    torso.position.y = bodyY;
    rig.add(torso);
    // Barrel: a capsule laid along Z, slightly tapered at the hips via a scaled sphere.
    const barrel = mk(new THREE.CapsuleGeometry(bodyR, bodyLen - bodyR * 2, 8, 18), matSkin, torso);
    barrel.rotation.x = Math.PI / 2;
    barrel.scale.set(1, 0.97, 1);
    // No extra bulges: overlapping spheres cut visible seams through each other's surface.
    // A single barrel with a slight haunch scale keeps the silhouette clean.

    // --- neck + head, projecting forward
    const headPivot = new THREE.Group();
    headPivot.position.set(0, bodyR * 0.55, bodyLen * 0.4);
    torso.add(headPivot);
    mk(new THREE.CylinderGeometry(headR * 0.55, headR * 0.7, headR * 1.1, 12), matSkin, headPivot, 0, headR * 0.28, headR * 0.25)
      .rotation.x = -0.5;
    const head = new THREE.Group();
    head.position.set(0, headR * 0.78, headR * 0.86);
    headPivot.add(head);
    mk(sph(headR, 22, 18), matSkin, head).scale.set(1, 0.98, 1.02);

    // Muzzle + nose.
    mk(sph(headR * 0.52, 16, 12), matSkin, head, 0, -headR * 0.24, headR * 0.72).scale.set(1.2, 0.82, 0.95);
    mk(sph(headR * 0.1, 10, 8), matDark, head, 0, -headR * 0.18, headR * 1.14).scale.set(1.4, 0.9, 1);

    if (species === 'cat') {
      for (const s of [-1, 1]) {
        const ear = mk(new THREE.ConeGeometry(headR * 0.34, headR * 0.62, 4), matSkin, head, s * headR * 0.46, headR * 0.8, -headR * 0.1);
        ear.rotation.z = s * 0.14;
      }
      for (const s of [-1, 1]) {
        for (let i = 0; i < 2; i++) {
          const w = mk(new THREE.CylinderGeometry(headR * 0.012, headR * 0.012, headR * 0.85, 4), matWhite, head,
            s * headR * 0.48, -headR * 0.1 + i * headR * 0.12, headR * 0.76);
          w.rotation.z = Math.PI / 2;
          w.rotation.y = s * 0.3;
          w.castShadow = false;
        }
      }
    } else {
      for (const s of [-1, 1]) {
        const ear = mk(sph(headR * 0.42, 14, 12), matHair, head, s * headR * 0.88, headR * 0.16, -headR * 0.02);
        ear.scale.set(0.34, 1.15, 0.7);
        ear.rotation.z = s * 0.5;
        ear.userData.floppy = true;
      }
      mk(new THREE.TorusGeometry(headR * 0.6, headR * 0.075, 8, 20), matAccent, headPivot, 0, headR * 0.52, headR * 0.3)
        .rotation.x = 1.05;
    }

    const eyes = [];
    for (const s of [-1, 1]) {
      const eye = new THREE.Group();
      eye.position.set(s * headR * 0.38, headR * 0.1, headR * 0.78);
      mk(sph(headR * 0.19, 14, 12), matWhite, eye).castShadow = false;
      mk(sph(headR * 0.1, 12, 10), matDark, eye, 0, 0, headR * 0.11).scale.set(1, 1.1, 0.65);
      head.add(eye);
      eyes.push(eye);
    }

    // --- four legs, two axle pairs
    const limbs = [];
    const quarters = [[-1, 1], [1, 1], [-1, -1], [1, -1]];
    quarters.forEach(([sx, sz], i) => {
      const hip = new THREE.Group();
      const front = sz > 0;
      hip.position.set(sx * bodyR * 0.66, -bodyR * 0.5, (front ? 1 : -1) * bodyLen * 0.3);
      torso.add(hip);
      mk(sph(legR * 1.15, 12, 10), matSkin, hip);      // shoulder / haunch
      mk(capsule(legR, legLen * 0.55), matSkin, hip, 0, -legLen * 0.27, 0);
      const knee = new THREE.Group();
      knee.position.y = -legLen * 0.54;
      hip.add(knee);
      mk(capsule(legR * 0.82, legLen * 0.4), matSkin, knee, 0, -legLen * 0.19, 0);
      const paw = mk(sph(legR * 1.1, 12, 10), matSkin, knee, 0, -legLen * 0.4, legR * 0.4);
      paw.scale.set(1, 0.6, 1.45);
      limbs.push({ kind: 'leg', side: sx, front, index: i, hip, knee, paw, length: legLen });
    });

    // --- tail: three segments so the wag travels along its length
    const tailRoot = new THREE.Group();
    tailRoot.position.set(0, bodyR * 0.66, -bodyLen * 0.44);
    torso.add(tailRoot);
    const tail = [];
    let parent = tailRoot;
    for (let i = 0; i < 3; i++) {
      const seg = new THREE.Group();
      seg.position.set(0, i === 0 ? 0 : bodyR * 0.58, 0);
      parent.add(seg);
      const r = bodyR * (0.2 - i * 0.045);
      mk(capsule(Math.max(0.004, r), bodyR * 0.82), species === 'cat' ? matSkin : matHair, seg, 0, bodyR * 0.32, 0);
      tail.push(seg);
      parent = seg;
    }

    return { torso, head, headPivot, eyes, limbs, tail, ears: head.children.filter((c) => c.userData.floppy) };
  }

  disposeTree(obj) {
    obj.traverse?.((o) => {
      o.geometry?.dispose?.();
      if (Array.isArray(o.material)) o.material.forEach((m) => m.dispose?.());
      else o.material?.dispose?.();
    });
  }

  /** Swap in a real VRM/glTF avatar. Keeps the same state machine and speech bubble. */
  async loadGltf(url) {
    const loader = new GLTFLoader();
    loader.setMeshoptDecoder(MeshoptDecoder);
    const gltf = await loader.loadAsync(url);
    if (this.character) this.scene.remove(this.character.root);

    const root = gltf.scene;
    root.traverse((o) => {
      if (o.isMesh) {
        o.castShadow = true;
        o.receiveShadow = true;
        if (o.material) o.material.side = THREE.FrontSide;
      }
    });
    // Fit to roughly the same screen height as the procedural figure.
    const box = new THREE.Box3().setFromObject(root);
    const size = new THREE.Vector3();
    box.getSize(size);
    const target = 1.6;
    if (size.y > 0) {
      const s = target / size.y;
      root.scale.setScalar(s);
    }
    root.position.y = 0;
    this.scene.add(root);
    this.character = { root, gltf, mixer: new THREE.AnimationMixer(root), procedural: false };
    return this.character;
  }

  // ------------------------------------------------------------------------ state plumbing
  setReminder(text) {
    this.sinceReminder = 0;
    this.bubble = { text, age: 0, ttl: 9 };
  }

  celebrate(text) {
    this.celebrating = 1.8;
    this.bubble = { text, age: 0, ttl: 6 };
  }

  setSleeping(v) {
    this.sleeping = !!v;
  }

  setWalking(v) {
    this.walkEnabled = !!v;
  }

  setDragged(v) {
    this.dragged = !!v;
    if (v) this.sincePet = 0;
  }

  /** Advance and render one frame. `dt` in seconds. */
  update(dt) {
    if (!this.ready || !this.character) return;
    const c = this.character;
    if (!c.procedural && c.mixer) {
      c.mixer.update(dt);
      return;
    }
    this.time += dt;
    const t = this.time;
    if (this.celebrating > 0) this.celebrating -= dt;
    this.sinceReminder += dt;
    this.sincePet += dt;
    if (this.bubble) {
      this.bubble.age += dt;
      if (this.bubble.age > this.bubble.ttl) this.bubble = null;
    }

    const walking = this.walkEnabled && !this.dragged && !this.sleeping;
    const next = chooseState({
      reminderActive: this.sinceReminder < 7,
      celebrating: this.celebrating > 0,
      sleeping: this.sleeping,
      walking,
      sincePet: this.sincePet,
      dragged: this.dragged,
    });
    if (next !== this.state) {
      this.state = next;
      this.sinceStateChange = 0;
    } else {
      this.sinceStateChange += dt;
    }
    if (!this._channelsChecked) {
      // Fail loudly and early if the pose weight table and the renderer have drifted apart.
      const required = ['breathe', 'sway', 'swing', 'sit', 'arm', 'tail', 'eyes'];
      const missing = required.filter((k) => !(k in POSE_WEIGHTS.idle));
      if (missing.length) {
        throw new Error(`POSE_WEIGHTS is missing channels: ${missing.join(', ')}`);
      }
      this._channelsChecked = true;
    }
    this.targetPose = POSE_WEIGHTS[this.state] || POSE_WEIGHTS.idle;
    for (const k of Object.keys(this.pose)) {
      const target = safeNum(this.targetPose[k], 0);
      this.pose[k] = damp(safeNum(this.pose[k], 0), target, 0.12, dt);
    }
    const p = this.pose;

    // Root: wander across the desktop when walking, otherwise stand still.
    const wander = wanderPosition(t, this.seed, -1, 1);
    const targetX = this.dragged ? c.root.position.x : (walking ? wander.x * 0.42 : c.root.position.x * 0.98);
    c.root.position.x = damp(c.root.position.x, targetX, 0.5, dt);
    const facing = walking ? wander.facing : 1;
    c.rig.rotation.y = damp(c.rig.rotation.y, facing > 0 ? 0.35 : -0.35, 0.4, dt);

    // Sit: lower the whole rig and fold the legs.
    const sitDrop = p.sit * c.height * 0.19 * (c.biped ? 1 : 0.55);
    c.rig.position.y = -sitDrop + bodyBob(t, 1, 0.012 * p.swing);

    // Breathing and idle sway. Scaling the torso group scales its children too, which is why
    // the shoulders rise with the chest instead of the head detaching.
    const breath = breathe(t, 0.22) * 0.022 * p.breathe;
    c.torso.scale.y = 1 + breath;
    c.torso.scale.x = 1 - breath * 0.4;
    c.torso.rotation.z = Math.sin(t * 0.9) * 0.022 * p.sway;
    c.headPivot.rotation.z = Math.sin(t * 0.9) * -0.02 * p.sway;

    // Head: look at the cursor/reminder bubble, plus a slow idle drift.
    const lookTarget = this.sinceReminder < 7 ? 0.35 : Math.sin(t * 0.37) * 0.25;
    c.head.rotation.y = damp(c.head.rotation.y, lookTarget, 0.28, dt);
    c.head.rotation.x = damp(c.head.rotation.x, p.sit * 0.16 + Math.sin(t * 0.5) * 0.015, 0.3, dt);

    // Eyes: blink, and close fully while asleep.
    const blink = blinkAmount(t, this.seed);
    const openness = clamp(1 - blink, 0, 1) * clamp(p.eyes, 0, 1);
    for (const eye of c.eyes) {
      eye.scale.y = Math.max(0.04, openness);
    }

    // Floppy ears swing with motion — cheap, and it sells "alive".
    for (const ear of c.ears) {
      ear.rotation.x = Math.sin(t * 3.1 + ear.position.x) * 0.12 + p.swing * 0.25;
    }

    // Limbs.
    for (const l of c.limbs) {
      if (l.kind === 'leg') {
        const idx = c.biped ? (l.side < 0 ? 0 : 1) : l.index;
        const n = c.biped ? 2 : 4;
        const phase = gaitPhase(t, 1, idx, n);
        const swing = swingAngle(phase) * p.swing;
        const lift = footLift(phase) * p.swing;

        if (c.biped) {
          const sitFold = p.sit * 1.15;
          l.hip.rotation.x = swing * 0.8 - sitFold * 0.9;
          l.knee.rotation.x = sitFold * 1.7 + lift * 1.6;
          l.knee.position.y = -l.length * 0.55;
          l.foot.rotation.x = -sitFold * 0.6;
        } else {
          const sitFold = p.sit * 0.9;
          l.hip.rotation.x = swing - sitFold * 0.7;
          l.knee.rotation.x = lift * 2 - sitFold * 0.4;
        }
      } else {
        // Arms: swing opposite the legs when walking; raise when waving/celebrating.
        const phase = gaitPhase(t, 1, l.side < 0 ? 1 : 0, 2);
        const armSwing = -swingAngle(phase) * 0.45 * p.arm;
        const wave = this.state === 'wave' ? (l.side > 0 ? 1 : 0.2) : 0;
        const cheer = this.state === 'celebrate' ? 1 : 0;
        const raise = Math.max(wave, cheer) * p.arm;
        l.shoulderJoint.rotation.z = l.side * (-0.02 + raise * 1.95) + Math.sin(t * 6) * 0.09 * raise;
        l.shoulderJoint.rotation.x = armSwing + Math.cos(t * 7) * 0.12 * raise;
        l.elbow.rotation.x = -0.22 - raise * 0.7 + Math.sin(t * 5 + l.side) * 0.05;
      }
    }

    // Tail: a slow wave, faster when happy.
    if (c.tail) {
      const amp = 0.25 + p.tail * 0.5;
      const speed = this.state === 'celebrate' ? 9 : 2.4;
      c.tail.forEach((seg, i) => {
        seg.rotation.z = Math.sin(t * speed - i * 0.7) * amp * 0.35;
        seg.rotation.x = -0.25 + Math.sin(t * speed * 0.5 - i) * 0.1 * amp;
      });
    }
  }

  render() {
    if (!this.ready) return;
    this.renderer.render(this.scene, this.camera);
  }

  /** Screen-space position of the head, for anchoring the speech bubble. */
  headScreenPosition() {
    if (!this.character) return { x: 0, y: 0 };
    const THREE = this.THREE;
    const v = new THREE.Vector3();
    this.character.head.getWorldPosition(v);
    v.y += this.character.headR + 0.18;
    v.project(this.camera);
    const w = this.mount.clientWidth || window.innerWidth;
    const h = this.mount.clientHeight || window.innerHeight;
    return { x: ((v.x + 1) / 2) * w, y: ((1 - v.y) / 2) * h };
  }

  dispose() {
    if (this.character) {
      this.scene.remove(this.character.root);
      this.character.root.traverse?.((o) => {
        o.geometry?.dispose?.();
        if (Array.isArray(o.material)) o.material.forEach((m) => m.dispose?.());
        else o.material?.dispose?.();
      });
      this.character = null;
    }
    if (this.renderer) {
      this.renderer.dispose();
      this.renderer.domElement.remove();
    }
    this.ready = false;
  }
}
