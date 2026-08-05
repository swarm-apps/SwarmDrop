"use client";

// Web 应用区的环境背景（WebGL）。**这是桌面 `src/components/layout/app-ambient-background.tsx`
// 的第二份实现，同一套标准**——同 `section.tsx` / `master-detail.tsx` / `empty-state.tsx` 与
// 它们桌面对应物的关系。
//
// ## 为什么它必须存在（推翻了一条旧决策）
//
// DESIGN.md 此前写着「The WebGL ambient background is desktop-only」，理由是 ogl 约 30KB
// 且持续占 GPU，而 Web 的基线视口是手机浏览器。这条理由本身没错，但它漏算了代价：
// 玻璃层（`glass-panel` / `glass-card`）是被**一起保留**下来的，而
//
//   backdrop-filter: blur() 作用在一块纯色上，产出的还是那块纯色。
//
// 亮色下 `rgb(255 255 255 / 0.58)` 叠在 `oklch(0.99)` 的壳上混出来仍是 0.99；暗色下
// 0.56 的 `oklch(0.31)` 叠在 `oklch(0.145)` 上约等于 0.24，与 `--card` 的 0.27 相差 0.03。
// 也就是说 Web 端保留的是玻璃的**合成层开销**，而不是玻璃。同一份收件箱空态代码，桌面看着
// 是成品、Web 看着是骨架屏，差别全在这里。有了这一层，玻璃才重新开始工作。
//
// ## 与桌面的差异（都是浏览器基线逼出来的，不是审美分叉）
//
// | | 桌面 | 这里 |
// |---|---|---|
// | 加载 | 同步 import | `next/dynamic` + `ssr:false`（见 `app-ambient-background.tsx`），让 wasm 与节点启动先走 |
// | DPR | aurora 不设（=1）、rays 上限 2 | 两者统一 [`ambientDpr`]：窄屏 1、宽屏上限 1.5 |
// | 帧率 | 跟满 RAF | [`FPS_CAP`] = 30。这层是缓慢流动的噪声场，30 与 60 肉眼无差，GPU 少一半 |
// | 层不透明度 | 1 | `--ambient-aurora-opacity`：暗 0.7 / 亮 0.48。着色器发的是加色光，深底上是辉光、浅底上是脏渍 |
// | 遮罩 | 无 | 径向 `mask-image` 把光留在边缘：着色器把光带钉在画面中线，1240px 限宽 + 32px 区块间距会让它从缝隙里原样透出来 |
//
// **着色器源码与 *_CONFIG 逐字取自桌面**，不要单边改：改了两边就会分叉，而分叉在截图对比里
// 看不出来（都是"一片流动的光"），只有并排放才发现不是同一个产品。
//
// 门控三件套原样保留：IntersectionObserver（组件移出视口）+ visibilitychange（标签页隐藏）
// + prefers-reduced-motion（**冻结首帧而不是整层不渲染**——纹理留下，动的部分去掉，
// 这是 DESIGN.md 明写的降级形态）。

import { useEffect, useRef, useState } from "react";
import { Mesh, Program, Renderer, Triangle } from "ogl";
import { useTheme } from "next-themes";

type Vec2 = [number, number];
type Vec3 = [number, number, number];

/** 目标帧率上限。见文件头：这层的运动是秒级的，30fps 与 60fps 肉眼不可分。 */
const FPS_CAP = 30;

/**
 * 环境层的渲染分辨率倍率。
 *
 * 窄屏（手机）钉死 1：那是电池最吃紧、GPU 最弱、而屏幕最小最看不出锯齿的场合，
 * 恰好三件事同向。宽屏上限 1.5 而不是 2——这是一层没有硬边的噪声场，
 * 2 倍采样多出来的信息全部落在人眼分辨不了的梯度里。
 */
function ambientDpr(): number {
  if (typeof window === "undefined") return 1;
  if (window.innerWidth < 768) return 1;
  return Math.min(window.devicePixelRatio || 1, 1.5);
}

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

type SideRaysOrigin = "top-right" | "top-left" | "bottom-right" | "bottom-left";

// 单一配置来源：SoftAurora 的全部着色器参数。与桌面逐字相同。
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

// 单一配置来源：SideRays 的全部着色器参数。与桌面逐字相同。
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

function hexToRgb(hex: string): Vec3 {
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
 * visibilitychange 漏掉「标签页仍可见但组件被其他面板覆盖/移出视口」的场景。
 *
 * `renderFrame` 接收已扣除暂停时长的动画时间（毫秒），因此恢复时动画相位连续、不跳变。
 *
 * **比桌面多一档 [`FPS_CAP`] 节流。** RAF 仍然每帧回调（它是浏览器的心跳，拦不住），
 * 但只有跨过帧预算才真正 `renderer.render()`——省下来的是 GPU 提交与像素填充，
 * 那才是这层的成本所在。节流用的是**动画时间**而不是墙钟：暂停恢复后不会因为
 * `lastDrawMs` 停在旧值而立刻补画一帧。
 *
 * 返回清理函数：停止循环并移除监听。
 */
function runVisibilityGatedLoop(
  container: HTMLElement,
  renderFrame: (animationTimeMs: number) => void,
): () => void {
  const frameBudgetMs = 1000 / FPS_CAP;
  let frameId = 0;
  let running = false;
  // 累计暂停时长，从 RAF 时间戳中扣除，避免恢复时 uTime 跳变造成闪烁。
  let pausedAccumMs = 0;
  let pauseStartMs = 0;
  let lastDrawMs = Number.NEGATIVE_INFINITY;
  let inViewport = true;
  let tabVisible = !document.hidden;

  const loop = (timestamp: number) => {
    const animationTimeMs = timestamp - pausedAccumMs;
    if (animationTimeMs - lastDrawMs >= frameBudgetMs) {
      lastDrawMs = animationTimeMs;
      renderFrame(animationTimeMs);
    }
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
    // **try 必须一路包到 mesh 建好**，不能只包 `new Renderer()`：ogl 的 `Program` 在
    // shader 编译或 link 失败时是 **throw**，而两段 fragment shader 都写死
    // `precision highp float`——低端移动 GPU 不保证支持片元 highp，而手机浏览器正是本端
    // 声明的基线视口。只包 Renderer 的话，那台设备上异常会冒到最近的 error boundary，
    // **把整个 `.app-shell` 连同正在跑的节点与传输一起卸载**，而清理函数从未注册、
    // 已创建的 context 也收不回来。装饰层不该有这个权力。
    // `!` 明确断言赋值：catch 分支一定 `return`，走到下面就一定建好了。
    // 但 catch 里仍写 `renderer?.` ——那一刻它可能真的还没赋上（`new Renderer` 自己抛）。
    let renderer!: Renderer;
    let program!: Program;
    let mesh!: Mesh;
    try {
      renderer = new Renderer({
        alpha: true,
        premultipliedAlpha: false,
        dpr: ambientDpr(),
      });

      const gl = renderer.gl;
      gl.clearColor(0, 0, 0, 0);
      gl.canvas.className = "block h-full w-full";
      gl.canvas.style.backgroundColor = "transparent";

      const geometry = new Triangle(gl);
      program = new Program(gl, {
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

      mesh = new Mesh(gl, { geometry, program });
    } catch (error) {
      // WebGL 不可用（旧设备、隐私模式、GPU 黑名单）或 shader 编不过时整层缺席，
      // 其余 UI 不受影响——这也正是玻璃层必须保留 `--card` 降级底的原因。
      // 已经创建出来的 context 要主动丢掉，否则它一直占着浏览器的 context 配额。
      console.warn("[ambient-background] SoftAurora disabled", error);
      renderer?.gl?.getExtension("WEBGL_lose_context")?.loseContext();
      return;
    }

    const gl = renderer.gl;

    const resize = () => {
      const width = Math.max(container.offsetWidth, 1);
      const height = Math.max(container.offsetHeight, 1);
      renderer.dpr = ambientDpr();
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
    // try 的范围同 `SoftAurora`：一路包到 mesh 建好，`!` 的理由也见那里。
    let renderer!: Renderer;
    let mesh!: Mesh;
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

    try {
      renderer = new Renderer({ dpr: ambientDpr(), alpha: true });

      const gl = renderer.gl;
      gl.clearColor(0, 0, 0, 0);
      gl.canvas.className = "block h-full w-full";
      gl.canvas.style.backgroundColor = "transparent";

      const geometry = new Triangle(gl);
      const program = new Program(gl, {
        vertex: sideRaysVertexShader,
        fragment: sideRaysFragmentShader,
        uniforms,
      });
      mesh = new Mesh(gl, { geometry, program });
    } catch (error) {
      console.warn("[ambient-background] SideRays disabled", error);
      renderer?.gl?.getExtension("WEBGL_lose_context")?.loseContext();
      return;
    }

    const gl = renderer.gl;

    const resize = () => {
      const width = Math.max(container.clientWidth, 1);
      const height = Math.max(container.clientHeight, 1);
      renderer.dpr = ambientDpr();
      renderer.setSize(width, height);
      uniforms.iResolution.value = [width * renderer.dpr, height * renderer.dpr];
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

/** 订阅一条 media query。SSR/首帧一律返回 false（静态导出下预渲染读不到用户偏好）。 */
function useMediaPreference(query: string): boolean {
  const [matches, setMatches] = useState(false);

  useEffect(() => {
    const media = window.matchMedia(query);
    setMatches(media.matches);
    const handleChange = () => setMatches(media.matches);
    media.addEventListener("change", handleChange);
    return () => media.removeEventListener("change", handleChange);
  }, [query]);

  return matches;
}

/**
 * 环境层本体。**默认导出**是给 `app-ambient-background.tsx` 的 `next/dynamic` 用的
 * ——具名导出不会被那条动态 import 的类型签名接受。
 *
 * 侧光只在暗色出现（与桌面一致）：它是打在深底上的一道冷光，亮底上只会变成一块脏渍。
 * `resolvedTheme` 在首帧是 `undefined`（next-themes 的值存在 localStorage，静态导出的
 * 预渲染 HTML 里没有），所以判据写成 `=== "dark"` 而不是 `!== "light"`——
 * 宁可晚一帧出现，也不要在亮色下先闪一下侧光。
 */
export default function AmbientCanvas() {
  const { resolvedTheme } = useTheme();
  const reducedMotion = useMediaPreference("(prefers-reduced-motion: reduce)");
  const reducedTransparency = useMediaPreference("(prefers-reduced-transparency: reduce)");
  const isDark = resolvedTheme === "dark";

  // CSS 那边已经把这两层 `display: none` 了，但**不挂载才是真的省**：`display:none`
  // 之下 React 树照常挂、两个 WebGL context 照常创建并一直占着浏览器的 context 配额
  // （RAF 会被 IntersectionObserver 停掉，GPU 循环开销确实没了，context 没走）。
  // 而「context 数量有上限」正是这一层必须是单例的硬理由。
  if (reducedTransparency) return null;

  return (
    <>
      <div className="app-ambient-background" aria-hidden="true">
        <div className="app-ambient-layer app-ambient-aurora">
          <SoftAurora animate={!reducedMotion} />
        </div>
      </div>
      {isDark && (
        <div className="app-ambient-light-overlay" aria-hidden="true">
          <SideRays key="dark-side-rays-overlay" animate={!reducedMotion} />
        </div>
      )}
    </>
  );
}
