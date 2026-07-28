@group(0) @binding(0) var source_texture: texture_2d<f32>;
@group(0) @binding(1) var second_texture: texture_2d<f32>;
@group(0) @binding(2) var linear_clamp: sampler;
struct Parameters { values: array<vec4<f32>, 8> }
@group(0) @binding(3) var<uniform> parameters: Parameters;

struct VertexOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let positions = array(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    let p = positions[index];
    var out: VertexOut; out.position = vec4(p, 0.0, 1.0); out.uv = p * vec2(0.5, -0.5) + vec2(0.5); return out;
}
fn sample_source(uv: vec2<f32>) -> vec4<f32> { return textureSampleLevel(source_texture, linear_clamp, uv, 0.0); }
@fragment fn fs_copy(in: VertexOut) -> @location(0) vec4<f32> { return sample_source(in.uv); }

fn aces(x: vec3<f32>) -> vec3<f32> {
    return clamp((x * (2.51 * x + vec3(0.03))) / (x * (2.43 * x + vec3(0.59)) + vec3(0.14)), vec3(0.0), vec3(1.0));
}
fn linear_to_srgb(x: vec3<f32>) -> vec3<f32> {
    let low = x * 12.92; let high = 1.055 * pow(x, vec3(1.0 / 2.4)) - vec3(0.055);
    return select(high, low, x <= vec3(0.0031308));
}
@fragment fn fs_tone_map(in: VertexOut) -> @location(0) vec4<f32> { let c=sample_source(in.uv); return vec4(linear_to_srgb(aces(c.rgb * parameters.values[0].x)), c.a); }
fn grading_result(source: vec4<f32>, graded: vec3<f32>, factor: f32) -> vec4<f32> { return vec4(mix(source.rgb, graded, vec3(factor)), source.a); }
@fragment fn fs_color_balance(in: VertexOut) -> @location(0) vec4<f32> {
    let c=sample_source(in.uv); var graded: vec3<f32>;
    if parameters.values[0].x < 0.5 {
        let lift=parameters.values[2].xyz+vec3(parameters.values[0].z); let lifted=(c.rgb-vec3(1.0))*(vec3(2.0)-lift)+vec3(1.0);
        let gain=parameters.values[4].xyz*parameters.values[1].x; let gained=max(lifted*gain,vec3(0.0));
        let gamma=max(parameters.values[3].xyz*parameters.values[0].w,vec3(0.000001)); graded=pow(gained,vec3(1.0)/gamma);
    } else {
        let slope=parameters.values[7].xyz*parameters.values[1].w; let offset=vec3(parameters.values[1].y)+(parameters.values[5].xyz-vec3(1.0));
        let power=max(parameters.values[6].xyz*parameters.values[1].z,vec3(0.000001)); graded=pow(max(c.rgb*slope+offset,vec3(0.0)),power);
    }
    return grading_result(c,graded,parameters.values[0].y);
}
@fragment fn fs_exposure_contrast(in: VertexOut) -> @location(0) vec4<f32> {
    let c=sample_source(in.uv); let exposed=c.rgb*exp2(parameters.values[0].x); let pivot=parameters.values[0].z;
    let graded=sign(exposed)*vec3(pivot)*pow(abs(exposed)/vec3(pivot),vec3(parameters.values[0].y)); return grading_result(c,graded,parameters.values[0].w);
}
@fragment fn fs_saturation(in: VertexOut) -> @location(0) vec4<f32> { let c=sample_source(in.uv); let l=dot(c.rgb,vec3(0.2126,0.7152,0.0722)); return grading_result(c,mix(vec3(l),c.rgb,vec3(parameters.values[0].x)),parameters.values[0].y); }
@fragment fn fs_channel_mixer(in: VertexOut) -> @location(0) vec4<f32> { let c=sample_source(in.uv); let graded=vec3(dot(c.rgb,parameters.values[0].xyz),dot(c.rgb,parameters.values[1].xyz),dot(c.rgb,parameters.values[2].xyz)); return grading_result(c,graded,parameters.values[0].w); }
@fragment fn fs_bloom_extract(in: VertexOut) -> @location(0) vec4<f32> {
    let c=sample_source(in.uv); let brightness=max(c.r,max(c.g,c.b)); let knee=max(parameters.values[0].y,0.00001); let soft=clamp((brightness-parameters.values[0].x+knee)/(2.0*knee),0.0,1.0); let contribution=max(brightness-parameters.values[0].x,0.0)+soft*soft*knee; return vec4(c.rgb*contribution/max(brightness,0.00001),1.0);
}
@fragment fn fs_bloom_blur(in: VertexOut) -> @location(0) vec4<f32> {
    let size=vec2<f32>(textureDimensions(source_texture)); let step=parameters.values[0].xy*parameters.values[0].z/size;
    var c=sample_source(in.uv)*0.227027; c+=sample_source(in.uv+step*1.384615)*0.316216; c+=sample_source(in.uv-step*1.384615)*0.316216; c+=sample_source(in.uv+step*3.230769)*0.070270; c+=sample_source(in.uv-step*3.230769)*0.070270; return c;
}
@fragment fn fs_bloom_composite(in: VertexOut) -> @location(0) vec4<f32> { let c=sample_source(in.uv); return vec4(c.rgb+textureSampleLevel(second_texture,linear_clamp,in.uv,0.0).rgb*parameters.values[0].x,c.a); }
fn luminance(c: vec3<f32>) -> f32 { return dot(c,vec3(0.2126,0.7152,0.0722)); }
@fragment fn fs_luminance_edge(in: VertexOut) -> @location(0) vec4<f32> {
    let d=1.0/vec2<f32>(textureDimensions(source_texture)); var gx=0.0; var gy=0.0;
    gx += -luminance(sample_source(in.uv+d*vec2(-1.0,-1.0)).rgb)+luminance(sample_source(in.uv+d*vec2(1.0,-1.0)).rgb); gx += -2.0*luminance(sample_source(in.uv+d*vec2(-1.0,0.0)).rgb)+2.0*luminance(sample_source(in.uv+d*vec2(1.0,0.0)).rgb); gx += -luminance(sample_source(in.uv+d*vec2(-1.0,1.0)).rgb)+luminance(sample_source(in.uv+d*vec2(1.0,1.0)).rgb);
    gy += -luminance(sample_source(in.uv+d*vec2(-1.0,-1.0)).rgb)-2.0*luminance(sample_source(in.uv+d*vec2(0.0,-1.0)).rgb)-luminance(sample_source(in.uv+d*vec2(1.0,-1.0)).rgb); gy += luminance(sample_source(in.uv+d*vec2(-1.0,1.0)).rgb)+2.0*luminance(sample_source(in.uv+d*vec2(0.0,1.0)).rgb)+luminance(sample_source(in.uv+d*vec2(1.0,1.0)).rgb);
    let edge=clamp(length(vec2(gx,gy))*parameters.values[0].x,0.0,1.0); return vec4(vec3(edge),1.0);
}
