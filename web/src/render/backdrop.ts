// The arena's backdrop (SPEC §11.4): a big dome around the car, dark at the horizon and
// lighter overhead. Its horizon colour is the fog colour, so the floor fades into it with
// no visible edge. The colours follow world height, so they stay put when the camera tilts.
import { BackSide, BufferAttribute, Color, Mesh, MeshBasicMaterial, SphereGeometry } from 'three';

/** Colour at and below the horizon; also the fog colour and the page background. */
export const BACKDROP_HORIZON = '#121a24';
/** Colour straight overhead. */
const BACKDROP_TOP = '#5a7ea3';
/** Dome radius: inside the camera's far plane, far beyond the fog. */
export const BACKDROP_RADIUS_M = 1500;
/** How quickly the colour climbs from the horizon (below 1 brightens it sooner). */
const GRADIENT_CURVE = 0.6;

/** Builds the dome. Move it with the car each frame so the car stays at its centre. */
export function buildBackdropDome(): Mesh {
  const geometry = new SphereGeometry(BACKDROP_RADIUS_M, 32, 16);
  const horizon = new Color(BACKDROP_HORIZON);
  const top = new Color(BACKDROP_TOP);
  const positions = geometry.getAttribute('position');
  const colours = new Float32Array(positions.count * 3);
  const colour = new Color();
  for (let vertex = 0; vertex < positions.count; vertex += 1) {
    const height = Math.max(0, positions.getY(vertex) / BACKDROP_RADIUS_M);
    colour.lerpColors(horizon, top, height ** GRADIENT_CURVE);
    colour.toArray(colours, vertex * 3);
  }
  geometry.setAttribute('color', new BufferAttribute(colours, 3));
  // Seen from inside, never fogged, never shadowed: it is the far wall of the world.
  const material = new MeshBasicMaterial({ vertexColors: true, side: BackSide, fog: false });
  return new Mesh(geometry, material);
}
