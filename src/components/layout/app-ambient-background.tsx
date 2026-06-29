import { useEffect, useRef, useState } from "react";
import { Mesh, Program, Renderer, Triangle } from "ogl";
import { useTheme } from "next-themes";

type SideRaysOrigin = "top-right" | "top-left" | "bottom-right" | "bottom-left";

// 主题相关的可覆盖项（颜色 + animate）。其余着色器参数为单一真相，统一来自下方
// 的 *_CONFIG 常量，不再在组件签名上重复一套默认值。
interface SoftAuroraProps {
  animate?: boolean;
  color1?: string;
  color2?: string;
}

interface SideRaysProps {
  animate?: boolean;
  rayColor1?: string;
  rayColor2?: string;
}

type Vec2 = [number, number];
type Vec3 = [number, number, number];

// 单一配置来源：SoftAurora 的全部着色器参数（含此前由组件签名默认值提供的
// noiseFrequency），组件内部直接读取，颜色可由 props 覆盖。
const AURORA_CONFIG = {
  speed: 0.6,
  scale: 1.5,
  brightness: 1,
  color1: "#f7f7f7",
  color2: "#22d3ee",
  noiseFrequency: 2.35,
  noiseAmplitude: 1,
  bandHeight: 0.5,
  bandSpread: 1,
  octaveDecay: 0.1,
  layerOffset: 0,
  colorSpeed: 1,
} as const;

// 单一配置来源：SideRays 的全部着色器参数，颜色可由 props 覆盖。
const SIDE_RAYS_CONFIG = {
  speed: 2.5,
  rayColor1: "#eab308",
  rayColor2: "#96c8ff",
  intensity: 2,
  spread: 2,
  origin: "top-right" as SideRaysOrigin,
  tilt: 0,
  saturation: 1.5,
  blend: 0.75,
  falloff: 2,
  opacity: 1,
} as const;

interface SideRaysUniforms {
  iTime: { value: number };
  iResolution: { value: Vec2 };
  iSpeed: { value: number };
  iRayColor1: { value: Vec3 };
  iRayColor2: { value: Vec3 };
  iIntensity: { value: number };
  iSpread: { value: number };
  iFlipX: { value: number };
  iFlipY: { value: number };
  iTilt: { value: number };
  iSaturation: { value: number };
  iBlend: { value: number };
  iFalloff: { value: number };
  iOpacity: { value: number };
}

function hexToRgb(hex: string): [number, number, number] {
  const match = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  return match
    ? [
        Number.parseInt(match[1], 16) / 255,
        Number.parseInt(match[2], 16) / 255,
        Number.parseInt(match[3], 16) / 255,
      ]
    : [1, 1, 1];
}

function originToFlip(origin: SideRaysOrigin): [number, number] {
  switch (origin) {
    case "top-left":
      return [1, 0];
    case "bottom-right":
      return [0, 1];
    case "bottom-left":
      return [1, 1];
    default:
      return [0, 0];
  }
}

const softAuroraVertexShader = `
attribute vec2 uv;
attribute vec2 position;
varying vec2 vUv;
void main() {
  vUv = uv;
  gl_Position = vec4(position, 0, 1);
}
`;

// SoftAurora shader adapted from React Bits:
// https://reactbits.dev/backgrounds/soft-aurora
const softAuroraFragmentShader = `
precision highp float;

uniform float uTime;
uniform vec3 uResolution;
uniform float uSpeed;
uniform float uScale;
uniform float uBrightness;
uniform vec3 uColor1;
uniform vec3 uColor2;
uniform float uNoiseFreq;
uniform float uNoiseAmp;
uniform float uBandHeight;
uniform float uBandSpread;
uniform float uOctaveDecay;
uniform float uLayerOffset;
uniform float uColorSpeed;

#define TAU 6.28318

vec3 gradientHash(vec3 p) {
  p = vec3(
    dot(p, vec3(127.1, 311.7, 234.6)),
    dot(p, vec3(269.5, 183.3, 198.3)),
    dot(p, vec3(169.5, 283.3, 156.9))
  );
  vec3 h = fract(sin(p) * 43758.5453123);
  float phi = acos(2.0 * h.x - 1.0);
  float theta = TAU * h.y;
  return vec3(cos(theta) * sin(phi), sin(theta) * cos(phi), cos(phi));
}

float quinticSmooth(float t) {
  float t2 = t * t;
  float t3 = t * t2;
  return 6.0 * t3 * t2 - 15.0 * t2 * t2 + 10.0 * t3;
}

vec3 cosineGradient(float t, vec3 a, vec3 b, vec3 c, vec3 d) {
  return a + b * cos(TAU * (c * t + d));
}

float perlin3D(float amplitude, float frequency, float px, float py, float pz) {
  float x = px * frequency;
  float y = py * frequency;

  float fx = floor(x); float fy = floor(y); float fz = floor(pz);
  float cx = ceil(x);  float cy = ceil(y);  float cz = ceil(pz);

  vec3 g000 = gradientHash(vec3(fx, fy, fz));
  vec3 g100 = gradientHash(vec3(cx, fy, fz));
  vec3 g010 = gradientHash(vec3(fx, cy, fz));
  vec3 g110 = gradientHash(vec3(cx, cy, fz));
  vec3 g001 = gradientHash(vec3(fx, fy, cz));
  vec3 g101 = gradientHash(vec3(cx, fy, cz));
  vec3 g011 = gradientHash(vec3(fx, cy, cz));
  vec3 g111 = gradientHash(vec3(cx, cy, cz));

  float d000 = dot(g000, vec3(x - fx, y - fy, pz - fz));
  float d100 = dot(g100, vec3(x - cx, y - fy, pz - fz));
  float d010 = dot(g010, vec3(x - fx, y - cy, pz - fz));
  float d110 = dot(g110, vec3(x - cx, y - cy, pz - fz));
  float d001 = dot(g001, vec3(x - fx, y - fy, pz - cz));
  float d101 = dot(g101, vec3(x - cx, y - fy, pz - cz));
  float d011 = dot(g011, vec3(x - fx, y - cy, pz - cz));
  float d111 = dot(g111, vec3(x - cx, y - cy, pz - cz));

  float sx = quinticSmooth(x - fx);
  float sy = quinticSmooth(y - fy);
  float sz = quinticSmooth(pz - fz);

  float lx00 = mix(d000, d100, sx);
  float lx10 = mix(d010, d110, sx);
  float lx01 = mix(d001, d101, sx);
  float lx11 = mix(d011, d111, sx);

  float ly0 = mix(lx00, lx10, sy);
  float ly1 = mix(lx01, lx11, sy);

  return amplitude * mix(ly0, ly1, sz);
}

float auroraGlow(float t, vec2 shift) {
  vec2 uv = gl_FragCoord.xy / uResolution.y;
  uv += shift;

  float noiseVal = 0.0;
  float freq = uNoiseFreq;
  float amp = uNoiseAmp;
  vec2 samplePos = uv * uScale;

  for (float i = 0.0; i < 3.0; i += 1.0) {
    noiseVal += perlin3D(amp, freq, samplePos.x, samplePos.y, t);
    amp *= uOctaveDecay;
    freq *= 2.0;
  }

  float yBand = uv.y * 10.0 - uBandHeight * 10.0;
  return 0.3 * max(exp(uBandSpread * (1.0 - 1.1 * abs(noiseVal + yBand))), 0.0);
}

void main() {
  vec2 uv = gl_FragCoord.xy / uResolution.xy;
  float t = uSpeed * 0.4 * uTime;
  vec2 shift = vec2(0.0);

  vec3 col = vec3(0.0);
  col += 0.99 * auroraGlow(t, shift) * cosineGradient(uv.x + uTime * uSpeed * 0.2 * uColorSpeed, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.3, 0.20, 0.20)) * uColor1;
  col += 0.99 * auroraGlow(t + uLayerOffset, shift) * cosineGradient(uv.x + uTime * uSpeed * 0.1 * uColorSpeed, vec3(0.5), vec3(0.5), vec3(2.0, 1.0, 0.0), vec3(0.5, 0.20, 0.25)) * uColor2;

  col *= uBrightness;
  float alpha = clamp(length(col), 0.0, 1.0);
  gl_FragColor = vec4(col, alpha);
}
`;

// SideRays shader adapted from React Bits:
// https://reactbits.dev/backgrounds/side-rays
const sideRaysVertexShader = `
attribute vec2 position;
void main() {
  gl_Position = vec4(position, 0.0, 1.0);
}
`;

const sideRaysFragmentShader = `
precision highp float;

uniform float iTime;
uniform vec2 iResolution;
uniform float iSpeed;
uniform vec3 iRayColor1;
uniform vec3 iRayColor2;
uniform float iIntensity;
uniform float iSpread;
uniform float iFlipX;
uniform float iFlipY;
uniform float iTilt;
uniform float iSaturation;
uniform float iBlend;
uniform float iFalloff;
uniform float iOpacity;

float rayStrength(vec2 raySource, vec2 rayRefDirection, vec2 coord, float seedA, float seedB, float speed) {
  vec2 sourceToCoord = coord - raySource;
  float cosAngle = dot(normalize(sourceToCoord), rayRefDirection);
  float baseStrength = clamp(
    (0.45 + 0.15 * sin(cosAngle * seedA + iTime * speed)) +
    (0.3 + 0.2 * cos(-cosAngle * seedB + iTime * speed)),
    0.0,
    1.0
  );
  float distanceFalloff = clamp((iResolution.x - length(sourceToCoord)) / iResolution.x, 0.5, 1.0);
  return baseStrength * distanceFalloff;
}

void main() {
  vec2 fragCoord = gl_FragCoord.xy;
  if (iFlipX > 0.5) fragCoord.x = iResolution.x - fragCoord.x;
  if (iFlipY > 0.5) fragCoord.y = iResolution.y - fragCoord.y;

  vec2 coord = vec2(fragCoord.x, iResolution.y - fragCoord.y);
  vec2 rayPos = vec2(iResolution.x * 1.1, -0.5 * iResolution.y);

  float tiltRad = iTilt * 3.14159265 / 180.0;
  float cs = cos(tiltRad);
  float sn = sin(tiltRad);
  vec2 rel = coord - rayPos;
  vec2 tiltedCoord = vec2(rel.x * cs - rel.y * sn, rel.x * sn + rel.y * cs) + rayPos;

  float halfSpread = iSpread * 0.275;
  vec2 rayRefDir1 = normalize(vec2(cos(0.785398 + halfSpread), sin(0.785398 + halfSpread)));
  vec2 rayRefDir2 = normalize(vec2(cos(0.785398 - halfSpread), sin(0.785398 - halfSpread)));

  vec4 rays1 = vec4(iRayColor1, 1.0) * rayStrength(rayPos, rayRefDir1, tiltedCoord, 36.2214, 21.11349, iSpeed);
  vec4 rays2 = vec4(iRayColor2, 1.0) * rayStrength(rayPos, rayRefDir2, tiltedCoord, 22.3991, 18.0234, iSpeed * 0.2);
  vec4 color = rays1 * (1.0 - iBlend) * 0.9 + rays2 * iBlend * 0.9;

  float distanceToLight = length(fragCoord.xy - vec2(rayPos.x, iResolution.y - rayPos.y)) / iResolution.y;
  float brightness = iIntensity * 0.4 / pow(max(distanceToLight, 0.001), iFalloff);
  color.rgb *= brightness;

  float gray = dot(color.rgb, vec3(0.299, 0.587, 0.114));
  color.rgb = mix(vec3(gray), color.rgb, iSaturation);
  color.a = max(color.r, max(color.g, color.b)) * iOpacity;
  gl_FragColor = color;
}
`;

/**
 * 仅在容器进入视口（IntersectionObserver）且标签页可见（visibilitychange）时运行
 * RAF 渲染循环，不可见时暂停以省电；恢复时续跑。两个触发都需要：单靠
 * visibilitychange 漏掉“标签页仍可见但组件被其他面板覆盖/移出视口”的场景。
 *
 * `renderFrame` 接收已扣除暂停时长的动画时间（毫秒），因此恢复时动画相位连续、不跳变。
 * 返回清理函数：停止循环并移除监听。
 */
function runVisibilityGatedLoop(
  container: HTMLElement,
  renderFrame: (animationTimeMs: number) => void,
): () => void {
  let frameId = 0;
  let running = false;
  // 累计暂停时长，从 RAF 时间戳中扣除，避免恢复时 uTime 跳变造成闪烁。
  let pausedAccumMs = 0;
  let pauseStartMs = 0;
  let inViewport = true;
  let tabVisible = !document.hidden;

  const loop = (timestamp: number) => {
    renderFrame(timestamp - pausedAccumMs);
    frameId = requestAnimationFrame(loop);
  };

  const start = () => {
    if (running) return;
    running = true;
    if (pauseStartMs !== 0) {
      pausedAccumMs += performance.now() - pauseStartMs;
      pauseStartMs = 0;
    }
    frameId = requestAnimationFrame(loop);
  };

  const stop = () => {
    if (!running) return;
    running = false;
    pauseStartMs = performance.now();
    if (frameId) {
      cancelAnimationFrame(frameId);
      frameId = 0;
    }
  };

  const evaluate = () => {
    if (inViewport && tabVisible) {
      start();
    } else {
      stop();
    }
  };

  const intersectionObserver = new IntersectionObserver((entries) => {
    inViewport = entries.some((entry) => entry.isIntersecting);
    evaluate();
  });
  intersectionObserver.observe(container);

  const handleVisibility = () => {
    tabVisible = !document.hidden;
    evaluate();
  };
  document.addEventListener("visibilitychange", handleVisibility);

  evaluate();

  return () => {
    stop();
    intersectionObserver.disconnect();
    document.removeEventListener("visibilitychange", handleVisibility);
  };
}

function SoftAurora({
  animate = true,
  color1 = AURORA_CONFIG.color1,
  color2 = AURORA_CONFIG.color2,
}: SoftAuroraProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    let renderer: Renderer;
    try {
      renderer = new Renderer({ alpha: true, premultipliedAlpha: false });
    } catch (error) {
      console.warn("[ambient-background] SoftAurora disabled", error);
      return;
    }

    const gl = renderer.gl;
    gl.clearColor(0, 0, 0, 0);
    gl.canvas.className = "block h-full w-full";
    gl.canvas.style.backgroundColor = "transparent";

    const geometry = new Triangle(gl);
    const program = new Program(gl, {
      vertex: softAuroraVertexShader,
      fragment: softAuroraFragmentShader,
      uniforms: {
        uTime: { value: 0 },
        uResolution: { value: [gl.canvas.width, gl.canvas.height, 1] },
        uSpeed: { value: AURORA_CONFIG.speed },
        uScale: { value: AURORA_CONFIG.scale },
        uBrightness: { value: AURORA_CONFIG.brightness },
        uColor1: { value: hexToRgb(color1) },
        uColor2: { value: hexToRgb(color2) },
        uNoiseFreq: { value: AURORA_CONFIG.noiseFrequency },
        uNoiseAmp: { value: AURORA_CONFIG.noiseAmplitude },
        uBandHeight: { value: AURORA_CONFIG.bandHeight },
        uBandSpread: { value: AURORA_CONFIG.bandSpread },
        uOctaveDecay: { value: AURORA_CONFIG.octaveDecay },
        uLayerOffset: { value: AURORA_CONFIG.layerOffset },
        uColorSpeed: { value: AURORA_CONFIG.colorSpeed },
      },
    });

    const mesh = new Mesh(gl, { geometry, program });

    const resize = () => {
      const width = Math.max(container.offsetWidth, 1);
      const height = Math.max(container.offsetHeight, 1);
      renderer.setSize(width, height);
      program.uniforms.uResolution.value = [
        gl.canvas.width,
        gl.canvas.height,
        gl.canvas.width / gl.canvas.height,
      ];
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    container.appendChild(gl.canvas);
    resize();

    const render = (time: number) => {
      program.uniforms.uTime.value = time * 0.001;
      renderer.render({ scene: mesh });
    };

    let stopLoop: (() => void) | null = null;
    if (animate) {
      stopLoop = runVisibilityGatedLoop(container, render);
    } else {
      render(0);
    }

    return () => {
      stopLoop?.();
      resizeObserver.disconnect();
      if (container.contains(gl.canvas)) {
        container.removeChild(gl.canvas);
      }
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    };
  }, [animate, color1, color2]);

  return <div ref={containerRef} className="h-full w-full" />;
}

function SideRays({
  animate = true,
  rayColor1 = SIDE_RAYS_CONFIG.rayColor1,
  rayColor2 = SIDE_RAYS_CONFIG.rayColor2,
}: SideRaysProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    let renderer: Renderer;
    try {
      renderer = new Renderer({ dpr: Math.min(window.devicePixelRatio, 2), alpha: true });
    } catch (error) {
      console.warn("[ambient-background] SideRays disabled", error);
      return;
    }

    const gl = renderer.gl;
    gl.clearColor(0, 0, 0, 0);
    gl.canvas.className = "block h-full w-full";
    gl.canvas.style.backgroundColor = "transparent";

    const [flipX, flipY] = originToFlip(SIDE_RAYS_CONFIG.origin);
    const uniforms: SideRaysUniforms = {
      iTime: { value: 0 },
      iResolution: { value: [1, 1] },
      iSpeed: { value: SIDE_RAYS_CONFIG.speed },
      iRayColor1: { value: hexToRgb(rayColor1) },
      iRayColor2: { value: hexToRgb(rayColor2) },
      iIntensity: { value: SIDE_RAYS_CONFIG.intensity },
      iSpread: { value: SIDE_RAYS_CONFIG.spread },
      iFlipX: { value: flipX },
      iFlipY: { value: flipY },
      iTilt: { value: SIDE_RAYS_CONFIG.tilt },
      iSaturation: { value: SIDE_RAYS_CONFIG.saturation },
      iBlend: { value: SIDE_RAYS_CONFIG.blend },
      iFalloff: { value: SIDE_RAYS_CONFIG.falloff },
      iOpacity: { value: SIDE_RAYS_CONFIG.opacity },
    };

    const geometry = new Triangle(gl);
    const program = new Program(gl, {
      vertex: sideRaysVertexShader,
      fragment: sideRaysFragmentShader,
      uniforms,
    });
    const mesh = new Mesh(gl, { geometry, program });

    const resize = () => {
      const width = Math.max(container.clientWidth, 1);
      const height = Math.max(container.clientHeight, 1);
      renderer.dpr = Math.min(window.devicePixelRatio, 2);
      renderer.setSize(width, height);
      uniforms.iResolution.value = [
        width * renderer.dpr,
        height * renderer.dpr,
      ];
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    container.appendChild(gl.canvas);
    resize();

    const render = (time: number) => {
      uniforms.iTime.value = time * 0.001;
      renderer.render({ scene: mesh });
    };

    let stopLoop: (() => void) | null = null;
    if (animate) {
      stopLoop = runVisibilityGatedLoop(container, render);
    } else {
      render(0);
    }

    return () => {
      stopLoop?.();
      resizeObserver.disconnect();
      if (container.contains(gl.canvas)) {
        container.removeChild(gl.canvas);
      }
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    };
  }, [animate, rayColor1, rayColor2]);

  return <div ref={containerRef} className="h-full w-full" />;
}

function usePrefersReducedMotion() {
  const [reducedMotion, setReducedMotion] = useState(false);

  useEffect(() => {
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReducedMotion(media.matches);
    const handleChange = () => setReducedMotion(media.matches);
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, []);

  return reducedMotion;
}

export function AppAmbientBackground() {
  const reducedMotion = usePrefersReducedMotion();

  return (
    <div className="app-ambient-background" aria-hidden="true">
      <div className="app-ambient-layer app-ambient-aurora">
        <SoftAurora animate={!reducedMotion} />
      </div>
    </div>
  );
}

export function AppAmbientLightOverlay() {
  const { resolvedTheme } = useTheme();
  const reducedMotion = usePrefersReducedMotion();
  const isDark = resolvedTheme === "dark";

  if (!isDark) return null;

  return (
    <div className="app-ambient-light-overlay" aria-hidden="true">
      <SideRays key="dark-side-rays-overlay" animate={!reducedMotion} />
    </div>
  );
}
