// The 3D viewport: scene, camera controls, truck assembly from placements,
// and screen projection of hotspot anchors.

import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoomEnvironment } from "three/addons/environments/RoomEnvironment.js";
import type { Placement, Scene, TruckView, Vec3 } from "../api";
import { AssetCache, detectSupport, type ModelAsset } from "./assets";

export interface ProjectedAnchor {
  slot: string;
  x: number;
  y: number;
  /** False when the anchor is behind the camera or on the far side of the truck. */
  facing: boolean;
}

type ProjectionListener = (anchors: ProjectedAnchor[]) => void;

/** Placements are diffed by this key so unchanged parts are not rebuilt. */
function placementKey(p: Placement): string {
  return [p.model, p.look, p.variant, ...p.translation, ...p.rotation, p.scale].join("|");
}

export class TruckViewer {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera = new THREE.PerspectiveCamera(40, 1, 0.1, 500);
  private readonly controls: OrbitControls;
  private readonly assets: AssetCache;
  private readonly truck = new THREE.Group();
  private readonly sceneModel = new THREE.Group();
  private readonly ambient = new THREE.HemisphereLight(0xdde6ff, 0x30302a, 0.6);
  private readonly sun = new THREE.DirectionalLight(0xffffff, 1.6);
  private readonly fallbackFloor: THREE.Mesh;
  /** True while the scene supplies the camera: the orbit target is not re-centred. */
  private sceneCamera = false;
  private sceneGeneration = 0;
  private readonly instances = new Map<string, THREE.Object3D>();
  private anchors: { slot: string; position: THREE.Vector3 }[] = [];
  private center = new THREE.Vector3(0, 1.5, 0);
  private paintable = new Set<THREE.MeshStandardMaterial>();
  private paintColor = new THREE.Color(0.8, 0.8, 0.8);
  private listener: ProjectionListener | null = null;
  private frame = 0;
  private dirty = true;
  private readonly resizeObserver: ResizeObserver;
  private generation = 0;

  constructor(private readonly container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    container.appendChild(this.renderer.domElement);
    this.assets = new AssetCache(detectSupport(this.renderer));

    const environment = new THREE.PMREMGenerator(this.renderer).fromScene(new RoomEnvironment(), 0.04);
    this.scene.environment = environment.texture;
    this.scene.background = new THREE.Color(0x15181d);
    // Defaults until a game scene is applied, and fallbacks for any piece
    // the scene does not specify.
    this.sun.position.set(-6, 10, -8);
    this.fallbackFloor = new THREE.Mesh(
      new THREE.CircleGeometry(14, 64),
      new THREE.MeshStandardMaterial({ color: 0x23272e, roughness: 0.9 }),
    );
    this.fallbackFloor.rotation.x = -Math.PI / 2;
    this.scene.add(this.ambient, this.sun, this.fallbackFloor, this.sceneModel, this.truck);

    // SCS trucks face -z; start with a front three-quarter view.
    this.camera.position.set(-6.5, 3.2, -8.5);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.target.copy(this.center);
    this.controls.enableDamping = true;
    this.controls.minDistance = 2.5;
    this.controls.maxDistance = 25;
    this.controls.maxPolarAngle = Math.PI * 0.495;
    this.controls.addEventListener("change", () => (this.dirty = true));

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();
    this.loop();
  }

  /** Called with projected anchors whenever the camera or truck changes. */
  onProjection(listener: ProjectionListener | null): void {
    this.listener = listener;
    this.dirty = true;
  }

  /** Brings the scene in line with `view`; resolves when every model loaded. */
  async show(view: TruckView): Promise<string[]> {
    const generation = ++this.generation;
    const wanted = new Map(view.placements.map((p) => [placementKey(p), p]));
    for (const [key, object] of this.instances) {
      if (!wanted.has(key)) {
        this.truck.remove(object);
        this.instances.delete(key);
      }
    }
    const errors: string[] = [];
    await Promise.all(
      [...wanted].map(async ([key, placement]) => {
        if (this.instances.has(key)) return;
        try {
          const asset = await this.assets.model(placement.model, placement.look, placement.variant);
          if (generation !== this.generation) return;
          this.addInstance(key, placement, asset);
        } catch (error) {
          errors.push(`${placement.model}: ${String(error)}`);
        }
      }),
    );
    if (generation !== this.generation) return errors;
    this.setPaint(view.paintColor);
    this.anchors = view.hotspots.map((h) => ({ slot: h.slot, position: new THREE.Vector3(...h.position) }));
    this.frameTruck();
    this.dirty = true;
    return errors;
  }

  private addInstance(key: string, placement: Placement, asset: ModelAsset): void {
    const group = new THREE.Group();
    for (const mesh of asset.meshes) group.add(mesh.clone());
    group.position.set(...placement.translation);
    group.quaternion.set(...placement.rotation);
    group.scale.setScalar(placement.scale);
    group.userData = { slot: placement.slot };
    for (const material of asset.paintable) {
      this.paintable.add(material);
      material.color.copy(this.paintColor);
    }
    this.truck.add(group);
    this.instances.set(key, group);
  }

  private setPaint(color: Vec3 | null): void {
    // Game colours are linear (e.g. Coffee Brown is (0.034, 0.023, 0.014)).
    this.paintColor = color ? new THREE.Color().setRGB(...color, THREE.LinearSRGBColorSpace) : new THREE.Color(0.6, 0.6, 0.6);
    for (const material of this.paintable) material.color.copy(this.paintColor);
  }

  /**
   * Applies a game tuning scene (finding F7). Each piece falls back
   * independently: a missing camera keeps the current one, a garage model
   * that fails to load keeps the plain floor. Returns load errors.
   */
  async setScene(scene: Scene): Promise<string[]> {
    const generation = ++this.sceneGeneration;
    const errors: string[] = [];

    if (scene.camera) {
      this.camera.position.set(...scene.camera.position);
      this.camera.fov = scene.camera.fov;
      this.camera.updateProjectionMatrix();
      this.center.set(...scene.camera.aim);
      this.controls.target.copy(this.center);
      this.sceneCamera = true;
    }
    if (scene.lightAmbient) {
      const [r, g, b] = scene.lightAmbient;
      this.ambient.color.setRGB(r, g, b);
      this.ambient.groundColor.setRGB(r * 0.5, g * 0.5, b * 0.5);
      this.ambient.intensity = 1.5;
    }
    if (scene.lightDiffuse) {
      const peak = Math.max(...scene.lightDiffuse, 1e-3);
      const [r, g, b] = scene.lightDiffuse;
      this.sun.color.setRGB(r / peak, g / peak, b / peak);
      this.sun.intensity = peak;
    }
    if (scene.lightDirection) {
      // The definition gives the direction light travels; three.js places
      // the light at a position shining towards its target (the origin).
      this.sun.position.copy(new THREE.Vector3(...scene.lightDirection).normalize().multiplyScalar(-20));
    }
    if (scene.lightEnvironment !== null) {
      // RoomEnvironment stands in for the game's cube reflection map.
      this.scene.environmentIntensity = Math.min(scene.lightEnvironment * 0.5, 2);
    }
    this.scene.fog =
      scene.fogColor && scene.fogDensity !== null
        ? new THREE.FogExp2(new THREE.Color(...scene.fogColor), scene.fogDensity)
        : null;
    this.truck.position.set(...scene.truckPosition);
    const [rx, ry, rz] = scene.truckRotation;
    this.truck.rotation.set(
      THREE.MathUtils.degToRad(rx),
      THREE.MathUtils.degToRad(ry),
      THREE.MathUtils.degToRad(rz),
    );

    this.sceneModel.clear();
    this.fallbackFloor.visible = true;
    if (scene.model) {
      const { path, look, variant, parts, position } = scene.model;
      try {
        const asset = await this.assets.model(path, look, variant, parts);
        if (generation !== this.sceneGeneration) return errors;
        const group = new THREE.Group();
        for (const mesh of asset.meshes) group.add(mesh.clone());
        group.position.set(...position);
        this.sceneModel.add(group);
        this.fallbackFloor.visible = false;
      } catch (error) {
        errors.push(`scene model ${path}: ${String(error)}`);
      }
    }
    this.dirty = true;
    return errors;
  }

  /** Centres the orbit target on the truck the first time it appears,
   * unless a game scene supplies the camera. */
  private frameTruck(): void {
    if (this.sceneCamera) return;
    const box = new THREE.Box3().setFromObject(this.truck);
    if (box.isEmpty()) return;
    const center = box.getCenter(new THREE.Vector3());
    if (center.distanceTo(this.center) > 0.5) {
      this.center.copy(center);
      this.controls.target.copy(center);
    }
  }

  private resize(): void {
    const { clientWidth: width, clientHeight: height } = this.container;
    if (width === 0 || height === 0) return;
    this.renderer.setSize(width, height);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.dirty = true;
  }

  private loop = (): void => {
    this.frame = requestAnimationFrame(this.loop);
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
    if (this.dirty) {
      this.dirty = false;
      this.emitProjection();
    }
  };

  private emitProjection(): void {
    if (!this.listener) return;
    const width = this.container.clientWidth;
    const height = this.container.clientHeight;
    this.truck.updateMatrixWorld();
    const truckCenter = new THREE.Vector3(0, 1.5, 0).applyMatrix4(this.truck.matrixWorld);
    const toCamera = new THREE.Vector3().subVectors(this.camera.position, truckCenter).normalize();
    const projected = this.anchors.map(({ slot, position: local }) => {
      // Anchors are in truck space; the scene may move or rotate the truck.
      const position = local.clone().applyMatrix4(this.truck.matrixWorld);
      const ndc = position.clone().project(this.camera);
      // Hide anchors on the far side of the truck (no depth test against
      // the meshes; see docs/RENDERING.md).
      const outward = position.clone().sub(truckCenter);
      outward.y = 0;
      const sideFacing = outward.lengthSq() < 0.04 || outward.normalize().dot(toCamera) > -0.15;
      return {
        slot,
        x: ((ndc.x + 1) / 2) * width,
        y: ((1 - ndc.y) / 2) * height,
        facing: ndc.z < 1 && sideFacing,
      };
    });
    this.listener(projected);
  }

  dispose(): void {
    cancelAnimationFrame(this.frame);
    this.resizeObserver.disconnect();
    this.controls.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}
