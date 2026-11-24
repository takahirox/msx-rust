export const displayPreferenceKey = 'msx-rust:display';

// The framebuffer arrives top row first. Map that row to the top of the canvas.
const vertexSource = `#version 300 es
out vec2 uv;
void main() {
  vec2 corner = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  uv = vec2(corner.x, 1.0 - corner.y);
  gl_Position = vec4(corner * 2.0 - 1.0, 0.0, 1.0);
}`;

const fragmentSource = `#version 300 es
precision highp float;
in vec2 uv;
uniform sampler2D frame;
uniform vec2 frameSize;
uniform vec2 outputSize;
uniform float fieldRows;
out vec4 color;

void main() {
  vec2 pixel = uv * frameSize;
  vec2 scale = outputSize / frameSize;
  vec2 center = floor(pixel) + 0.5;
  // Bias linear sampling toward the closest texel, with less sharpening
  // when output pixels are too sparse to resolve the source grid.
  vec2 crisp = smoothstep(vec2(0.85), vec2(1.5), scale);
  vec2 samplePoint = mix(pixel, center, vec2(0.72, 0.84) * crisp);
  // Interlaced fields are woven in pairs. Resolve both rows at larger sizes
  // and blend the pair when the output can't resolve each field's rows.
  if (fieldRows == 2.0) {
    float pairCenter = floor(pixel.y / 2.0) * 2.0 + 1.0;
    samplePoint.y = mix(pairCenter, center.y, crisp.y);
  }
  vec3 rgb = texture(frame, samplePoint / frameSize).rgb;

  float lineScale = scale.y * fieldRows;
  float lineEdge = abs(fract(pixel.y / fieldRows) - 0.5);
  float scanline = 0.14 * smoothstep(1.5, 2.65, lineScale)
    * smoothstep(0.18, 0.49, lineEdge) / sqrt(fieldRows);
  float column = mod(floor(gl_FragCoord.x), 3.0);
  vec3 phosphor = 1.0 - step(vec3(0.5), abs(vec3(0, 1, 2) - column));
  float mask = 0.065 * smoothstep(1.2, 2.4, min(scale.x, lineScale));
  color = vec4(rgb * (1.0 - scanline) * (1.0 - mask * (1.0 - phosphor)), 1.0);
}`;

// Limit shader work and backing memory on large/high-density displays.
const maxOutputWidth = 960;
const maxOutputHeight = 720;

class OutputAllocationError extends Error {
  constructor(width, height) {
    super('WebGL2 output allocation failed');
    this.width = width;
    this.height = height;
  }
}

function compileShader(gl, type, source) {
  const shader = gl.createShader(type);
  if (!shader) throw new Error('WebGL2 shader allocation failed');
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (gl.getShaderParameter(shader, gl.COMPILE_STATUS)) return shader;
  gl.deleteShader(shader);
  throw new Error('WebGL2 shader compilation failed');
}

function createCrt(canvas) {
  const gl = canvas.getContext('webgl2', { alpha: false, depth: false, antialias: false });
  if (!gl) throw new Error('WebGL2 is unavailable');
  let program, vertex, fragment, texture;
  let width = 0, height = 0;
  let outputWidth = 0, outputHeight = 0;
  let frameSize, outputSize, fieldRows;
  function dispose() {
    if (texture) gl.deleteTexture(texture);
    if (program) {
      gl.useProgram(null);
      gl.deleteProgram(program);
    }
  }
  try {
    vertex = compileShader(gl, gl.VERTEX_SHADER, vertexSource);
    fragment = compileShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
    program = gl.createProgram();
    texture = gl.createTexture();
    if (!program || !texture) throw new Error('WebGL2 resource allocation failed');
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error('WebGL2 shader linking failed');
    gl.useProgram(program);
    gl.bindTexture(gl.TEXTURE_2D, texture);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    frameSize = gl.getUniformLocation(program, 'frameSize');
    outputSize = gl.getUniformLocation(program, 'outputSize');
    fieldRows = gl.getUniformLocation(program, 'fieldRows');
  } catch (error) {
    dispose();
    throw error;
  } finally {
    if (vertex) gl.deleteShader(vertex);
    if (fragment) gl.deleteShader(fragment);
  }
  return {
    resize({ width: w, height: h }) {
      if (outputWidth === w && outputHeight === h) return false;
      if (canvas.width !== w) canvas.width = w;
      if (canvas.height !== h) canvas.height = h;
      if (gl.drawingBufferWidth !== w || gl.drawingBufferHeight !== h) throw new OutputAllocationError(w, h);
      gl.viewport(0, 0, w, h);
      gl.uniform2f(outputSize, w, h);
      outputWidth = w;
      outputHeight = h;
      return true;
    },
    upload(frame) {
      if (frame.width !== width || frame.height !== height) {
        ({ width, height } = frame);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, frame.pixels);
        gl.uniform2f(frameSize, width, height);
        // The VDP doubles the 192/212 active lines when interlace is enabled.
        gl.uniform1f(fieldRows, height > 212 ? 2 : 1);
      } else {
        gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, width, height, gl.RGBA, gl.UNSIGNED_BYTE, frame.pixels);
      }
    },
    draw() {
      gl.drawArrays(gl.TRIANGLES, 0, 3);
      if (gl.getError() !== gl.NO_ERROR) throw new Error('WebGL2 could not draw the display');
    },
    dispose,
  };
}

export function createDisplay(screen, canvas, select, status, readFrame) {
  const context = screen.getContext('2d', { alpha: false });
  let hasFrame = false;
  let renderer;
  let failure;
  let pixelRatio;
  let requested = 'crt';
  try {
    if (localStorage.getItem(displayPreferenceKey) === 'clean') requested = 'clean';
  } catch { /* Browser storage can be blocked. */ }
  select.value = requested;

  function showStatus(message) {
    if (status.textContent !== message) status.textContent = message;
  }
  function showClean(frame) {
    if (screen.width !== frame.width) screen.width = frame.width;
    if (screen.height !== frame.height) screen.height = frame.height;
    context.putImageData(new ImageData(frame.pixels, frame.width, frame.height), 0, 0);
    if (!canvas.hidden) canvas.hidden = true;
    showStatus(requested === 'crt' && failure ? `Clean active · ${failure.message}.` : '');
  }
  function fallback(error, frame) {
    renderer?.dispose();
    renderer = undefined;
    failure = error;
    if (hasFrame) showClean(frame ?? readFrame());
  }
  function measureOutput() {
    const { width, height } = screen.getBoundingClientRect();
    pixelRatio = devicePixelRatio;
    const scale = Math.min(pixelRatio, maxOutputWidth / width, maxOutputHeight / height);
    return {
      width: Math.max(1, Math.round(width * scale)),
      height: Math.max(1, Math.round(height * scale)),
    };
  }
  function retrySmallerOutput() {
    if (!(failure instanceof OutputAllocationError)) return false;
    const { width, height } = measureOutput();
    if (width > failure.width || height > failure.height) return false;
    if (width === failure.width && height === failure.height) return false;
    failure = undefined;
    return true;
  }
  function present() {
    const frame = readFrame();
    hasFrame = true;
    if (failure && requested === 'crt' && pixelRatio !== devicePixelRatio) retrySmallerOutput();
    if (requested === 'crt' && !failure) {
      try {
        if (!renderer) renderer = createCrt(canvas);
        if (canvas.hidden || pixelRatio !== devicePixelRatio) renderer.resize(measureOutput());
        renderer.upload(frame);
        renderer.draw();
        if (canvas.hidden) {
          canvas.hidden = false;
          showStatus('');
        }
      } catch (error) {
        fallback(error, frame);
      }
    } else {
      showClean(frame);
    }
  }
  function redraw() {
    if (requested !== 'crt') return;
    if (!renderer) {
      if (retrySmallerOutput()) present();
      return;
    }
    try {
      if (renderer.resize(measureOutput())) renderer.draw();
    } catch (error) {
      fallback(error);
    }
  }
  select.addEventListener('change', () => {
    requested = select.value;
    try { localStorage.setItem(displayPreferenceKey, requested); } catch { /* Keep the in-tab choice. */ }
    failure = undefined;
    if (hasFrame) present();
  });
  canvas.addEventListener('webglcontextlost', event => {
    event.preventDefault();
    fallback(new Error('WebGL2 context lost'));
  });
  canvas.addEventListener('webglcontextrestored', () => {
    failure = undefined;
    if (hasFrame && requested === 'crt') present();
  });
  new ResizeObserver(redraw).observe(screen);
  return {
    present,
    refreshScale() {
      // A density-only change can leave the observed CSS size unchanged.
      if (requested === 'crt' && (renderer || failure instanceof OutputAllocationError)
          && pixelRatio !== devicePixelRatio) redraw();
    },
  };
}
