use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};

use serde::Deserialize;

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextureParameters {
    residency: TextureResidency,
    texture: TextureDescriptor,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CullParameters {
    camera: ActiveCamera,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RasterParameters {
    depth_compare: CompareFunction,
    depth_write_enabled: bool,
    clear_depth: f32,
    clear_color: [f64; 4],
    predicate_default: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct FrameOutParameters {
    surface_format: SurfaceFormatRequest,
    hdr_enabled: bool,
    tone_mapper: ToneMapper,
    exposure_stops: f32,
    output_transfer: OutputTransfer,
    scale_mode: ScaleMode,
    filter: FrameFilter,
    background_color: [f32; 4],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ColorBalanceParameters {
    mode: ColorBalanceMode,
    factor: f32,
    lift: f32,
    lift_color: [f32; 4],
    gamma: f32,
    gamma_color: [f32; 4],
    gain: f32,
    gain_color: [f32; 4],
    offset: f32,
    offset_color: [f32; 4],
    power: f32,
    power_color: [f32; 4],
    slope: f32,
    slope_color: [f32; 4],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ExposureContrastParameters {
    exposure_stops: f32,
    contrast: f32,
    pivot: f32,
    factor: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaturationParameters {
    saturation: f32,
    factor: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ChannelMixerParameters {
    red_output: [f32; 3],
    green_output: [f32; 3],
    blue_output: [f32; 3],
    factor: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BloomExtractParameters {
    threshold: f32,
    knee: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BloomBlurParameters {
    direction: [f32; 2],
    radius: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BloomCompositeParameters {
    intensity: f32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LuminanceEdgeParameters {
    strength: f32,
}

fn range(value: f32, min: f32, max: f32, path: String) -> Result<f32, GraphError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(value)
    } else {
        Err(error(
            "GRAPH_PARAMETERS_INVALID",
            &format!("value must be finite and in [{min},{max}]"),
            path,
        ))
    }
}

fn components<const N: usize>(
    value: [f32; N],
    min: f32,
    max: f32,
    base: &str,
) -> Result<[f32; N], GraphError> {
    let mut result = value;
    for (i, component) in result.iter_mut().enumerate() {
        *component = range(*component, min, max, format!("{base}[{i}]"))?;
    }
    Ok(result)
}
fn color(value: [f32; 4], min: f32, max: f32, base: &str) -> Result<[f32; 3], GraphError> {
    let value = components(value, min, max, base)?;
    Ok([value[0], value[1], value[2]])
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct OutputKey(usize, u16);
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum AttachmentRoot {
    Authored(OutputKey),
    CompilerDefault { node: usize, ordinal: u16 },
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum TransitionTargetKey {
    Authored(OutputKey),
    CompilerDefaultInput {
        owner_node: usize,
        input_ordinal: u16,
    },
}
#[derive(Clone, Copy)]
struct BoundInput {
    producer: OutputKey,
}
#[derive(Clone)]
struct DependencyEdge {
    from_node: usize,
    from_socket: String,
    producer_output_ordinal: u16,
    to_node: usize,
    to_socket: String,
    consumer_input_ordinal: u16,
    resource: NodeOutputRef,
}

#[derive(Clone, Copy)]
struct TextureTransition {
    writer_node: usize,
    input_socket: &'static str,
    target: TransitionTargetKey,
    output: OutputKey,
}

#[derive(Clone, Copy)]
enum ResolvedTransition {
    Resolved {
        family: u32,
        version: u32,
        target: TransitionTargetKey,
    },
    Cyclic,
}

#[derive(Clone)]
struct DefaultDraft {
    key: TransitionTargetKey,
    resource: u32,
    family: u32,
    owner_node: usize,
    input_ordinal: u16,
    socket: &'static str,
    role: CompilerTextureRole,
    format: TextureFormat,
    descriptor: DraftDescriptor,
}

#[derive(Clone)]
enum DraftDescriptor {
    Deferred,
    Known(NormalizedTextureDescriptor),
}

enum SampledDescriptor<'a> {
    Known(&'a NormalizedTextureDescriptor),
    Deferred,
    Unproduced,
}

fn known_family_descriptor<'a>(
    family: u32,
    authored_families: &'a [TextureFamily],
    drafts: &'a [DefaultDraft],
) -> Option<&'a NormalizedTextureDescriptor> {
    if let Some(family) = authored_families.get(family as usize) {
        return Some(family_descriptor(family));
    }
    let draft = drafts.get(family as usize - authored_families.len())?;
    match &draft.descriptor {
        DraftDescriptor::Known(descriptor) => Some(descriptor),
        DraftDescriptor::Deferred => None,
    }
}

fn sampled_descriptor<'a>(
    key: OutputKey,
    version_of: &HashMap<OutputKey, (u32, u32, u32)>,
    resolved: &HashMap<OutputKey, ResolvedTransition>,
    authored_families: &'a [TextureFamily],
    drafts: &'a [DefaultDraft],
) -> Result<SampledDescriptor<'a>, GraphError> {
    if let Some(&(family, _, _)) = version_of.get(&key) {
        return Ok(known_family_descriptor(family, authored_families, drafts)
            .map(SampledDescriptor::Known)
            .unwrap_or(SampledDescriptor::Deferred));
    }
    match resolved.get(&key) {
        Some(ResolvedTransition::Cyclic) => Ok(SampledDescriptor::Deferred),
        Some(ResolvedTransition::Resolved { .. }) => Err(error(
            "GRAPH_RESOURCE_VERSION_INVALID",
            "resolved texture has no version",
            "resources",
        )),
        None => Ok(SampledDescriptor::Unproduced),
    }
}

fn reaches(
    from: usize,
    to: usize,
    outgoing_edges: &[Vec<usize>],
    ordering_outgoing: &[Vec<usize>],
    edges: &[DependencyEdge],
    live: &HashSet<usize>,
    memo: &mut HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(&answer) = memo.get(&(from, to)) {
        return answer;
    }
    let mut stack = vec![from];
    let mut visited = HashSet::new();
    let mut answer = false;
    while let Some(node) = stack.pop() {
        if !visited.insert(node) {
            continue;
        }
        if node == to {
            answer = true;
            break;
        }
        for &edge_index in &outgoing_edges[node] {
            let next = edges[edge_index].to_node;
            if live.contains(&next) {
                stack.push(next);
            }
        }
        for &next in &ordering_outgoing[node] {
            if live.contains(&next) {
                stack.push(next);
            }
        }
    }
    memo.insert((from, to), answer);
    answer
}

fn error(code: &'static str, message: &str, path: impl Into<String>) -> GraphError {
    GraphError::at(code, message, path)
}
fn validate_name_length(s: &str, path: impl Into<String>) -> Result<(), GraphError> {
    if s.len() > 64 {
        Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "identifier exceeds 64 bytes",
            path.into(),
        ))
    } else {
        Ok(())
    }
}
fn validate_name_grammar(s: &str, path: impl Into<String>) -> Result<(), GraphError> {
    if s.is_empty() || !identifier(s) {
        Err(error("GRAPH_INVALID_ID", "invalid identifier", path))
    } else {
        Ok(())
    }
}

pub fn parse_and_compile(bytes: &[u8]) -> Result<CompiledGraph, GraphError> {
    compile(super::ast::parse(bytes)?)
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}
fn compatible_view(a: TextureFormat, b: TextureFormat) -> bool {
    matches!(
        (a, b),
        (TextureFormat::Rgba8Unorm, TextureFormat::Rgba8UnormSrgb)
            | (TextureFormat::Rgba8UnormSrgb, TextureFormat::Rgba8Unorm)
            | (TextureFormat::Bgra8Unorm, TextureFormat::Bgra8UnormSrgb)
            | (TextureFormat::Bgra8UnormSrgb, TextureFormat::Bgra8Unorm)
    )
}
fn normalize_texture(
    d: TextureDescriptor,
    base: &str,
) -> Result<NormalizedTextureDescriptor, GraphError> {
    let bad = |message: &str, suffix: &str| {
        error(
            "GRAPH_PARAMETERS_INVALID",
            message,
            format!("{base}.texture.{suffix}"),
        )
    };
    let (extent, w, h, layers, relative) = match d.extent {
        TextureExtent::Absolute {
            width,
            height,
            depth_or_array_layers,
        } => (
            NormalizedTextureExtent::Absolute {
                width,
                height,
                depth_or_array_layers,
            },
            width,
            height,
            depth_or_array_layers,
            false,
        ),
        TextureExtent::SurfaceRelative {
            mut width,
            mut height,
            depth_or_array_layers,
        } => {
            if width.numerator == 0
                || width.denominator == 0
                || height.numerator == 0
                || height.denominator == 0
                || depth_or_array_layers == 0
            {
                return Err(bad(
                    "extent components and ratio terms must be nonzero",
                    "extent",
                ));
            }
            let g = gcd(width.numerator, width.denominator);
            width.numerator /= g;
            width.denominator /= g;
            let g = gcd(height.numerator, height.denominator);
            height.numerator /= g;
            height.denominator /= g;
            (
                NormalizedTextureExtent::SurfaceRelative {
                    width,
                    height,
                    depth_or_array_layers,
                },
                1,
                1,
                depth_or_array_layers,
                true,
            )
        }
    };
    if w == 0 || h == 0 || layers == 0 {
        return Err(bad("extent components must be nonzero", "extent"));
    }
    if relative && d.dimension != TextureDimension::D2 {
        return Err(bad("surface-relative textures must be d2", "extent"));
    }
    if d.dimension == TextureDimension::D1 && (h != 1 || layers != 1) {
        return Err(bad(
            "d1 textures require height and layers equal to one",
            "extent",
        ));
    }
    if d.format == TextureFormat::Depth32Float && d.dimension != TextureDimension::D2 {
        return Err(bad("depth textures must be d2", "dimension"));
    }
    if !matches!(d.sample_count, 1 | 4) {
        return Err(bad("sampleCount must be 1 or 4", "sampleCount"));
    }
    if d.mip_level_count == 0 {
        return Err(bad("mipLevelCount must be at least one", "mipLevelCount"));
    }
    if d.sample_count == 4
        && (d.dimension != TextureDimension::D2 || d.mip_level_count != 1 || layers != 1)
    {
        return Err(bad(
            "multisampled textures must be d2, single-mip, single-layer",
            "sampleCount",
        ));
    }
    let max_dim = w.max(h).max(if d.dimension == TextureDimension::D3 {
        layers
    } else {
        1
    });
    let max_mips = 32 - max_dim.leading_zeros();
    if !relative && d.mip_level_count > max_mips {
        return Err(bad(
            "mipLevelCount exceeds the full mip chain",
            "mipLevelCount",
        ));
    }
    let limit = if d.dimension == TextureDimension::D3 {
        2048
    } else {
        8192
    };
    if w > limit
        || h > limit
        || (d.dimension == TextureDimension::D3 && layers > 2048)
        || (d.dimension != TextureDimension::D3 && layers > 256)
    {
        return Err(bad("texture exceeds dimension limits", "extent"));
    }
    for (j, &view) in d.view_formats.iter().enumerate() {
        if view == d.format || !compatible_view(d.format, view) {
            return Err(bad(
                "view format must be compatible and exclude the base format",
                &format!("viewFormats[{j}]"),
            ));
        }
    }
    let mut views = d.view_formats;
    views.sort();
    views.dedup();
    Ok(NormalizedTextureDescriptor {
        dimension: d.dimension,
        format: d.format,
        extent,
        mip_level_count: d.mip_level_count,
        sample_count: d.sample_count,
        view_formats: views,
    })
}

fn decode(node: &Node, i: usize) -> Result<NormalizedParameters, GraphError> {
    let base = format!("nodes[{i}].parameters");
    let invalid =
        |e: serde_json::Error| error("GRAPH_PARAMETERS_INVALID", &e.to_string(), base.clone());
    macro_rules! empty {
        ($variant:expr) => {{
            serde_json::from_value::<Empty>(node.parameters.clone()).map_err(invalid)?;
            $variant
        }};
    }
    fn literal(value: &serde_json::Value, ty: SemanticType) -> Option<TypedLiteral> {
        let floats = |value: &serde_json::Value, n: usize| -> Option<Vec<f32>> {
            let values = value.as_array()?;
            if values.len() != n {
                return None;
            }
            values
                .iter()
                .map(|value| value.as_f64().map(|value| value as f32))
                .collect::<Option<Vec<_>>>()
                .filter(|values| values.iter().all(|value| value.is_finite()))
        };
        let vector = |n| floats(value, n);
        Some(match ty {
            SemanticType::Bool => TypedLiteral::Bool(value.as_bool()?),
            SemanticType::F32 => {
                let value = value.as_f64()? as f32;
                if !value.is_finite() {
                    return None;
                }
                TypedLiteral::F32(value)
            }
            SemanticType::U32 => TypedLiteral::U32(value.as_u64()?.try_into().ok()?),
            SemanticType::Vec2 => TypedLiteral::Vec2(vector(2)?.try_into().ok()?),
            SemanticType::Vec3 => TypedLiteral::Vec3(vector(3)?.try_into().ok()?),
            SemanticType::Vec4 => TypedLiteral::Vec4(vector(4)?.try_into().ok()?),
            SemanticType::U32x16 => TypedLiteral::U32x16(
                value
                    .as_array()?
                    .iter()
                    .map(|v| v.as_u64()?.try_into().ok())
                    .collect::<Option<Vec<u32>>>()?
                    .try_into()
                    .ok()?,
            ),
            SemanticType::LocalAabb => TypedLiteral::LocalAabb {
                min: floats(value.get("min")?, 3)?.try_into().ok()?,
                max: floats(value.get("max")?, 3)?.try_into().ok()?,
            },
            ty @ (SemanticType::Mat2 | SemanticType::Mat3 | SemanticType::Mat4) => {
                let n = match ty {
                    SemanticType::Mat2 => 2,
                    SemanticType::Mat3 => 3,
                    _ => 4,
                };
                let columns = value.as_array()?;
                if columns.len() != n {
                    return None;
                }
                let columns = columns
                    .iter()
                    .map(|v| floats(v, n))
                    .collect::<Option<Vec<_>>>()?;
                match ty {
                    SemanticType::Mat2 => TypedLiteral::Mat2(
                        columns
                            .into_iter()
                            .map(|v| v.try_into().ok())
                            .collect::<Option<Vec<_>>>()?
                            .try_into()
                            .ok()?,
                    ),
                    SemanticType::Mat3 => TypedLiteral::Mat3(
                        columns
                            .into_iter()
                            .map(|v| v.try_into().ok())
                            .collect::<Option<Vec<_>>>()?
                            .try_into()
                            .ok()?,
                    ),
                    _ => TypedLiteral::Mat4(
                        columns
                            .into_iter()
                            .map(|v| v.try_into().ok())
                            .collect::<Option<Vec<_>>>()?
                            .try_into()
                            .ok()?,
                    ),
                }
            }
            SemanticType::MeshData | SemanticType::Texture => return None,
        })
    }
    Ok(match node.executor.key.as_str() {
        "mesh" => empty!(NormalizedParameters::Mesh),
        "frustum_cull" => {
            let p: CullParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::FrustumCull { camera: p.camera }
        }
        "fullscreen_copy" => empty!(NormalizedParameters::FullscreenCopy),
        "color_balance" => {
            let p: ColorBalanceParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::ColorBalance {
                mode: p.mode,
                factor: range(p.factor, 0.0, 1.0, format!("{base}.factor"))?,
                lift: range(p.lift, -1.0, 1.0, format!("{base}.lift"))?,
                lift_color: color(p.lift_color, 0.0, 4.0, &format!("{base}.liftColor"))?,
                gamma: range(p.gamma, 0.01, 4.0, format!("{base}.gamma"))?,
                gamma_color: color(p.gamma_color, 0.0, 4.0, &format!("{base}.gammaColor"))?,
                gain: range(p.gain, 0.0, 4.0, format!("{base}.gain"))?,
                gain_color: color(p.gain_color, 0.0, 4.0, &format!("{base}.gainColor"))?,
                offset: range(p.offset, -1.0, 1.0, format!("{base}.offset"))?,
                offset_color: color(p.offset_color, 0.0, 2.0, &format!("{base}.offsetColor"))?,
                power: range(p.power, 0.01, 4.0, format!("{base}.power"))?,
                power_color: color(p.power_color, 0.0, 4.0, &format!("{base}.powerColor"))?,
                slope: range(p.slope, 0.0, 4.0, format!("{base}.slope"))?,
                slope_color: color(p.slope_color, 0.0, 4.0, &format!("{base}.slopeColor"))?,
            }
        }
        "exposure_contrast" => {
            let p: ExposureContrastParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::ExposureContrast {
                exposure_stops: range(
                    p.exposure_stops,
                    -10.0,
                    10.0,
                    format!("{base}.exposureStops"),
                )?,
                contrast: range(p.contrast, 0.01, 4.0, format!("{base}.contrast"))?,
                pivot: range(p.pivot, 0.001, 4.0, format!("{base}.pivot"))?,
                factor: range(p.factor, 0.0, 1.0, format!("{base}.factor"))?,
            }
        }
        "saturation" => {
            let p: SaturationParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::Saturation {
                saturation: range(p.saturation, 0.0, 4.0, format!("{base}.saturation"))?,
                factor: range(p.factor, 0.0, 1.0, format!("{base}.factor"))?,
            }
        }
        "channel_mixer" => {
            let p: ChannelMixerParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::ChannelMixer {
                red_output: components(p.red_output, -2.0, 2.0, &format!("{base}.redOutput"))?,
                green_output: components(
                    p.green_output,
                    -2.0,
                    2.0,
                    &format!("{base}.greenOutput"),
                )?,
                blue_output: components(p.blue_output, -2.0, 2.0, &format!("{base}.blueOutput"))?,
                factor: range(p.factor, 0.0, 1.0, format!("{base}.factor"))?,
            }
        }
        "bloom_extract" => {
            let p: BloomExtractParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::BloomExtract {
                threshold: range(p.threshold, 0.0, 64.0, format!("{base}.threshold"))?,
                knee: range(p.knee, 0.0, 1.0, format!("{base}.knee"))?,
            }
        }
        "bloom_blur" => {
            let p: BloomBlurParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            let x = range(p.direction[0], -1.0, 1.0, format!("{base}.direction[0]"))?;
            let y = range(p.direction[1], -1.0, 1.0, format!("{base}.direction[1]"))?;
            if (x.abs() + y.abs() - 1.0).abs() > 0.0001 {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "direction must be a unit axis",
                    format!("{base}.direction"),
                ));
            }
            NormalizedParameters::BloomBlur {
                direction: [x, y],
                radius: range(p.radius, 1.0, 16.0, format!("{base}.radius"))?,
            }
        }
        "bloom_composite" => {
            let p: BloomCompositeParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::BloomComposite {
                intensity: range(p.intensity, 0.0, 16.0, format!("{base}.intensity"))?,
            }
        }
        "luminance_edge" => {
            let p: LuminanceEdgeParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            NormalizedParameters::LuminanceEdge {
                strength: range(p.strength, 0.0, 16.0, format!("{base}.strength"))?,
            }
        }
        "frame_out" => {
            let p: FrameOutParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            let exposure_stops = range(
                p.exposure_stops,
                -10.0,
                10.0,
                format!("{base}.exposureStops"),
            )?;
            let background_color = components(
                p.background_color,
                0.0,
                1.0,
                &format!("{base}.backgroundColor"),
            )?;
            NormalizedParameters::FrameOut {
                surface_format: p.surface_format,
                dynamic_range: if p.hdr_enabled {
                    FrameDynamicRange::Hdr {
                        tone_mapper: p.tone_mapper,
                        exposure_stops,
                    }
                } else {
                    FrameDynamicRange::Sdr
                },
                output_transfer: p.output_transfer,
                scale_mode: p.scale_mode,
                filter: p.filter,
                background_color,
            }
        }
        "texture" => {
            let p: TextureParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            if matches!(
                p.residency,
                TextureResidency::History | TextureResidency::Readback
            ) {
                return Err(error(
                    "GRAPH_UNSUPPORTED_FEATURE",
                    "history and readback textures are unsupported",
                    format!("{base}.residency"),
                ));
            }
            let unsupported = |suffix: &str| {
                error(
                    "GRAPH_UNSUPPORTED_FEATURE",
                    "texture feature is unsupported",
                    format!("{base}.texture.{suffix}"),
                )
            };
            if p.texture.dimension != TextureDimension::D2 {
                return Err(unsupported("dimension"));
            }
            if p.texture.mip_level_count != 1 {
                return Err(unsupported("mipLevelCount"));
            }
            let depth_or_array_layers = match &p.texture.extent {
                TextureExtent::Absolute {
                    depth_or_array_layers,
                    ..
                }
                | TextureExtent::SurfaceRelative {
                    depth_or_array_layers,
                    ..
                } => *depth_or_array_layers,
            };
            if depth_or_array_layers != 1 {
                return Err(unsupported("extent.depthOrArrayLayers"));
            }
            NormalizedParameters::Texture {
                residency: p.residency,
                descriptor: normalize_texture(p.texture, &base)?,
            }
        }
        key if contract(key).is_some_and(|contract| contract.is_raster_draw()) => {
            let p: RasterParameters =
                serde_json::from_value(node.parameters.clone()).map_err(invalid)?;
            if !p.clear_depth.is_finite() || !(0.0..=1.0).contains(&p.clear_depth) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "clearDepth must be finite and in [0,1]",
                    format!("{base}.clearDepth"),
                ));
            }
            if p.clear_color.iter().any(|x| !x.is_finite()) {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "clearColor must be finite",
                    format!("{base}.clearColor"),
                ));
            }
            NormalizedParameters::Raster {
                depth_compare: p.depth_compare,
                depth_write_enabled: p.depth_write_enabled,
                clear_depth: p.clear_depth,
                clear_color: p.clear_color,
                predicate_default: p.predicate_default,
            }
        }
        key if contract(key)
            .is_some_and(|contract| contract.execution == ExecutionClass::Expression) =>
        {
            let contract = contract(key).unwrap();
            let object = node.parameters.as_object().ok_or_else(|| {
                error(
                    "GRAPH_PARAMETERS_INVALID",
                    "parameters must be an object",
                    base.clone(),
                )
            })?;
            let default_inputs: Vec<_> = contract
                .inputs
                .iter()
                .filter(|input| input.cardinality.max == 1)
                .collect();
            if object.len() != default_inputs.len() {
                return Err(error(
                    "GRAPH_PARAMETERS_INVALID",
                    "expression defaults must exactly match inputs",
                    base,
                ));
            }
            let mut defaults = Vec::with_capacity(default_inputs.len());
            for input in default_inputs {
                let key = format!("{}Default", input.name);
                let value = object.get(&key).ok_or_else(|| {
                    error(
                        "GRAPH_PARAMETERS_INVALID",
                        "missing expression default",
                        format!("{base}.{key}"),
                    )
                })?;
                let TypeConstraint::Exact(ty) = input.accepted else {
                    unreachable!()
                };
                defaults.push(literal(value, ty).ok_or_else(|| {
                    error(
                        "GRAPH_PARAMETERS_INVALID",
                        "invalid typed expression default",
                        format!("{base}.{key}"),
                    )
                })?);
            }
            NormalizedParameters::ExpressionDefaults { defaults }
        }
        _ => unreachable!(),
    })
}

fn accepts(c: TypeConstraint, ty: SemanticType) -> bool {
    match c {
        TypeConstraint::Exact(x) => x == ty,
        TypeConstraint::OneOf(xs) => xs.contains(&ty),
    }
}

pub fn compile(graph: Graph) -> Result<CompiledGraph, GraphError> {
    super::ast::validate_pipeline_declarations(&graph)?;
    if graph.nodes.len() > MAX_EXECUTIONS {
        return Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "node count exceeds 1024",
            "nodes",
        ));
    }
    let mut input_count = 0usize;
    for (i, node) in graph.nodes.iter().enumerate() {
        input_count = input_count.saturating_add(node.inputs.values().map(Vec::len).sum::<usize>());
        if input_count > 8192 {
            return Err(error(
                "GRAPH_LIMIT_EXCEEDED",
                "input count exceeds 8192",
                format!("nodes[{i}].inputs"),
            ));
        }
    }
    if graph.schema_version != 3 {
        return Err(GraphError::new(
            "GRAPH_SCHEMA_UNSUPPORTED",
            "schemaVersion must be 3",
        ));
    }
    validate_name_length(&graph.graph_id, "graphId")?;
    for (i, n) in graph.nodes.iter().enumerate() {
        for (value, path) in [
            (&n.id, format!("nodes[{i}].id")),
            (&n.executor.key, format!("nodes[{i}].executor.key")),
        ] {
            validate_name_length(value, path)?;
        }
        for (socket, refs) in &n.inputs {
            validate_name_length(socket, format!("nodes[{i}].inputs.{socket}"))?;
            for (index, r) in refs.iter().enumerate() {
                validate_name_length(&r.node, format!("nodes[{i}].inputs.{socket}[{index}].node"))?;
                validate_name_length(
                    &r.socket,
                    format!("nodes[{i}].inputs.{socket}[{index}].socket"),
                )?;
            }
        }
    }

    validate_name_grammar(&graph.graph_id, "graphId")?;
    let mut ids = HashMap::new();
    for (i, n) in graph.nodes.iter().enumerate() {
        for (value, path) in [
            (&n.id, format!("nodes[{i}].id")),
            (&n.executor.key, format!("nodes[{i}].executor.key")),
        ] {
            validate_name_grammar(value, path)?;
        }
        if ids.insert(n.id.as_str(), i).is_some() {
            return Err(error(
                "GRAPH_DUPLICATE_ID",
                "duplicate node id",
                format!("nodes[{i}].id"),
            ));
        }
        for (socket, refs) in &n.inputs {
            validate_name_grammar(socket, format!("nodes[{i}].inputs.{socket}"))?;
            for (index, r) in refs.iter().enumerate() {
                validate_name_grammar(
                    &r.node,
                    format!("nodes[{i}].inputs.{socket}[{index}].node"),
                )?;
                validate_name_grammar(
                    &r.socket,
                    format!("nodes[{i}].inputs.{socket}[{index}].socket"),
                )?;
            }
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        for (s, refs) in &n.inputs {
            for (index, r) in refs.iter().enumerate() {
                if !ids.contains_key(r.node.as_str()) {
                    return Err(error(
                        "GRAPH_UNKNOWN_NODE",
                        "unknown input node",
                        format!("nodes[{i}].inputs.{s}[{index}].node"),
                    ));
                }
            }
        }
    }
    let contracts: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            contract(&n.executor.key).ok_or_else(|| {
                error(
                    "GRAPH_UNKNOWN_EXECUTOR",
                    "unknown executor",
                    format!("nodes[{i}].executor.key"),
                )
            })
        })
        .collect::<Result<_, _>>()?;
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.executor.version != contracts[i].version {
            return Err(error(
                "GRAPH_EXECUTOR_VERSION_UNSUPPORTED",
                "unsupported executor version",
                format!("nodes[{i}].executor.version"),
            ));
        }
    }
    let params: Vec<_> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| decode(n, i))
        .collect::<Result<_, _>>()?;
    if graph
        .nodes
        .iter()
        .filter(|node| node.executor.key == "frame_out" && node.state == NodeState::Enabled)
        .count()
        != 1
    {
        return Err(error(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "exactly one frame_out is required",
            "nodes",
        ));
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        if n.state != NodeState::Enabled && n.executor.key != "frame_out" {
            return Err(error(
                "GRAPH_NODE_STATE_INVALID",
                "muted nodes are unsupported",
                format!("nodes[{i}].state"),
            ));
        }
    }

    // Socket validation is intentionally global and phased. In particular, no
    // cardinality or semantic error may hide a later structural socket error.
    for (i, n) in graph.nodes.iter().enumerate() {
        for name in n.inputs.keys() {
            if !contracts[i].inputs.iter().any(|s| s.name == name) {
                return Err(error(
                    "GRAPH_UNKNOWN_SOCKET",
                    "unknown input socket",
                    format!("nodes[{i}].inputs.{name}"),
                ));
            }
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        for (name, refs) in &n.inputs {
            for (index, r) in refs.iter().enumerate() {
                let pn = ids[r.node.as_str()];
                if !contracts[pn].outputs.iter().any(|out| out.name == r.socket) {
                    return Err(error(
                        "GRAPH_UNKNOWN_SOCKET",
                        "unknown output socket",
                        format!("nodes[{i}].inputs.{name}[{index}].socket"),
                    ));
                }
            }
        }
    }
    for (i, n) in graph.nodes.iter().enumerate() {
        for input in contracts[i].inputs {
            let sources = n.inputs.get(input.name);
            if sources.is_some_and(Vec::is_empty) {
                return Err(error(
                    "GRAPH_SOCKET_CARDINALITY",
                    "present input bindings must not be empty",
                    format!("nodes[{i}].inputs.{}", input.name),
                ));
            }
            let count = sources.map_or(0, Vec::len);
            if count < input.cardinality.min as usize || count > input.cardinality.max as usize {
                return Err(error(
                    "GRAPH_SOCKET_CARDINALITY",
                    "input binding count is outside the accepted range",
                    format!("nodes[{i}].inputs.{}", input.name),
                ));
            }
        }
    }

    let mut bound: Vec<BTreeMap<&str, BoundInput>> = vec![BTreeMap::new(); graph.nodes.len()];
    for (i, n) in graph.nodes.iter().enumerate() {
        for input in contracts[i].inputs {
            let Some(refs) = n.inputs.get(input.name) else {
                continue;
            };
            for (source_index, r) in refs.iter().enumerate() {
                let pn = ids[r.node.as_str()];
                let (ordinal, out) = contracts[pn]
                    .outputs
                    .iter()
                    .enumerate()
                    .find(|(_, o)| o.name == r.socket)
                    .expect("producer sockets were globally validated");
                if !accepts(input.accepted, out.semantic_type) {
                    return Err(error(
                        "GRAPH_SOCKET_TYPE_MISMATCH",
                        "socket type mismatch",
                        format!("nodes[{i}].inputs.{}[{source_index}]", input.name),
                    ));
                }
                if source_index == 0 {
                    bound[i].insert(
                        input.name,
                        BoundInput {
                            producer: OutputKey(pn, ordinal as u16),
                        },
                    );
                }
            }
        }
    }
    let mut edges = Vec::new();
    for i in 0..graph.nodes.len() {
        for (input_ordinal, input) in contracts[i].inputs.iter().enumerate() {
            for (source_index, b) in graph.nodes[i]
                .inputs
                .get(input.name)
                .into_iter()
                .flatten()
                .enumerate()
                .map(|(source_index, r)| {
                    let pn = ids[r.node.as_str()];
                    let ordinal = contracts[pn]
                        .outputs
                        .iter()
                        .position(|o| o.name == r.socket)
                        .unwrap();
                    (
                        source_index,
                        BoundInput {
                            producer: OutputKey(pn, ordinal as u16),
                        },
                    )
                })
            {
                edges.push(DependencyEdge {
                    from_node: b.producer.0,
                    from_socket: contracts[b.producer.0].outputs[b.producer.1 as usize]
                        .name
                        .into(),
                    producer_output_ordinal: b.producer.1,
                    to_node: i,
                    to_socket: input.name.into(),
                    consumer_input_ordinal: input_ordinal as u16,
                    resource: graph.nodes[i].inputs[input.name][source_index].clone(),
                });
            }
        }
    }
    edges.sort_by_key(|e| {
        (
            e.to_node,
            e.consumer_input_ordinal,
            e.from_node,
            e.producer_output_ordinal,
        )
    });
    let is_normalizable_target = |edge: &DependencyEdge| {
        contracts[edge.from_node].is_raster_draw()
            && contracts[edge.to_node].is_raster_draw()
            && matches!(
                contracts[edge.to_node].inputs[edge.consumer_input_ordinal as usize].role,
                InputRole::ColorTarget { .. } | InputRole::DepthTarget
            )
    };
    let is_exact_reader = |edge: &DependencyEdge| {
        let input = &contracts[edge.to_node].inputs[edge.consumer_input_ordinal as usize];
        input.role == InputRole::SampledTexture
            || (input.role == InputRole::SemanticRead
                && contracts[edge.from_node].outputs[edge.producer_output_ordinal as usize]
                    .semantic_type
                    == SemanticType::Texture)
    };
    // Compute demand from authored resource dependencies only. In particular,
    // a later WAR ordering edge must never resurrect its reader.
    let mut provisional_live = HashSet::new();
    let mut provisional_stack: Vec<_> = contracts
        .iter()
        .enumerate()
        .filter(|(i, contract)| {
            contract.inherently_observable && graph.nodes[*i].state == NodeState::Enabled
        })
        .map(|(i, _)| i)
        .collect();
    let mut authored_deps = vec![Vec::new(); graph.nodes.len()];
    for edge in &edges {
        authored_deps[edge.to_node].push(edge.from_node);
    }
    while let Some(node) = provisional_stack.pop() {
        if provisional_live.insert(node) {
            provisional_stack.extend(authored_deps[node].iter().copied());
        }
    }

    // Preserve authored readers that must finish before an attachment version is overwritten.
    let mut ordinary_consumers = HashMap::<OutputKey, Vec<usize>>::new();
    for edge in &edges {
        let key = OutputKey(edge.from_node, edge.producer_output_ordinal);
        if !is_normalizable_target(edge)
            && is_exact_reader(edge)
            && provisional_live.contains(&edge.to_node)
        {
            ordinary_consumers
                .entry(key)
                .or_default()
                .push(edge.to_node);
        }
    }
    let authored_depends_on = |consumer: usize, dependency: usize| {
        let mut seen = vec![false; graph.nodes.len()];
        let mut pending = vec![consumer];
        while let Some(node) = pending.pop() {
            if node == dependency {
                return true;
            }
            if !seen[node] {
                seen[node] = true;
                pending.extend(authored_deps[node].iter().copied());
            }
        }
        false
    };
    // Attachment normalization may replace an authored raster-to-raster edge,
    // but it must not erase the ordering constraint expressed by that edge.
    // A constraint opposite to authored cohort order is therefore reported by
    // the ordinary cycle detector below.
    let mut ordering_edges: Vec<_> = edges
        .iter()
        .filter(|edge| is_normalizable_target(edge))
        .map(|edge| (edge.from_node, edge.to_node))
        .collect();

    // Normalize raster attachment versions before liveness. Authored graphs may
    // express a render-pass cohort either as a chain or as direct siblings of
    // one texture. Turn both forms into an explicit, deterministic SSA chain.
    fn attachment_root(
        node: usize,
        ordinal: u16,
        bound: &[BTreeMap<&str, BoundInput>],
        contracts: &[&Contract],
        colors: &mut HashMap<(usize, u16), u8>,
        roots: &mut HashMap<(usize, u16), AttachmentRoot>,
    ) -> Result<AttachmentRoot, GraphError> {
        if let Some(&root) = roots.get(&(node, ordinal)) {
            return Ok(root);
        }
        if colors.get(&(node, ordinal)) == Some(&1) {
            return Err(error(
                "GRAPH_ATTACHMENT_LINEAGE_INVALID",
                "attachment lineage is cyclic",
                format!("nodes[{node}].inputs"),
            ));
        }
        colors.insert((node, ordinal), 1);
        let socket = if ordinal == 0 { "color" } else { "depth" };
        let root = match bound[node].get(socket).map(|binding| binding.producer) {
            None => AttachmentRoot::CompilerDefault { node, ordinal },
            Some(output)
                if !contracts[output.0].is_raster_draw()
                    && contracts[output.0].outputs[output.1 as usize].semantic_type
                        == SemanticType::Texture =>
            {
                AttachmentRoot::Authored(output)
            }
            Some(output) if contracts[output.0].is_raster_draw() && output.1 == ordinal => {
                attachment_root(output.0, ordinal, bound, contracts, colors, roots)?
            }
            Some(_) => {
                return Err(error(
                    "GRAPH_ATTACHMENT_LINEAGE_INVALID",
                    "attachment lineage has no texture or default root",
                    format!("nodes[{node}].inputs.{socket}"),
                ))
            }
        };
        colors.insert((node, ordinal), 2);
        roots.insert((node, ordinal), root);
        Ok(root)
    }

    let raster_nodes: Vec<_> = contracts
        .iter()
        .enumerate()
        .filter_map(|(i, contract)| contract.is_raster_draw().then_some(i))
        .collect();
    let mut aliases = HashMap::<OutputKey, OutputKey>::new();
    for ordinal in 0..=1u16 {
        let mut colors = HashMap::new();
        let mut roots = HashMap::new();
        let mut cohorts = BTreeMap::<AttachmentRoot, Vec<usize>>::new();
        for &node in &raster_nodes {
            let root = attachment_root(node, ordinal, &bound, &contracts, &mut colors, &mut roots)?;
            cohorts.entry(root).or_default().push(node);
        }
        for (root, mut members) in cohorts {
            members.sort_unstable();
            // An observed version ends its aliasing segment. Writers after the
            // cut continue the same physical lineage, but may not replace the
            // version seen by the reader.
            let mut segment_start = 0;
            for position in 0..members.len() {
                let output = OutputKey(members[position], ordinal);
                let at_cut = members.get(position + 1).is_some_and(|&next_writer| {
                    ordinary_consumers.get(&output).is_some_and(|readers| {
                        readers.iter().any(|&reader| {
                            contracts[reader].fullscreen_policy.is_some()
                                && !authored_depends_on(reader, next_writer)
                        })
                    })
                });
                if at_cut || position + 1 == members.len() {
                    let terminal = OutputKey(members[position], ordinal);
                    for &member in &members[segment_start..=position] {
                        aliases.insert(OutputKey(member, ordinal), terminal);
                    }
                    if let Some(&next_writer) = members.get(position + 1) {
                        for &member in &members[segment_start..=position] {
                            let output = OutputKey(member, ordinal);
                            for &reader in ordinary_consumers.get(&output).into_iter().flatten() {
                                if reader != next_writer {
                                    ordering_edges.push((reader, next_writer));
                                }
                            }
                        }
                    }
                    segment_start = position + 1;
                }
            }
            let socket = if ordinal == 0 { "color" } else { "depth" };
            for (position, &member) in members.iter().enumerate() {
                let target = if position == 0 {
                    match root {
                        AttachmentRoot::Authored(output) => Some(output),
                        AttachmentRoot::CompilerDefault { .. } => None,
                    }
                } else {
                    Some(OutputKey(members[position - 1], ordinal))
                };
                bound[member].remove(socket);
                if let Some(producer) = target {
                    bound[member].insert(socket, BoundInput { producer });
                }
            }
        }
    }
    ordering_edges.sort_unstable();
    ordering_edges.dedup();
    // Replace authored attachment dependencies with the canonical WAW chain,
    // and redirect observations of any cohort member to its terminal version.
    edges.retain(|edge| {
        !(contracts[edge.to_node].is_raster_draw()
            && (edge.to_socket == "color" || edge.to_socket == "depth"))
    });
    for &node in &raster_nodes {
        for (socket, ordinal) in [("color", 0u16), ("depth", 1u16)] {
            let Some(binding) = bound[node].get(socket) else {
                continue;
            };
            let input_ordinal = contracts[node]
                .inputs
                .iter()
                .position(|input| input.name == socket)
                .expect("raster target contract") as u16;
            edges.push(DependencyEdge {
                from_node: binding.producer.0,
                from_socket: contracts[binding.producer.0].outputs[binding.producer.1 as usize]
                    .name
                    .into(),
                producer_output_ordinal: binding.producer.1,
                to_node: node,
                to_socket: socket.into(),
                consumer_input_ordinal: input_ordinal,
                resource: NodeOutputRef {
                    node: graph.nodes[binding.producer.0].id.clone(),
                    socket: contracts[binding.producer.0].outputs[binding.producer.1 as usize]
                        .name
                        .into(),
                },
            });
            let _ = ordinal;
        }
    }
    for node in 0..graph.nodes.len() {
        for input in contracts[node].inputs.iter().filter(|input| {
            matches!(
                input.role,
                InputRole::SampledTexture | InputRole::SemanticRead
            )
        }) {
            let Some(binding) = bound[node].get_mut(input.name) else {
                continue;
            };
            if input.role == InputRole::SemanticRead
                && contracts[binding.producer.0].outputs[binding.producer.1 as usize].semantic_type
                    != SemanticType::Texture
            {
                continue;
            }
            if let Some(&terminal) = aliases.get(&binding.producer) {
                binding.producer = terminal;
            }
        }
    }
    for edge in &mut edges {
        if is_exact_reader(edge) {
            let key = OutputKey(edge.from_node, edge.producer_output_ordinal);
            if let Some(&terminal) = aliases.get(&key) {
                edge.from_node = terminal.0;
                edge.producer_output_ordinal = terminal.1;
                edge.from_socket = contracts[terminal.0].outputs[terminal.1 as usize]
                    .name
                    .into();
                edge.resource = NodeOutputRef {
                    node: graph.nodes[terminal.0].id.clone(),
                    socket: edge.from_socket.clone(),
                };
            }
        }
    }
    edges.sort_by_key(|e| {
        (
            e.to_node,
            e.consumer_input_ordinal,
            e.from_node,
            e.producer_output_ordinal,
        )
    });
    let mut deps = vec![Vec::new(); graph.nodes.len()];
    for edge in &edges {
        deps[edge.to_node].push(edge.from_node);
    }
    for node_deps in &mut deps {
        node_deps.sort();
        node_deps.dedup();
    }
    let mut live = HashSet::new();
    let mut stack: Vec<_> = contracts
        .iter()
        .enumerate()
        .filter(|(i, c)| c.inherently_observable && graph.nodes[*i].state == NodeState::Enabled)
        .map(|(i, _)| i)
        .collect();
    while let Some(i) = stack.pop() {
        if live.insert(i) {
            stack.extend(deps[i].iter().copied());
        }
    }
    // IDs are independent of scheduling: original node order, then contract output order.
    // Source nodes expose only outputs that survived active-edge/liveness analysis;
    // executable nodes retain their complete output shape for runtime lowering.
    let referenced_outputs: HashSet<_> = edges
        .iter()
        .filter(|edge| live.contains(&edge.to_node))
        .map(|edge| OutputKey(edge.from_node, edge.producer_output_ordinal))
        .collect();
    let mut output_ids = BTreeMap::new();
    let mut resource_meta = Vec::new();
    for i in 0..graph.nodes.len() {
        if live.contains(&i) {
            for (o, out) in contracts[i].outputs.iter().enumerate() {
                if out.semantic_type.is_virtual() {
                    continue;
                }
                let key = OutputKey(i, o as u16);
                if contracts[i].execution == ExecutionClass::Source
                    && !referenced_outputs.contains(&key)
                {
                    continue;
                }
                let id = resource_meta.len() as u32;
                output_ids.insert(key, id);
                resource_meta.push((i, o as u16, *out));
            }
        }
    }
    let all_outputs: usize = contracts
        .iter()
        .flat_map(|contract| contract.outputs)
        .filter(|output| !output.semantic_type.is_virtual())
        .count();
    let authored_materialized_output_count = output_ids.len();

    // Establish families and transitions without relying on a schedule.
    let mut families = Vec::new();
    let mut source_family = HashMap::new();
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) {
            continue;
        }
        let source = output_ids.get(&OutputKey(i, 0)).copied();
        match &params[i] {
            NormalizedParameters::Texture {
                residency,
                descriptor,
            } => {
                let id = families.len() as u32;
                let r = source.unwrap();
                source_family.insert(TransitionTargetKey::Authored(OutputKey(i, 0)), id);
                families.push(TextureFamily {
                    id,
                    key: TextureFamilyKey {
                        source_node: i as u32,
                        source_socket: 0,
                    },
                    source: TextureFamilySource::AuthoredTexture {
                        resource: r,
                        residency: *residency,
                        descriptor: descriptor.clone(),
                    },
                    lifetime: Lifetime {
                        first_use: 0,
                        last_use: 0,
                    },
                    versions: vec![],
                    usage: vec![],
                    allocation: None,
                    aliasable: false,
                });
            }
            _ => {}
        }
    }

    // Reserve compiler-owned roots without exposing incomplete public plan entries.
    // IDs remain stable while effective families and descriptors are resolved.
    let full_extent = NormalizedTextureExtent::SurfaceRelative {
        width: Ratio {
            numerator: 1,
            denominator: 1,
        },
        height: Ratio {
            numerator: 1,
            denominator: 1,
        },
        depth_or_array_layers: 1,
    };
    let authored_family_count = families.len();
    let mut default_roots: Vec<DefaultDraft> = Vec::new();
    let mut default_targets = HashMap::new();
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) || !contracts[i].is_raster_draw() {
            continue;
        }
        for (socket, role, format, opposite) in [
            (
                "color",
                CompilerTextureRole::ColorTarget,
                TextureFormat::Rgba16Float,
                "depth",
            ),
            (
                "depth",
                CompilerTextureRole::DepthTarget,
                TextureFormat::Depth32Float,
                "color",
            ),
        ]
        .into_iter()
        {
            if bound[i].contains_key(socket) {
                continue;
            }
            let input_ordinal = contracts[i]
                .inputs
                .iter()
                .position(|input| input.name == socket)
                .expect("compiler target has a contract input")
                as u16;
            let key = TransitionTargetKey::CompilerDefaultInput {
                owner_node: i,
                input_ordinal,
            };
            let resource = (authored_materialized_output_count + default_roots.len()) as u32;
            let family = (authored_family_count + default_roots.len()) as u32;
            default_targets.insert((i, socket), key);
            source_family.insert(key, family);
            let _ = opposite;
            default_roots.push(DefaultDraft {
                key,
                resource,
                family,
                owner_node: i,
                input_ordinal,
                socket,
                role,
                format,
                descriptor: DraftDescriptor::Deferred,
            });
        }
    }
    let mut transitions: Vec<TextureTransition> = Vec::new();
    let mut transitions_by_target: BTreeMap<TransitionTargetKey, Vec<usize>> = BTreeMap::new();
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) {
            continue;
        }
        let transition_sockets: &[(&str, u16)] = match contracts[i] {
            ref contract if contract.is_raster_draw() => &[("color", 0), ("depth", 1)],
            _ if contracts[i].fullscreen_policy.is_some() => &[("colorTarget", 0)],
            _ => continue,
        };
        for &(input_socket, output_ordinal) in transition_sockets {
            let transition = TextureTransition {
                writer_node: i,
                input_socket,
                target: bound[i]
                    .get(input_socket)
                    .map(|b| TransitionTargetKey::Authored(b.producer))
                    .unwrap_or_else(|| default_targets[&(i, input_socket)]),
                output: OutputKey(i, output_ordinal),
            };
            let index = transitions.len();
            transitions.push(transition);
            transitions_by_target
                .entry(transition.target)
                .or_default()
                .push(index);
        }
    }

    fn resolve_transition(
        output: OutputKey,
        transitions: &[TextureTransition],
        transition_for_output: &HashMap<OutputKey, usize>,
        source_family: &HashMap<TransitionTargetKey, u32>,
        colors: &mut HashMap<OutputKey, u8>,
        resolved: &mut HashMap<OutputKey, ResolvedTransition>,
    ) -> ResolvedTransition {
        if let Some(&value) = resolved.get(&output) {
            return value;
        }
        if colors.get(&output) == Some(&1) {
            return ResolvedTransition::Cyclic;
        }
        colors.insert(output, 1);
        let transition = transitions[transition_for_output[&output]];
        let value = if let Some(&family) = source_family.get(&transition.target) {
            ResolvedTransition::Resolved {
                family,
                version: 0,
                target: transition.target,
            }
        } else if let TransitionTargetKey::Authored(target_output) = transition.target {
            if !transition_for_output.contains_key(&target_output) {
                ResolvedTransition::Cyclic
            } else {
                match resolve_transition(
                    target_output,
                    transitions,
                    transition_for_output,
                    source_family,
                    colors,
                    resolved,
                ) {
                    ResolvedTransition::Resolved {
                        family, version, ..
                    } => ResolvedTransition::Resolved {
                        family,
                        version: version + 1,
                        target: transition.target,
                    },
                    ResolvedTransition::Cyclic => ResolvedTransition::Cyclic,
                }
            }
        } else {
            ResolvedTransition::Cyclic
        };
        colors.insert(output, 2);
        resolved.insert(output, value);
        value
    }

    let transition_for_output: HashMap<_, _> = transitions
        .iter()
        .enumerate()
        .map(|(index, transition)| (transition.output, index))
        .collect();
    let mut resolved = HashMap::new();
    let mut colors = HashMap::new();
    for transition in &transitions {
        resolve_transition(
            transition.output,
            &transitions,
            &transition_for_output,
            &source_family,
            &mut colors,
            &mut resolved,
        );
    }
    let mut version_of: HashMap<OutputKey, (u32, u32, u32)> = HashMap::new();
    for transition in &transitions {
        if let ResolvedTransition::Resolved {
            family,
            version,
            target,
        } = resolved[&transition.output]
        {
            let target_id = match target {
                TransitionTargetKey::Authored(key) => output_ids[&key],
                TransitionTargetKey::CompilerDefaultInput { .. } => {
                    default_roots
                        .iter()
                        .find(|root| root.key == target)
                        .unwrap()
                        .resource
                }
            };
            version_of.insert(transition.output, (family, version, target_id));
        }
    }

    let mut outgoing_edges = vec![Vec::new(); graph.nodes.len()];
    for (index, edge) in edges.iter().enumerate() {
        if live.contains(&edge.from_node) && live.contains(&edge.to_node) {
            outgoing_edges[edge.from_node].push(index);
        }
    }
    let mut ordering_outgoing = vec![Vec::new(); graph.nodes.len()];
    for &(from, to) in &ordering_edges {
        if live.contains(&from) && live.contains(&to) {
            ordering_outgoing[from].push(to);
        }
    }
    for outgoing in &mut outgoing_edges {
        outgoing.sort_by_key(|&index| {
            let edge = &edges[index];
            (
                edge.to_node,
                edge.producer_output_ordinal,
                edge.consumer_input_ordinal,
            )
        });
    }

    // Same-pass hazards are global and precede every duplicate-writer diagnostic.
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) {
            continue;
        }
        if contracts[i].is_raster_draw() {
            if bound[i].get("color").map(|b| b.producer)
                == bound[i].get("depth").map(|b| b.producer)
                && bound[i].contains_key("color")
                || matches!((version_of.get(&OutputKey(i, 0)), version_of.get(&OutputKey(i, 1))), (Some((cf, _, _)), Some((df, _, _))) if cf == df)
            {
                return Err(error(
                    "GRAPH_SAME_PASS_HAZARD",
                    "color and depth use one texture family",
                    format!("nodes[{i}].inputs"),
                ));
            }
        } else if contracts[i].fullscreen_policy.is_some() {
            let hazard = contracts[i].inputs.iter().filter(|input| matches!(input.role, InputRole::SampledTexture)).any(|input| matches!((version_of.get(&bound[i][input.name].producer), version_of.get(&OutputKey(i, 0))), (Some((sf, _, _)), Some((tf, _, _))) if sf == tf));
            if hazard {
                return Err(error(
                    "GRAPH_SAME_PASS_HAZARD",
                    "copy source and target use one texture family",
                    format!("nodes[{i}].inputs"),
                ));
            }
        }
    }
    let mut first_writer = BTreeMap::new();
    for transition in &transitions {
        if first_writer
            .insert(transition.target, transition.writer_node)
            .is_some_and(|writer| writer != transition.writer_node)
        {
            return Err(error(
                "GRAPH_DUPLICATE_WRITER",
                "texture version has multiple writers",
                format!(
                    "nodes[{}].inputs.{}",
                    transition.writer_node, transition.input_socket
                ),
            ));
        }
    }

    // Infer draft descriptors without recursion. Unknown and invalid dependencies
    // remain deferred so descriptor diagnostics never steal cycle diagnostics.
    for _ in 0..default_roots.len() {
        let mut changed = false;
        for index in 0..default_roots.len() {
            if matches!(default_roots[index].descriptor, DraftDescriptor::Known(_)) {
                continue;
            }
            let owner = default_roots[index].owner_node;
            let opposite_output = if default_roots[index].role == CompilerTextureRole::ColorTarget {
                OutputKey(owner, 1)
            } else {
                OutputKey(owner, 0)
            };
            let Some(&(opposite_family, _, _)) = version_of.get(&opposite_output) else {
                continue;
            };
            let opposite_descriptor =
                known_family_descriptor(opposite_family, &families, &default_roots);
            let extent = if opposite_family as usize >= authored_family_count
                && default_roots
                    .get(opposite_family as usize - authored_family_count)
                    .is_some_and(|draft| draft.owner_node == owner)
            {
                Some(full_extent.clone())
            } else {
                opposite_descriptor.map(|descriptor| descriptor.extent.clone())
            };
            if let Some(extent) = extent {
                default_roots[index].descriptor =
                    DraftDescriptor::Known(NormalizedTextureDescriptor {
                        dimension: TextureDimension::D2,
                        format: default_roots[index].format,
                        extent,
                        mip_level_count: 1,
                        sample_count: opposite_descriptor
                            .map_or(1, |descriptor| descriptor.sample_count),
                        view_formats: vec![],
                    });
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Classify sampled edges before descriptor and stale-version validation. A
    // demanded resolve is keyed by the exact symbolic output, not its family.
    let mut resolve_demands = BTreeSet::<OutputKey>::new();
    for edge in &edges {
        if !live.contains(&edge.to_node)
            || contracts[edge.to_node].inputs[edge.consumer_input_ordinal as usize].role
                != InputRole::SampledTexture
        {
            continue;
        }
        let key = OutputKey(edge.from_node, edge.producer_output_ordinal);
        let Some(&(family, _, _)) = version_of.get(&key) else {
            if matches!(resolved.get(&key), Some(ResolvedTransition::Cyclic)) {
                continue;
            }
            if source_family
                .get(&TransitionTargetKey::Authored(key))
                .and_then(|&family| known_family_descriptor(family, &families, &default_roots))
                .is_some_and(|descriptor| descriptor.sample_count == 4)
            {
                return Err(error(
                    "GRAPH_ILLEGAL_ACCESS",
                    "unproduced multisampled texture cannot be sampled",
                    format!("nodes[{}].inputs.{}", edge.to_node, edge.to_socket),
                ));
            }
            continue;
        };
        let Some(descriptor) = known_family_descriptor(family, &families, &default_roots) else {
            continue;
        };
        if descriptor.sample_count == 1 {
            continue;
        }
        if descriptor.format == TextureFormat::Depth32Float {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "multisampled depth texture cannot be sampled",
                format!("nodes[{}].inputs.{}", edge.to_node, edge.to_socket),
            ));
        }
        if !contracts[edge.from_node].is_raster_draw() || edge.producer_output_ordinal != 0 {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "multisampled color texture is not a produced pipeline color",
                format!("nodes[{}].inputs.{}", edge.to_node, edge.to_socket),
            ));
        }
        resolve_demands.insert(key);
    }

    // Every ordinary texture reader must execute before a successor overwrites
    // its allocation. Resolve demands read the attachment during its producer.
    let mut reachability = HashMap::new();
    for (i, contract) in contracts.iter().enumerate() {
        if !live.contains(&i) {
            continue;
        }
        for input in contract
            .inputs
            .iter()
            .filter(|input| input.role == InputRole::SampledTexture)
        {
            let key = bound[i][input.name].producer;
            if resolve_demands.contains(&key) || !version_of.contains_key(&key) {
                continue;
            }
            let Some(next_indices) = transitions_by_target.get(&TransitionTargetKey::Authored(key))
            else {
                continue;
            };
            let [next_index] = next_indices.as_slice() else {
                continue;
            };
            let next = transitions[*next_index];
            if i != next.writer_node
                && !reaches(
                    i,
                    next.writer_node,
                    &outgoing_edges,
                    &ordering_outgoing,
                    &edges,
                    &live,
                    &mut reachability,
                )
            {
                return Err(error(
                    "GRAPH_RESOURCE_VERSION_INVALID",
                    "older texture version may be read after its successor",
                    format!("nodes[{i}].inputs.{}", input.name),
                ));
            }
        }
    }

    // Validate every independently resolved attachment before graph cycle reporting.
    for i in 0..graph.nodes.len() {
        if !live.contains(&i) || !contracts[i].is_raster_draw() {
            continue;
        }
        let cd = version_of
            .get(&OutputKey(i, 0))
            .and_then(|&(family, _, _)| known_family_descriptor(family, &families, &default_roots));
        let dd = version_of
            .get(&OutputKey(i, 1))
            .and_then(|&(family, _, _)| known_family_descriptor(family, &families, &default_roots));
        let ok_depth = dd.is_none_or(|dd| {
            dd.dimension == TextureDimension::D2
                && dd.format == TextureFormat::Depth32Float
                && matches!(dd.sample_count, 1 | 4)
                && dd.mip_level_count == 1
                && dd.view_formats.is_empty()
                && extent_layers(&dd.extent) == 1
        });
        let ok_color = cd.is_none_or(|cd| {
            cd.dimension == TextureDimension::D2
                && cd.format != TextureFormat::Depth32Float
                && matches!(cd.sample_count, 1 | 4)
                && cd.mip_level_count == 1
                && cd.view_formats.is_empty()
                && extent_layers(&cd.extent) == 1
        });
        if !ok_color {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "color attachment is invalid",
                format!("nodes[{i}].inputs.color"),
            ));
        }
        if !ok_depth {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "depth attachment is invalid",
                format!("nodes[{i}].inputs.depth"),
            ));
        }
        if let (Some(cd), Some(dd)) = (cd, dd) {
            if cd.dimension != dd.dimension
                || cd.extent != dd.extent
                || cd.sample_count != dd.sample_count
            {
                return Err(error(
                    "GRAPH_ILLEGAL_ACCESS",
                    "attachments are incompatible",
                    format!("nodes[{i}].inputs"),
                ));
            }
        }
    }

    for i in 0..graph.nodes.len() {
        if !live.contains(&i) || contracts[i].fullscreen_policy.is_none() {
            continue;
        }
        let source = sampled_descriptor(
            bound[i]["source"].producer,
            &version_of,
            &resolved,
            &families,
            &default_roots,
        )?;
        let source_descriptor = match source {
            SampledDescriptor::Known(descriptor) => Some(descriptor),
            SampledDescriptor::Deferred | SampledDescriptor::Unproduced => None,
        };
        let target_family_id = version_of
            .get(&OutputKey(i, 0))
            .map(|&(family, _, _)| family)
            .or_else(|| {
                source_family
                    .get(&TransitionTargetKey::Authored(
                        bound[i]["colorTarget"].producer,
                    ))
                    .copied()
            });
        let target_descriptor = target_family_id
            .and_then(|family| known_family_descriptor(family, &families, &default_roots));
        let bloom = if contracts[i].fullscreen_policy == Some(FullscreenPolicy::BloomComposite) {
            Some(sampled_descriptor(
                bound[i]["bloom"].producer,
                &version_of,
                &resolved,
                &families,
                &default_roots,
            )?)
        } else {
            None
        };
        let bloom_descriptor = match &bloom {
            Some(SampledDescriptor::Known(descriptor)) => Some(*descriptor),
            _ => None,
        };
        let source_ok = source_descriptor.is_none_or(|descriptor| {
            descriptor.format == TextureFormat::Rgba16Float
                && (is_single_view_d2(descriptor)
                    || resolve_demands.contains(&bound[i]["source"].producer))
        });
        let target_ok = target_descriptor.is_none_or(|descriptor| {
            is_single_view_d2(descriptor)
                && match contracts[i].fullscreen_policy {
                    Some(FullscreenPolicy::Copy) => {
                        descriptor.format != TextureFormat::Depth32Float
                    }
                    Some(FullscreenPolicy::BloomExtract)
                    | Some(FullscreenPolicy::HdrSameExtent)
                    | Some(FullscreenPolicy::BloomComposite) => {
                        descriptor.format == TextureFormat::Rgba16Float
                    }
                    _ => false,
                }
        });
        let bloom_ok = bloom_descriptor.is_none_or(|descriptor| {
            descriptor.format == TextureFormat::Rgba16Float
                && (is_single_view_d2(descriptor)
                    || resolve_demands.contains(&bound[i]["bloom"].producer))
        });
        let extent_ok = match contracts[i].fullscreen_policy {
            Some(FullscreenPolicy::Copy)
            | Some(FullscreenPolicy::HdrSameExtent)
            | Some(FullscreenPolicy::BloomComposite) => {
                !matches!((source_descriptor, target_descriptor), (Some(source), Some(target)) if source.extent != target.extent)
            }
            Some(FullscreenPolicy::BloomExtract) => true,
            _ => false,
        };
        if !source_ok || !target_ok || !bloom_ok || !extent_ok {
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                "fullscreen textures are incompatible",
                format!("nodes[{i}].inputs"),
            ));
        }
        if matches!(source, SampledDescriptor::Unproduced) {
            return Err(error(
                "GRAPH_UNINITIALIZED_RESOURCE",
                "copy source is not produced",
                format!("nodes[{i}].inputs.source"),
            ));
        }
        if matches!(bloom, Some(SampledDescriptor::Unproduced)) {
            return Err(error(
                "GRAPH_UNINITIALIZED_RESOURCE",
                "bloom source is not produced",
                format!("nodes[{i}].inputs.bloom"),
            ));
        }
    }

    // Frame output must consume an initialized, produced, filterable color texture.
    for (i, contract) in contracts.iter().enumerate() {
        if !live.contains(&i) || contract.key != "frame_out" {
            continue;
        }
        let key = bound[i]["color"].producer;
        let Some(&(family, _, _)) = version_of.get(&key) else {
            if !matches!(resolved.get(&key), Some(ResolvedTransition::Cyclic)) {
                return Err(error(
                    "GRAPH_UNINITIALIZED_RESOURCE",
                    "frame output source is not produced",
                    format!("nodes[{i}].inputs.color"),
                ));
            }
            continue;
        };
        let Some(descriptor) = known_family_descriptor(family, &families, &default_roots) else {
            continue;
        };
        let NormalizedParameters::FrameOut { dynamic_range, .. } = &params[i] else {
            unreachable!()
        };
        let mut effective = descriptor.clone();
        if resolve_demands.contains(&key) {
            effective.sample_count = 1;
        }
        if !frame_out_source_compatible(&effective, dynamic_range) {
            let message = match dynamic_range {
                FrameDynamicRange::Hdr { .. } => "HDR frame output requires rgba16_float",
                FrameDynamicRange::Sdr => {
                    "SDR frame output requires a linear filterable color texture"
                }
            };
            return Err(error(
                "GRAPH_ILLEGAL_ACCESS",
                message,
                format!("nodes[{i}].inputs.color"),
            ));
        }
    }

    // Stable Kahn scheduling is deliberately after resource and access validation.
    let mut indegree = vec![0; graph.nodes.len()];
    for &node in &live {
        indegree[node] = edges
            .iter()
            .filter(|edge| edge.to_node == node && live.contains(&edge.from_node))
            .count()
            + ordering_edges
                .iter()
                .filter(|(from, to)| *to == node && live.contains(from))
                .count();
    }
    let mut queue = BinaryHeap::new();
    for &node in &live {
        if indegree[node] == 0 {
            queue.push(Reverse(node));
        }
    }
    let mut order = Vec::new();
    while let Some(Reverse(node)) = queue.pop() {
        order.push(node);
        for &edge_index in &outgoing_edges[node] {
            let consumer = edges[edge_index].to_node;
            indegree[consumer] -= 1;
            if indegree[consumer] == 0 {
                queue.push(Reverse(consumer));
            }
        }
        for &consumer in &ordering_outgoing[node] {
            indegree[consumer] -= 1;
            if indegree[consumer] == 0 {
                queue.push(Reverse(consumer));
            }
        }
    }
    if order.len() != live.len() {
        let residual: Vec<_> = (0..graph.nodes.len())
            .map(|node| live.contains(&node) && indegree[node] != 0)
            .collect();
        fn cycle_dfs(
            node: usize,
            outgoing_edges: &[Vec<usize>],
            ordering_outgoing: &[Vec<usize>],
            edges: &[DependencyEdge],
            residual: &[bool],
            colors: &mut [u8],
            node_stack: &mut Vec<usize>,
            edge_stack: &mut Vec<usize>,
        ) -> Option<Vec<usize>> {
            colors[node] = 1;
            node_stack.push(node);
            for &edge_index in &outgoing_edges[node] {
                let to = edges[edge_index].to_node;
                if !residual[to] {
                    continue;
                }
                if colors[to] == 0 {
                    edge_stack.push(edge_index);
                    if let Some(cycle) = cycle_dfs(
                        to,
                        outgoing_edges,
                        ordering_outgoing,
                        edges,
                        residual,
                        colors,
                        node_stack,
                        edge_stack,
                    ) {
                        return Some(cycle);
                    }
                    edge_stack.pop();
                } else if colors[to] == 1 {
                    let position = node_stack
                        .iter()
                        .position(|&stacked| stacked == to)
                        .unwrap();
                    let mut cycle = edge_stack[position..].to_vec();
                    cycle.push(edge_index);
                    return Some(cycle);
                }
            }
            for &to in &ordering_outgoing[node] {
                if !residual[to] {
                    continue;
                }
                if colors[to] == 0 {
                    if cycle_dfs(
                        to,
                        outgoing_edges,
                        ordering_outgoing,
                        edges,
                        residual,
                        colors,
                        node_stack,
                        edge_stack,
                    )
                    .is_some()
                    {
                        // Ordering edges are synthetic and have no authored
                        // socket payload, but still constitute a real cycle.
                        return Some(Vec::new());
                    }
                } else if colors[to] == 1 {
                    return Some(Vec::new());
                }
            }
            node_stack.pop();
            colors[node] = 2;
            None
        }
        let mut colors = vec![0; graph.nodes.len()];
        let mut cycle = None;
        for node in 0..graph.nodes.len() {
            if residual[node] && colors[node] == 0 {
                cycle = cycle_dfs(
                    node,
                    &outgoing_edges,
                    &ordering_outgoing,
                    &edges,
                    &residual,
                    &mut colors,
                    &mut Vec::new(),
                    &mut Vec::new(),
                );
                if cycle.is_some() {
                    break;
                }
            }
        }
        let mut graph_error = GraphError::new("GRAPH_CYCLE", "live graph contains a cycle");
        let payload: Vec<_> = cycle
            .unwrap_or_default()
            .into_iter()
            .map(|index| {
                let edge = &edges[index];
                serde_json::json!({
                    "fromNode": graph.nodes[edge.from_node].id,
                    "fromSocket": edge.from_socket,
                    "toNode": graph.nodes[edge.to_node].id,
                    "toSocket": edge.to_socket,
                    "resource": edge.resource,
                })
            })
            .collect();
        graph_error.details =
            serde_json::json!({"message":graph_error.message,"kind":"cycle","edges":payload});
        return Err(graph_error);
    }
    if transitions
        .iter()
        .any(|transition| matches!(resolved[&transition.output], ResolvedTransition::Cyclic))
        || default_roots
            .iter()
            .any(|draft| matches!(draft.descriptor, DraftDescriptor::Deferred))
    {
        return Err(error(
            "GRAPH_RESOURCE_VERSION_INVALID",
            "texture predecessor is unresolved in an acyclic graph",
            "resources",
        ));
    }

    for draft in &default_roots {
        let DraftDescriptor::Known(descriptor) = &draft.descriptor else {
            unreachable!("deferred drafts rejected above")
        };
        families.push(TextureFamily {
            id: draft.family,
            key: TextureFamilyKey {
                source_node: draft.owner_node as u32,
                source_socket: draft.input_ordinal,
            },
            source: TextureFamilySource::CompilerDefaultInput {
                resource: draft.resource,
                owner_node_index: draft.owner_node as u32,
                input_ordinal: draft.input_ordinal,
                role: draft.role,
                descriptor: descriptor.clone(),
            },
            lifetime: Lifetime {
                first_use: 0,
                last_use: 0,
            },
            versions: vec![],
            usage: vec![],
            allocation: None,
            aliasable: false,
        });
    }
    for transition in &transitions {
        if let Some(&(family, version, target)) = version_of.get(&transition.output) {
            families[family as usize].versions.push(TextureVersion {
                version,
                resource: output_ids[&transition.output],
                target,
                initialized: true,
                stored: true,
                lifetime: Lifetime {
                    first_use: 0,
                    last_use: 0,
                },
            });
        }
    }
    for family in &mut families {
        family.versions.sort_by_key(|version| version.version);
        if family
            .versions
            .iter()
            .enumerate()
            .any(|(index, version)| version.version != index as u32)
        {
            return Err(error(
                "GRAPH_RESOURCE_VERSION_INVALID",
                "texture versions must form a dense linear chain",
                "resources",
            ));
        }
    }

    let mut resources = Vec::new();
    for (i, o, out) in resource_meta {
        let key = OutputKey(i, o);
        let id = output_ids[&key];
        let plan = match out.semantic_type {
            SemanticType::Texture if matches!(params[i], NormalizedParameters::Texture { .. }) => {
                if let NormalizedParameters::Texture {
                    residency,
                    descriptor,
                } = &params[i]
                {
                    ResourcePlan::TextureSource {
                        family: source_family[&TransitionTargetKey::Authored(key)],
                        residency: *residency,
                        descriptor: descriptor.clone(),
                    }
                } else {
                    unreachable!()
                }
            }
            SemanticType::Texture => {
                let (f, v, t) = version_of[&key];
                ResourcePlan::Texture {
                    family: f,
                    version: v,
                    target: t,
                    initialized: true,
                    stored: true,
                    allocation: None,
                }
            }
            SemanticType::MeshData => ResourcePlan::MeshData,
            SemanticType::Bool
            | SemanticType::F32
            | SemanticType::U32
            | SemanticType::Vec2
            | SemanticType::Vec3
            | SemanticType::Vec4
            | SemanticType::Mat2
            | SemanticType::Mat3
            | SemanticType::Mat4
            | SemanticType::U32x16
            | SemanticType::LocalAabb => {
                unreachable!("pure expression outputs are never materialized")
            }
        };
        resources.push(CompiledResource {
            original_node_index: i as u32,
            origin: ResourceOrigin::AuthoredOutput {
                node: graph.nodes[i].id.clone(),
                socket: out.name.into(),
                output_ordinal: o,
            },
            semantic_type: out.semantic_type,
            producer_execution: None,
            lifetime: None,
            plan,
        });
        let _ = id;
    }
    for draft in &default_roots {
        let DraftDescriptor::Known(descriptor) = &draft.descriptor else {
            unreachable!("deferred drafts rejected above")
        };
        resources.push(CompiledResource {
            original_node_index: draft.owner_node as u32,
            origin: ResourceOrigin::CompilerDefaultInput {
                owner_node_index: draft.owner_node as u32,
                input_ordinal: draft.input_ordinal,
                socket: draft.socket.into(),
                role: draft.role,
            },
            semantic_type: SemanticType::Texture,
            producer_execution: None,
            lifetime: None,
            plan: ResourcePlan::TextureSource {
                family: draft.family,
                residency: TextureResidency::Transient,
                descriptor: descriptor.clone(),
            },
        });
    }
    // A multisampled pipeline color remains the authored attachment version. Sampling
    // that exact version instead addresses one compiler-owned fixed-function resolve.
    let mut resolve_resources = BTreeMap::new();
    for producer in resolve_demands {
        let (family, _, _) = version_of[&producer];
        let descriptor = family_descriptor(&families[family as usize]);
        let source_resource = output_ids[&producer];
        let root = resources.len() as u32;
        let resource = root + 1;
        let family_id = families.len() as u32;
        let mut resolved_descriptor = descriptor.clone();
        resolved_descriptor.sample_count = 1;
        resolve_resources.insert(producer, resource);
        resources.push(CompiledResource {
            original_node_index: producer.0 as u32,
            origin: ResourceOrigin::CompilerColorResolve {
                producer_node_index: producer.0 as u32,
                output_ordinal: producer.1,
                source_resource,
            },
            semantic_type: SemanticType::Texture,
            producer_execution: None,
            lifetime: None,
            plan: ResourcePlan::TextureSource {
                family: family_id,
                residency: TextureResidency::Transient,
                descriptor: resolved_descriptor.clone(),
            },
        });
        resources.push(CompiledResource {
            original_node_index: producer.0 as u32,
            origin: ResourceOrigin::CompilerColorResolve {
                producer_node_index: producer.0 as u32,
                output_ordinal: producer.1,
                source_resource,
            },
            semantic_type: SemanticType::Texture,
            producer_execution: None,
            lifetime: None,
            plan: ResourcePlan::Texture {
                family: family_id,
                version: 0,
                target: root,
                initialized: true,
                stored: true,
                allocation: None,
            },
        });
        families.push(TextureFamily {
            id: family_id,
            key: TextureFamilyKey {
                source_node: producer.0 as u32,
                source_socket: producer.1,
            },
            source: TextureFamilySource::CompilerColorResolve {
                resource: root,
                descriptor: resolved_descriptor,
                producer_node_index: producer.0 as u32,
                output_ordinal: producer.1,
                source_resource,
            },
            lifetime: Lifetime {
                first_use: 0,
                last_use: 0,
            },
            versions: vec![TextureVersion {
                version: 0,
                resource,
                target: root,
                initialized: true,
                stored: true,
                lifetime: Lifetime {
                    first_use: 0,
                    last_use: 0,
                },
            }],
            usage: vec![],
            allocation: None,
            aliasable: false,
        });
    }
    let mut executions = Vec::new();
    for &i in &order {
        if matches!(
            contracts[i].execution,
            ExecutionClass::Source | ExecutionClass::Expression
        ) {
            continue;
        }
        let input_resource = |s: &str| {
            let key = bound[i][s].producer;
            let resource = output_ids[&key];
            if contracts[i]
                .inputs
                .iter()
                .any(|input| input.name == s && input.role == InputRole::SampledTexture)
            {
                resolve_resources.get(&key).copied().unwrap_or(resource)
            } else {
                resource
            }
        };
        let mut inputs = Vec::new();
        for (input_ordinal, s) in contracts[i].inputs.iter().enumerate() {
            if s.role != InputRole::Expression {
                let resource = if let Some(b) = bound[i].get(s.name) {
                    let resource = output_ids[&b.producer];
                    if s.role == InputRole::SampledTexture {
                        resolve_resources
                            .get(&b.producer)
                            .copied()
                            .unwrap_or(resource)
                    } else {
                        resource
                    }
                } else if s.default_policy == InputDefaultPolicy::CompilerTexture {
                    let key = TransitionTargetKey::CompilerDefaultInput {
                        owner_node: i,
                        input_ordinal: input_ordinal as u16,
                    };
                    default_roots
                        .iter()
                        .find(|root| root.key == key)
                        .expect("default policy materialized a root")
                        .resource
                } else {
                    continue;
                };
                inputs.push(CompiledSocketInput {
                    socket: s.name.into(),
                    resource,
                });
            }
        }
        let outputs: Vec<_> = contracts[i]
            .outputs
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.semantic_type.is_virtual())
            .map(|(o, s)| CompiledSocketOutput {
                socket: s.name.into(),
                resource: output_ids[&OutputKey(i, o as u16)],
            })
            .collect();
        let mut accesses = Vec::new();
        let kind = match contracts[i].key {
            _ if contracts[i].is_raster_draw() => {
                let color = output_ids[&OutputKey(i, 0)];
                let depth = output_ids[&OutputKey(i, 1)];
                let clear = match params[i] {
                    NormalizedParameters::Raster { clear_color, .. } => clear_color,
                    _ => unreachable!(),
                };
                let clear_depth = match &params[i] {
                    NormalizedParameters::Raster { clear_depth, .. } => *clear_depth,
                    _ => unreachable!(),
                };
                let first_color = version_of[&OutputKey(i, 0)].1 == 0;
                let first_depth = version_of[&OutputKey(i, 1)].1 == 0;
                let resolve_target = resolve_resources.get(&OutputKey(i, 0)).copied();
                let source_store = if resolve_target.is_some()
                    && !edges.iter().any(|edge| {
                        edge.from_node == i
                            && edge.producer_output_ordinal == 0
                            && matches!(
                                contracts[edge.to_node].inputs
                                    [edge.consumer_input_ordinal as usize]
                                    .role,
                                InputRole::ColorTarget { .. }
                            )
                    }) {
                    StoreOp::Discard
                } else {
                    StoreOp::Store
                };
                let stored = source_store == StoreOp::Store;
                if let ResourcePlan::Texture {
                    family,
                    version,
                    stored: resource_stored,
                    ..
                } = &mut resources[color as usize].plan
                {
                    *resource_stored = stored;
                    if let Some(texture_version) = families
                        .get_mut(*family as usize)
                        .and_then(|family| family.versions.get_mut(*version as usize))
                    {
                        texture_version.stored = stored;
                    }
                }
                let cl = if first_color {
                    NormalizedColorLoad::Clear { value: clear }
                } else {
                    NormalizedColorLoad::Load
                };
                let dl = if first_depth {
                    NormalizedDepthLoad::Clear { value: clear_depth }
                } else {
                    NormalizedDepthLoad::Load
                };
                for s in ["mesh"] {
                    accesses.push(CompiledAccess {
                        socket: s.into(),
                        resource: input_resource(s),
                        mode: AccessMode::SemanticRead,
                    });
                }
                accesses.push(CompiledAccess {
                    socket: "color".into(),
                    resource: color,
                    mode: AccessMode::ColorAttachment {
                        location: 0,
                        load: cl,
                        store: source_store,
                        full_overwrite: first_color,
                    },
                });
                accesses.push(CompiledAccess {
                    socket: "depth".into(),
                    resource: depth,
                    mode: AccessMode::DepthAttachment {
                        load: dl,
                        store: StoreOp::Store,
                        full_overwrite: first_depth,
                    },
                });
                if let Some(resource) = resolve_target {
                    accesses.push(CompiledAccess {
                        socket: "colorResolve".into(),
                        resource,
                        mode: AccessMode::ColorResolve {
                            source: color,
                            location: 0,
                        },
                    });
                }
                ExecutionKind::RasterDraw
            }
            _ if contracts[i].fullscreen_policy.is_some() => {
                let color = output_ids[&OutputKey(i, 0)];
                let load = NormalizedColorLoad::Clear {
                    value: [0.0, 0.0, 0.0, 0.0],
                };
                for input in contracts[i]
                    .inputs
                    .iter()
                    .filter(|input| input.role == InputRole::SampledTexture)
                {
                    accesses.push(CompiledAccess {
                        socket: input.name.into(),
                        resource: input_resource(input.name),
                        mode: AccessMode::SampledTexture,
                    });
                }
                accesses.push(CompiledAccess {
                    socket: "color".into(),
                    resource: color,
                    mode: AccessMode::ColorAttachment {
                        location: 0,
                        load,
                        store: StoreOp::Store,
                        full_overwrite: true,
                    },
                });
                ExecutionKind::Fullscreen
            }
            "frame_out" => {
                let r = input_resource("color");
                accesses.push(CompiledAccess {
                    socket: "color".into(),
                    resource: r,
                    mode: AccessMode::SampledTexture,
                });
                ExecutionKind::FrameOut { color: r }
            }
            _ => unreachable!(),
        };
        executions.push(CompiledExecution {
            id: graph.nodes[i].id.clone(),
            original_node_index: i as u32,
            executor: graph.nodes[i].executor.clone(),
            parameters: params[i].clone(),
            kind,
            inputs,
            outputs,
            accesses,
        });
    }
    // Lower virtual values into a stable, device-independent expression plan.
    let mut expression_plan = ExpressionPlan::default();
    let mut expression_ids = HashMap::<OutputKey, ExprId>::new();
    let mut expression_provenance = HashMap::<OutputKey, Option<u32>>::new();
    let mut cse = HashMap::<String, ExprId>::new();
    let mut requires_camera = false;
    let mut mesh_root = None;
    let mut intern = |semantic_type: SemanticType,
                      op: ExpressionOp,
                      origin: NodeOutputRef,
                      mesh_provenance: Option<u32>| {
        let key = format!("{semantic_type:?}:{op:?}:{mesh_provenance:?}");
        if let Some(id) = cse.get(&key) {
            return *id;
        }
        let id = ExprId(expression_plan.expressions.len() as u32);
        expression_plan.expressions.push(Expression {
            semantic_type,
            op,
            origin,
            mesh_provenance,
        });
        cse.insert(key, id);
        id
    };
    for &i in &order {
        if contracts[i].execution != ExecutionClass::Expression {
            continue;
        }
        let defaults: &[TypedLiteral] = match &params[i] {
            NormalizedParameters::ExpressionDefaults { defaults } => defaults.as_slice(),
            NormalizedParameters::FrustumCull { .. } => &[],
            _ => unreachable!(),
        };
        let mut operands = Vec::new();
        let mut operand_provenance = Vec::new();
        for (ordinal, input) in contracts[i].inputs.iter().enumerate() {
            let bindings: Vec<_> = graph.nodes[i]
                .inputs
                .get(input.name)
                .into_iter()
                .flatten()
                .map(|r| {
                    let producer = ids[r.node.as_str()];
                    let output = contracts[producer]
                        .outputs
                        .iter()
                        .position(|o| o.name == r.socket)
                        .unwrap();
                    BoundInput {
                        producer: OutputKey(producer, output as u16),
                    }
                })
                .collect();
            if bindings.is_empty() {
                if input.cardinality.max > 1 {
                    continue;
                }
                let literal = defaults[ordinal].clone();
                operands.push(intern(
                    literal.semantic_type(),
                    ExpressionOp::Literal { literal },
                    NodeOutputRef {
                        node: graph.nodes[i].id.clone(),
                        socket: input.name.into(),
                    },
                    None,
                ));
                operand_provenance.push(None);
            } else {
                for binding in bindings {
                    let key = binding.producer;
                    let producer_type = contracts[key.0].outputs[key.1 as usize].semantic_type;
                    let operand_mesh = if contracts[key.0].key == "mesh" {
                        Some(output_ids[&OutputKey(key.0, 0)])
                    } else {
                        expression_provenance[&key]
                    };
                    let id = if producer_type.is_virtual() {
                        if contracts[key.0].key == "mesh" {
                            let mesh = output_ids[&OutputKey(key.0, 0)];
                            if mesh_root.is_some_and(|root| root != mesh) {
                                return Err(error(
                                    "GRAPH_SOCKET_TYPE_MISMATCH",
                                    "instance traversal has multiple mesh roots",
                                    format!("nodes[{i}].inputs.{}", input.name),
                                ));
                            }
                            mesh_root = Some(mesh);
                            let op = match producer_type {
                                SemanticType::U32x16 => ExpressionOp::InstanceType { mesh },
                                SemanticType::LocalAabb => ExpressionOp::LocalAabb { mesh },
                                _ => unreachable!(),
                            };
                            intern(
                                producer_type,
                                op,
                                graph.nodes[key.0]
                                    .inputs
                                    .get("")
                                    .and_then(|refs| refs.first().cloned())
                                    .unwrap_or(NodeOutputRef {
                                        node: graph.nodes[key.0].id.clone(),
                                        socket: contracts[key.0].outputs[key.1 as usize]
                                            .name
                                            .into(),
                                    }),
                                Some(mesh),
                            )
                        } else {
                            expression_ids[&key]
                        }
                    } else {
                        continue;
                    };
                    operands.push(id);
                    operand_provenance.push(operand_mesh);
                }
            }
        }
        let mut provenances = operand_provenance.into_iter().flatten();
        let provenance = provenances.next();
        if provenances.any(|candidate| Some(candidate) != provenance) {
            return Err(error(
                "GRAPH_SOCKET_TYPE_MISMATCH",
                "expression mixes mesh provenance",
                format!("nodes[{i}].inputs"),
            ));
        }
        for (output_ordinal, output) in contracts[i].outputs.iter().enumerate() {
            let key = contracts[i].key;
            let op = match key {
                "not" => ExpressionOp::Not { value: operands[0] },
                "and" | "or" | "xor" | "xnor" => ExpressionOp::Boolean {
                    operation: match key {
                        "and" => BooleanOp::And,
                        "or" => BooleanOp::Or,
                        "xor" => BooleanOp::Xor,
                        _ => BooleanOp::Xnor,
                    },
                    operands: operands.clone(),
                },
                "greater_than_f32" | "less_than_f32" | "equals_f32" => ExpressionOp::CompareF32 {
                    operation: if key.starts_with("greater") {
                        CompareOp::GreaterThan
                    } else if key.starts_with("less") {
                        CompareOp::LessThan
                    } else {
                        CompareOp::Equals
                    },
                    left: operands[0],
                    right: operands[1],
                },
                "greater_than_u32" | "less_than_u32" | "equals_u32" => ExpressionOp::CompareU32 {
                    operation: if key.starts_with("greater") {
                        CompareOp::GreaterThan
                    } else if key.starts_with("less") {
                        CompareOp::LessThan
                    } else {
                        CompareOp::Equals
                    },
                    left: operands[0],
                    right: operands[1],
                },
                k if k.starts_with("separate_vec") => ExpressionOp::VectorProject {
                    vector: operands[0],
                    index: output_ordinal as u8,
                },
                k if k.starts_with("combine_vec") => ExpressionOp::VectorConstruct {
                    components: operands.clone(),
                },
                k if k.starts_with("separate_mat") => ExpressionOp::MatrixColumn {
                    matrix: operands[0],
                    index: output_ordinal as u8,
                },
                k if k.starts_with("combine_mat") => ExpressionOp::MatrixConstruct {
                    columns: operands.clone(),
                },
                "separate_u32x16" => ExpressionOp::TypeWord {
                    value: operands[0],
                    index: output_ordinal as u8,
                },
                "combine_u32x16" => ExpressionOp::TypeConstruct {
                    words: operands.clone(),
                },
                "separate_u32_bits" => ExpressionOp::U32Bit {
                    value: operands[0],
                    index: output_ordinal as u8,
                },
                "combine_u32_bits" => ExpressionOp::U32Construct {
                    bits: operands.clone(),
                },
                "separate_local_aabb" if output_ordinal == 0 => {
                    ExpressionOp::AabbMin { aabb: operands[0] }
                }
                "separate_local_aabb" => ExpressionOp::AabbMax { aabb: operands[0] },
                "frustum_cull" => {
                    requires_camera = true;
                    let mesh = output_ids[&bound[i]["mesh"].producer];
                    if mesh_root.is_some_and(|root| root != mesh) {
                        return Err(error(
                            "GRAPH_SOCKET_TYPE_MISMATCH",
                            "instance traversal has multiple mesh roots",
                            format!("nodes[{i}].inputs.mesh"),
                        ));
                    }
                    mesh_root = Some(mesh);
                    ExpressionOp::FrustumCulled {
                        mesh,
                        local_aabb: operands[0],
                    }
                }
                _ => unreachable!(),
            };
            let id = intern(
                output.semantic_type,
                op,
                NodeOutputRef {
                    node: graph.nodes[i].id.clone(),
                    socket: output.name.into(),
                },
                provenance,
            );
            expression_ids.insert(OutputKey(i, output_ordinal as u16), id);
            expression_provenance.insert(OutputKey(i, output_ordinal as u16), provenance);
        }
    }
    let mut predicates = Vec::new();
    for (execution, compiled) in executions.iter().enumerate() {
        let node = compiled.original_node_index as usize;
        if !contracts[node].is_raster_draw() {
            continue;
        }
        let mesh = output_ids[&bound[node]["mesh"].producer];
        if mesh_root.is_some_and(|root| root != mesh) {
            return Err(error(
                "GRAPH_SOCKET_TYPE_MISMATCH",
                "instance traversal has multiple mesh roots",
                format!("nodes[{node}].inputs.mesh"),
            ));
        }
        mesh_root = Some(mesh);
        let predicate = if let Some(binding) = bound[node].get("predicate") {
            expression_ids[&binding.producer]
        } else {
            let NormalizedParameters::Raster {
                predicate_default, ..
            } = params[node]
            else {
                unreachable!()
            };
            intern(
                SemanticType::Bool,
                ExpressionOp::Literal {
                    literal: TypedLiteral::Bool(predicate_default),
                },
                NodeOutputRef {
                    node: graph.nodes[node].id.clone(),
                    socket: "predicate".into(),
                },
                None,
            )
        };
        predicates.push(PipelinePredicatePlan {
            execution: execution as u32,
            predicate,
            ordinal: 0,
        });
    }
    for (ordinal, predicate) in predicates.iter_mut().enumerate() {
        predicate.ordinal = ordinal as u32;
    }
    if expression_plan.expressions.len() > MAX_EXPRESSIONS
        || predicates.len() > MAX_PREDICATE_PIPELINES
    {
        return Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "instance traversal plan exceeds limits",
            "nodes",
        ));
    }
    let instance_traversal = mesh_root.map(|mesh| InstanceTraversalPlan {
        mesh,
        expressions: expression_plan,
        pipelines: predicates,
        requires_camera,
    });
    for (ordinal, e) in executions.iter().enumerate() {
        for o in &e.outputs {
            resources[o.resource as usize].producer_execution = Some(ordinal as u32);
        }
        for access in &e.accesses {
            if matches!(access.mode, AccessMode::ColorResolve { .. }) {
                resources[access.resource as usize].producer_execution = Some(ordinal as u32);
            }
        }
    }
    let render_passes = build_render_passes(&executions, &resources, &families);
    let execution_pass: Vec<u32> = render_passes
        .iter()
        .enumerate()
        .flat_map(|(pass, value)| value.executions.iter().map(move |_| pass as u32))
        .collect();
    // Dense lifetimes use physical pass ordinals; producer metadata remains logical.
    for (ordinal, e) in executions.iter().enumerate() {
        let ordinal = execution_pass[ordinal];
        let mut touched = BTreeSet::new();
        for x in &e.inputs {
            touched.insert(x.resource);
        }
        for x in &e.outputs {
            touched.insert(x.resource);
        }
        for x in &e.accesses {
            touched.insert(x.resource);
        }
        for r in touched {
            let life = resources[r as usize].lifetime.get_or_insert(Lifetime {
                first_use: ordinal,
                last_use: ordinal,
            });
            life.first_use = life.first_use.min(ordinal);
            life.last_use = life.last_use.max(ordinal);
        }
    }
    for f in &mut families {
        let mut first = None;
        let mut last = 0;
        for v in &mut f.versions {
            v.lifetime = resources[v.resource as usize].lifetime.unwrap();
            first = Some(first.map_or(v.lifetime.first_use, |x: u32| x.min(v.lifetime.first_use)));
            last = last.max(v.lifetime.last_use);
        }
        f.lifetime = Lifetime {
            first_use: first.unwrap_or(0),
            last_use: last,
        };
        f.usage = texture_usage(f, &executions);
        let transient = match f.source {
            TextureFamilySource::AuthoredTexture { residency, .. } => {
                residency == TextureResidency::Transient
            }
            TextureFamilySource::CompilerDefaultInput { .. }
            | TextureFamilySource::CompilerColorResolve { .. } => true,
        };
        f.aliasable = transient && f.versions.iter().all(|v| v.initialized);
    }
    let (classes, transient) = allocate(&mut families, &mut resources);
    if resources.len() > 1024 {
        return Err(error(
            "GRAPH_LIMIT_EXCEEDED",
            "too many final resources",
            "resources",
        ));
    }
    Ok(CompiledGraph {
        schema_version: 3,
        graph_id: graph.graph_id,
        revision: graph.revision,
        node_count: graph.nodes.len() as u32,
        pipelines: graph.pipelines,
        resources,
        executions,
        render_passes,
        texture_families: families,
        allocation_classes: classes,
        culled_node_count: (graph.nodes.len() - live.len()) as u32,
        culled_resource_count: (all_outputs - authored_materialized_output_count) as u32,
        transient_slot_count: transient,
        instance_traversal,
    })
}

pub(crate) fn execution_attachments(
    execution: &CompiledExecution,
) -> (Vec<ColorAttachmentPlan>, Option<DepthStencilAttachmentPlan>) {
    let mut colors = Vec::new();
    let mut depth = None;
    for access in &execution.accesses {
        match access.mode {
            AccessMode::ColorAttachment {
                location,
                load,
                store,
                ..
            } => colors.push(ColorAttachmentPlan {
                resource: access.resource,
                resolve_target: execution.accesses.iter().find_map(|candidate| {
                    match candidate.mode {
                        AccessMode::ColorResolve {
                            source,
                            location: l,
                        } if source == access.resource && l == location => Some(candidate.resource),
                        _ => None,
                    }
                }),
                location,
                load,
                store,
            }),
            AccessMode::DepthAttachment { load, store, .. } => {
                depth = Some(DepthStencilAttachmentPlan {
                    resource: access.resource,
                    load,
                    store,
                })
            }
            _ => {}
        }
    }
    colors.sort_by_key(|color| color.location);
    (colors, depth)
}

pub(crate) fn build_render_passes(
    executions: &[CompiledExecution],
    resources: &[CompiledResource],
    families: &[TextureFamily],
) -> Vec<PhysicalRenderPass> {
    let family = |resource: u32| {
        resources
            .get(resource as usize)
            .and_then(|resource| match resource.plan {
                ResourcePlan::Texture { family, .. }
                | ResourcePlan::TextureSource { family, .. } => Some(family),
                _ => None,
            })
    };
    let mut passes: Vec<PhysicalRenderPass> = Vec::new();
    for (index, execution) in executions.iter().enumerate() {
        if matches!(execution.kind, ExecutionKind::FrameOut { .. }) {
            passes.push(PhysicalRenderPass {
                executions: vec![index as u32],
                kind: PhysicalRenderPassKind::Surface,
            });
            continue;
        }
        let (colors, depth) = execution_attachments(execution);
        let mut merged = false;
        if matches!(execution.kind, ExecutionKind::RasterDraw)
            && colors.iter().all(|v| v.load == NormalizedColorLoad::Load)
            && depth
                .as_ref()
                .is_none_or(|v| v.load == NormalizedDepthLoad::Load)
        {
            if let Some(PhysicalRenderPass {
                executions: members,
                kind:
                    PhysicalRenderPassKind::Texture {
                        color_attachments: previous_colors,
                        depth_stencil: previous_depth,
                    },
            }) = passes.last_mut()
            {
                let previous = &executions[*members.last().unwrap() as usize];
                let target_input = |socket: &str| {
                    execution
                        .inputs
                        .iter()
                        .find(|input| input.socket == socket)
                        .map(|input| input.resource)
                };
                let exact = previous.outputs.iter().any(|out| {
                    out.socket == "color" && Some(out.resource) == target_input("color")
                }) && depth.as_ref().is_none_or(|_| {
                    previous.outputs.iter().any(|out| {
                        out.socket == "depth" && Some(out.resource) == target_input("depth")
                    })
                });
                let compatible = previous_colors.len() == colors.len()
                    && previous_colors.iter().zip(&colors).all(|(a, b)| {
                        a.location == b.location
                            && family(a.resource) == family(b.resource)
                            && family(a.resource).is_some_and(|f| {
                                family(b.resource).is_some_and(|next_family| {
                                    families.get(f as usize).is_some_and(|first| {
                                        families.get(next_family as usize).is_some_and(|next| {
                                            family_descriptor(first) == family_descriptor(next)
                                        })
                                    })
                                })
                            })
                    })
                    && match (previous_depth.as_ref(), depth.as_ref()) {
                        (None, None) => true,
                        (Some(a), Some(b)) => {
                            family(a.resource) == family(b.resource)
                                && family(a.resource).is_some_and(|f| {
                                    family(b.resource).is_some_and(|next_family| {
                                        families.get(f as usize).is_some_and(|first| {
                                            families.get(next_family as usize).is_some_and(|next| {
                                                family_descriptor(first) == family_descriptor(next)
                                            })
                                        })
                                    })
                                })
                        }
                        _ => false,
                    };
                let no_resolve = previous_colors.iter().all(|v| v.resolve_target.is_none());
                let no_external = previous.outputs.iter().all(|out| {
                    executions
                        .iter()
                        .enumerate()
                        .filter(|(_, e)| e.inputs.iter().any(|v| v.resource == out.resource))
                        .all(|(consumer, _)| consumer == index)
                });
                if exact && compatible && no_resolve && no_external {
                    for (physical, final_value) in previous_colors.iter_mut().zip(&colors) {
                        physical.resource = final_value.resource;
                        physical.resolve_target = final_value.resolve_target;
                        physical.store = final_value.store;
                    }
                    if let (Some(physical), Some(final_value)) =
                        (previous_depth.as_mut(), depth.as_ref())
                    {
                        physical.resource = final_value.resource;
                        physical.store = final_value.store;
                    }
                    members.push(index as u32);
                    merged = true;
                }
            }
        }
        if !merged {
            passes.push(PhysicalRenderPass {
                executions: vec![index as u32],
                kind: PhysicalRenderPassKind::Texture {
                    color_attachments: colors,
                    depth_stencil: depth,
                },
            });
        }
    }
    passes
}

fn extent_layers(e: &NormalizedTextureExtent) -> u32 {
    match e {
        NormalizedTextureExtent::Absolute {
            depth_or_array_layers,
            ..
        }
        | NormalizedTextureExtent::SurfaceRelative {
            depth_or_array_layers,
            ..
        } => *depth_or_array_layers,
    }
}
pub(super) fn family_descriptor(family: &TextureFamily) -> &NormalizedTextureDescriptor {
    match &family.source {
        TextureFamilySource::AuthoredTexture { descriptor, .. }
        | TextureFamilySource::CompilerDefaultInput { descriptor, .. }
        | TextureFamilySource::CompilerColorResolve { descriptor, .. } => descriptor,
    }
}
pub(super) fn is_single_view_d2(descriptor: &NormalizedTextureDescriptor) -> bool {
    descriptor.dimension == TextureDimension::D2
        && descriptor.sample_count == 1
        && descriptor.mip_level_count == 1
        && extent_layers(&descriptor.extent) == 1
        && descriptor.view_formats.is_empty()
}
pub(super) fn frame_out_source_compatible(
    descriptor: &NormalizedTextureDescriptor,
    dynamic_range: &FrameDynamicRange,
) -> bool {
    is_single_view_d2(descriptor)
        && match dynamic_range {
            FrameDynamicRange::Hdr { .. } => descriptor.format == TextureFormat::Rgba16Float,
            FrameDynamicRange::Sdr => matches!(
                descriptor.format,
                TextureFormat::Rgba8Unorm | TextureFormat::Bgra8Unorm | TextureFormat::Rgba16Float
            ),
        }
}
pub(super) fn texture_usage(
    f: &TextureFamily,
    executions: &[CompiledExecution],
) -> Vec<TextureUsage> {
    let rs: HashSet<_> = f.versions.iter().map(|v| v.resource).collect();
    let mut u = BTreeSet::new();
    for e in executions {
        for a in &e.accesses {
            if !rs.contains(&a.resource) {
                continue;
            }
            match a.mode {
                AccessMode::SampledTexture => {
                    u.insert(TextureUsage::Sampled);
                }
                AccessMode::StorageRead | AccessMode::StorageWrite { .. } => {
                    u.insert(TextureUsage::Storage);
                }
                AccessMode::ColorAttachment { .. } => {
                    u.insert(TextureUsage::ColorAttachment);
                }
                AccessMode::DepthAttachment { .. } => {
                    u.insert(TextureUsage::DepthAttachment);
                }
                AccessMode::ColorResolve { .. } => {
                    u.insert(TextureUsage::ColorAttachment);
                }
                _ => {}
            }
        }
    }
    u.into_iter().collect()
}
fn allocate(
    families: &mut [TextureFamily],
    resources: &mut [CompiledResource],
) -> (Vec<AllocationClass>, u32) {
    let mut grouped: BTreeMap<TextureCompatibilityKey, Vec<usize>> = BTreeMap::new();
    for (i, f) in families.iter().enumerate() {
        let descriptor = family_descriptor(f);
        grouped
            .entry(TextureCompatibilityKey {
                dimension: descriptor.dimension,
                format: descriptor.format,
                extent: descriptor.extent.clone(),
                mip_level_count: descriptor.mip_level_count,
                sample_count: descriptor.sample_count,
                view_formats: descriptor.view_formats.clone(),
            })
            .or_default()
            .push(i);
    }
    let mut classes = Vec::new();
    let mut transient = 0;
    for (key, ids) in grouped {
        let class = classes.len() as u32;
        let mut slots: Vec<AllocationSlot> = Vec::new();
        let mut aliasable = Vec::new();
        let mut dedicated = Vec::new();
        let mut persistent_ids = Vec::new();
        for fi in ids {
            let persistent = matches!(
                families[fi].source,
                TextureFamilySource::AuthoredTexture {
                    residency: TextureResidency::Persistent,
                    ..
                }
            );
            let alias = families[fi].aliasable && !persistent;
            if persistent {
                persistent_ids.push(fi);
            } else if alias {
                aliasable.push(fi);
            } else {
                dedicated.push(fi);
            }
        }
        aliasable.sort_by_key(|&fi| {
            (
                families[fi].lifetime.first_use,
                families[fi].lifetime.last_use,
                families[fi].key.clone(),
            )
        });
        dedicated.sort_by_key(|&fi| families[fi].key.clone());
        persistent_ids.sort_by_key(|&fi| families[fi].key.clone());
        for fi in aliasable.into_iter().chain(dedicated).chain(persistent_ids) {
            let persistent = matches!(
                families[fi].source,
                TextureFamilySource::AuthoredTexture {
                    residency: TextureResidency::Persistent,
                    ..
                }
            );
            let alias = families[fi].aliasable && !persistent;
            let found = if alias {
                slots.iter().position(|s| {
                    s.kind == AllocationKind::AliasedTransient
                        && s.occupants.iter().all(|&old| {
                            families[old as usize].lifetime.last_use
                                < families[fi].lifetime.first_use
                        })
                })
            } else {
                None
            };
            let slot = found.unwrap_or_else(|| {
                let s = slots.len();
                if !persistent {
                    transient += 1;
                }
                slots.push(AllocationSlot {
                    kind: if persistent {
                        AllocationKind::Persistent
                    } else if alias {
                        AllocationKind::AliasedTransient
                    } else {
                        AllocationKind::DedicatedTransient
                    },
                    usage: Vec::new(),
                    occupants: Vec::new(),
                });
                s
            });
            slots[slot].occupants.push(fi as u32);
            slots[slot].usage.extend(families[fi].usage.iter().copied());
            slots[slot].usage.sort();
            slots[slot].usage.dedup();
            let a = AllocationRef {
                class,
                slot: slot as u32,
            };
            families[fi].allocation = Some(a);
            for v in &families[fi].versions {
                if let ResourcePlan::Texture { allocation, .. } =
                    &mut resources[v.resource as usize].plan
                {
                    *allocation = Some(a);
                }
            }
        }
        classes.push(AllocationClass { key, slots });
    }
    (classes, transient)
}
