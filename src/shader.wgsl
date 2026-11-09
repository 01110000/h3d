// 3D Gaussian Splatting: EWA projection of 3D covariances to screen-space
// ellipses, drawn as instanced quads and alpha-blended back to front.

struct Uniforms {
    view: mat4x4<f32>,
    proj: mat4x4<f32>,
    // xy: viewport size in pixels, zw: focal length in pixels
    viewport_focal: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
};

const SIGMA_EXTENT: f32 = 3.0;

@vertex
fn vs_main(
    @builtin(vertex_index) vi: u32,
    @location(0) center: vec3<f32>,
    @location(1) cov_a: vec3<f32>,
    @location(2) cov_b: vec3<f32>,
    @location(3) color: vec4<f32>,
) -> VsOut {
    var out: VsOut;
    out.color = color;
    out.local = vec2<f32>(0.0);
    out.position = vec4<f32>(0.0, 0.0, 2.0, 1.0); // culled by default

    let cam = u.view * vec4<f32>(center, 1.0);
    let z = -cam.z;
    if (z < 0.05) {
        return out;
    }
    let clip = u.proj * cam;
    let ndc = clip.xy / clip.w;
    if (abs(ndc.x) > 1.3 || abs(ndc.y) > 1.3) {
        return out;
    }

    let viewport = u.viewport_focal.xy;
    let focal = u.viewport_focal.zw;

    // Jacobian of the perspective projection (pixels per view-space unit).
    let j0 = vec3<f32>(focal.x / z, 0.0, focal.x * cam.x / (z * z));
    let j1 = vec3<f32>(0.0, focal.y / z, focal.y * cam.y / (z * z));
    let w = mat3x3<f32>(u.view[0].xyz, u.view[1].xyz, u.view[2].xyz);
    // Rows of T = J * W
    let t0 = j0 * w;
    let t1 = j1 * w;

    let sigma = mat3x3<f32>(
        vec3<f32>(cov_a.x, cov_a.y, cov_a.z),
        vec3<f32>(cov_a.y, cov_b.x, cov_b.y),
        vec3<f32>(cov_a.z, cov_b.y, cov_b.z),
    );
    let st0 = sigma * t0;
    let st1 = sigma * t1;
    // Low-pass filter (0.3 px^2) keeps far-away splats at least a pixel wide.
    let a = dot(t0, st0) + 0.3;
    let b = dot(t0, st1);
    let c = dot(t1, st1) + 0.3;

    let mid = 0.5 * (a + c);
    let radius = length(vec2<f32>(0.5 * (a - c), b));
    let l1 = mid + radius;
    let l2 = max(mid - radius, 0.1);
    var v1 = vec2<f32>(1.0, 0.0);
    if (abs(b) > 1e-6) {
        v1 = normalize(vec2<f32>(b, l1 - a));
    } else if (c > a) {
        v1 = vec2<f32>(0.0, 1.0);
    }
    let v2 = vec2<f32>(-v1.y, v1.x);
    let major = min(sqrt(l1), 1024.0) * SIGMA_EXTENT * v1;
    let minor = min(sqrt(l2), 1024.0) * SIGMA_EXTENT * v2;

    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u)) * 2.0 - 1.0;
    let offset_px = corner.x * major + corner.y * minor;
    out.position = vec4<f32>(ndc + offset_px * 2.0 / viewport, 0.5, 1.0);
    out.local = corner * SIGMA_EXTENT;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let power = -0.5 * dot(in.local, in.local);
    let alpha = min(0.99, in.color.a * exp(power));
    if (alpha < 1.0 / 255.0) {
        discard;
    }
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
