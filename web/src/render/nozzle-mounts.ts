// Mounts that join each thruster nozzle to the car body (SPEC §11.4), so no nozzle
// floats in the air. The kind of mount follows the way the nozzle's gas leaves:
// sideways nozzles sit on a fin-shaped pylon rising from the bonnet or deck, downward
// nozzles on a pad under the body, and rearward nozzles on a plate on the tail.
// Pylons are painted like the body, so they read as part of it; pads and plates are trim.
// Rendering only: the nozzle positions come from the sim's layout and are never moved.
import { BoxGeometry, ExtrudeGeometry, Mesh, MeshStandardMaterial, Shape, Vector3 } from 'three';

import type { NozzlePlace } from '../nozzle-layout.ts';
import { BODY_BOTTOM_M, BODY_WIDTH_M, bodyTopHeightM, paint } from './car-mesh.ts';

/** Dark trim, so pads and plates read as hardware against the paint. */
const MOUNT_COLOUR = 0x2b2f36;

/** Pylon length along the car, and half the height of its flat outer face. */
const PYLON_LENGTH_M = 0.3;
const PYLON_FACE_HALF_HEIGHT_M = 0.1;
/** How far in from its outer face the pylon's top reaches. */
const PYLON_TOP_DEPTH_M = 0.2;
/** Where the pylon's foot starts on the body, as a distance from the centre line. */
const PYLON_FOOT_INNER_M = 0.6;
/** The pylon's foot sinks this far into the body, so no gap shows on a sloped bonnet. */
const PYLON_FOOT_SINK_M = 0.08;

/** Pad under the body for a downward nozzle: half its size along the car and across it. */
const PAD_HALF_LENGTH_M = 0.12;
const PAD_HALF_WIDTH_M = 0.13;
/** How far the pad hangs below the body's underside. */
const PAD_DROP_M = 0.05;

/** Plate on the tail for a rearward nozzle: half its height and width, and thickness. */
const PLATE_HALF_SIZE_M = 0.14;
const PLATE_THICKNESS_M = 0.04;

const trimMaterial = new MeshStandardMaterial({
  color: MOUNT_COLOUR,
  flatShading: true,
  roughness: 0.7,
});

/** A nozzle's mount, and where on it the nozzle sits. */
export interface NozzleMount {
  /** The mount, in the car body frame. */
  mesh: Mesh;
  /**
   * Distance from the nozzle's physics position to the mount's outer face, along the
   * exhaust, in metres. The nozzle's narrow end sits there, flush on the mount.
   */
  faceOffsetM: number;
}

/** Builds the mount for one nozzle, chosen by the main axis of its exhaust. */
export function buildNozzleMount(place: NozzlePlace): NozzleMount {
  const exhaust = new Vector3().fromArray(place.exhaust);
  const [forwardM, upM, rightM] = place.positionM;
  const sideways = Math.abs(exhaust.z);
  const vertical = Math.abs(exhaust.y);
  if (sideways >= vertical && sideways >= Math.abs(exhaust.x)) {
    return { mesh: buildSidePylon(forwardM, upM, rightM), faceOffsetM: 0 };
  }
  if (vertical >= Math.abs(exhaust.x)) {
    return buildUnderPad(forwardM, upM, rightM);
  }
  return buildTailPlate(place.positionM, exhaust);
}

/**
 * A fin seen from the front: its foot spans the body's top edge, it leans out to a flat
 * outer face around the nozzle, and its top runs back in. Extruded along the car.
 */
function buildSidePylon(forwardM: number, upM: number, rightM: number): Mesh {
  const side = Math.sign(rightM) || 1;
  const faceM = Math.abs(rightM);
  const footM = bodyTopHeightM(forwardM) - PYLON_FOOT_SINK_M;
  const faceBottomM = upM - PYLON_FACE_HALF_HEIGHT_M;
  const faceTopM = upM + PYLON_FACE_HALF_HEIGHT_M;

  // Points are [distance out from the centre line, height]; `side` puts them left or right.
  const outline: [number, number][] = [
    [PYLON_FOOT_INNER_M, footM],
    [BODY_WIDTH_M / 2, footM],
    [faceM, faceBottomM],
    [faceM, faceTopM],
    [faceM - PYLON_TOP_DEPTH_M, faceTopM],
  ];
  const shape = new Shape();
  outline.forEach(([outM, heightM], index) => {
    if (index === 0) {
      shape.moveTo(side * outM, heightM);
    } else {
      shape.lineTo(side * outM, heightM);
    }
  });
  shape.closePath();

  const geometry = new ExtrudeGeometry(shape, { depth: PYLON_LENGTH_M, bevelEnabled: false });
  // The outline is drawn in the shape's XY plane: turn its X into the car's Z (across)
  // and the extrusion into the car's X (along), centred on the nozzle.
  geometry.translate(0, 0, -PYLON_LENGTH_M / 2);
  geometry.rotateY(-Math.PI / 2);
  geometry.translate(forwardM, 0, 0);
  return new Mesh(geometry, paint);
}

/** A flat block from inside the body down to just below its underside. */
function buildUnderPad(forwardM: number, upM: number, rightM: number): NozzleMount {
  const topM = Math.max(upM, BODY_BOTTOM_M) + PAD_DROP_M;
  const faceM = BODY_BOTTOM_M - PAD_DROP_M;
  const geometry = new BoxGeometry(2 * PAD_HALF_LENGTH_M, topM - faceM, 2 * PAD_HALF_WIDTH_M);
  const mesh = new Mesh(geometry, trimMaterial);
  mesh.position.set(forwardM, (topM + faceM) / 2, rightM);
  return { mesh, faceOffsetM: upM - faceM };
}

/** A thin square plate behind the nozzle, its back face at the nozzle's position. */
function buildTailPlate(positionM: [number, number, number], exhaust: Vector3): NozzleMount {
  const geometry = new BoxGeometry(PLATE_THICKNESS_M, 2 * PLATE_HALF_SIZE_M, 2 * PLATE_HALF_SIZE_M);
  const mesh = new Mesh(geometry, trimMaterial);
  mesh.position.fromArray(positionM).addScaledVector(exhaust, PLATE_THICKNESS_M / 2);
  return { mesh, faceOffsetM: PLATE_THICKNESS_M };
}
