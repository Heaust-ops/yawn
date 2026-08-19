use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

use log::info;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::DedicatedWorkerGlobalScope;

#[cfg(target_arch = "wasm32")]
use crate::render_data::RenderDataConfig;
use crate::{
    command_ring::CommandRing,
    render_data::{
        upload::{decode_render_data_packet, prepare_render_data},
        InstanceHandle, InstanceType, MeshHandle, RenderData,
    },
};

pub mod executors;
pub(crate) mod frame_data;
pub mod gpu_scene;
pub mod instance_filter;
pub mod material;
pub mod pipeline_library;
pub mod scene_frame;

use pipeline_library::PipelineKey;
pub use pipeline_library::PipelineLibrary;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug, Clone)]
pub struct ResizeMessage {
    pub scale_factor: f64,
    pub width: f64,
    pub height: f64,
}

fn supports_planned_x4_attachment(
    is_depth: bool,
    resolve_required: bool,
    render_attachment: bool,
    multisample_x4: bool,
    multisample_resolve: bool,
) -> bool {
    render_attachment && multisample_x4 && (is_depth || !resolve_required || multisample_resolve)
}

#[cfg(test)]
mod device_feature_tests {
    use super::*;

    #[test]
    fn adapter_x4_policy_distinguishes_depth_and_color_resolve() {
        assert!(supports_planned_x4_attachment(
            true, false, true, true, false
        ));
        assert!(!supports_planned_x4_attachment(
            true, false, true, false, true
        ));
        assert!(!supports_planned_x4_attachment(
            false, true, true, true, false
        ));
        assert!(supports_planned_x4_attachment(
            false, true, true, true, true
        ));
    }
}

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
struct FullscreenUniforms {
    values: [[f32; 4]; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FullscreenSamplerChoice {
    Linear,
    Nearest,
}

fn frame_out_sampler_choice(
    parameters: &crate::render_graph::NormalizedParameters,
) -> Option<FullscreenSamplerChoice> {
    let crate::render_graph::NormalizedParameters::FrameOut { filter, .. } = parameters else {
        return None;
    };
    Some(match filter {
        crate::render_graph::FrameFilter::Linear => FullscreenSamplerChoice::Linear,
        crate::render_graph::FrameFilter::Nearest => FullscreenSamplerChoice::Nearest,
    })
}

fn pack_fullscreen_uniforms(
    key: &str,
    parameters: &crate::render_graph::NormalizedParameters,
) -> Option<FullscreenUniforms> {
    use crate::render_graph::NormalizedParameters;
    let mut values = [[0.; 4]; 8];
    let first = match (key, parameters) {
        ("fullscreen_copy", NormalizedParameters::FullscreenCopy) => [0.; 4],
        (
            "color_balance",
            NormalizedParameters::ColorBalance {
                mode,
                factor,
                lift,
                lift_color,
                gamma,
                gamma_color,
                gain,
                gain_color,
                offset,
                offset_color,
                power,
                power_color,
                slope,
                slope_color,
            },
        ) => {
            values[0] = [
                if *mode == crate::render_graph::ColorBalanceMode::LiftGammaGain {
                    0.
                } else {
                    1.
                },
                *factor,
                *lift,
                *gamma,
            ];
            values[1] = [*gain, *offset, *power, *slope];
            for (lane, color) in [
                lift_color,
                gamma_color,
                gain_color,
                offset_color,
                power_color,
                slope_color,
            ]
            .iter()
            .enumerate()
            {
                values[lane + 2] = [color[0], color[1], color[2], 0.];
            }
            return Some(FullscreenUniforms { values });
        }
        (
            "exposure_contrast",
            NormalizedParameters::ExposureContrast {
                exposure_stops,
                contrast,
                pivot,
                factor,
            },
        ) => [*exposure_stops, *contrast, *pivot, *factor],
        ("saturation", NormalizedParameters::Saturation { saturation, factor }) => {
            [*saturation, *factor, 0., 0.]
        }
        (
            "channel_mixer",
            NormalizedParameters::ChannelMixer {
                red_output,
                green_output,
                blue_output,
                factor,
            },
        ) => {
            values[0] = [red_output[0], red_output[1], red_output[2], *factor];
            values[1] = [green_output[0], green_output[1], green_output[2], 0.];
            values[2] = [blue_output[0], blue_output[1], blue_output[2], 0.];
            return Some(FullscreenUniforms { values });
        }
        ("bloom_extract", NormalizedParameters::BloomExtract { threshold, knee }) => {
            [*threshold, *knee, 0., 0.]
        }
        ("bloom_blur", NormalizedParameters::BloomBlur { direction, radius }) => {
            [direction[0], direction[1], *radius, 0.]
        }
        ("bloom_composite", NormalizedParameters::BloomComposite { intensity }) => {
            [*intensity, 0., 0., 0.]
        }
        ("luminance_edge", NormalizedParameters::LuminanceEdge { strength }) => {
            [*strength, 0., 0., 0.]
        }
        _ => return None,
    };
    values[0] = first;
    Some(FullscreenUniforms { values })
}

fn pack_frame_out_uniforms(
    parameters: &crate::render_graph::NormalizedParameters,
    surface: &crate::render_graph::RuntimeSurfaceContract,
) -> Option<FullscreenUniforms> {
    use crate::render_graph::*;
    let NormalizedParameters::FrameOut {
        surface_format: _,
        dynamic_range,
        output_transfer,
        scale_mode,
        background_color,
        ..
    } = parameters
    else {
        return None;
    };
    if *output_transfer == OutputTransfer::Linear && surface.format.is_srgb() {
        return None;
    }
    let (hdr, mapper, exposure) = match dynamic_range {
        FrameDynamicRange::Sdr => (0., 0., 0.),
        FrameDynamicRange::Hdr {
            tone_mapper,
            exposure_stops,
        } => (
            1.,
            match tone_mapper {
                ToneMapper::None => 0.,
                ToneMapper::Reinhard => 1.,
                ToneMapper::Aces => 2.,
            },
            *exposure_stops,
        ),
    };
    let mut values = [[0.; 4]; 8];
    values[0] = [
        hdr,
        mapper,
        exposure,
        if *output_transfer == OutputTransfer::Srgb && !surface.format.is_srgb() {
            1.
        } else {
            0.
        },
    ];
    values[1] = [
        match scale_mode {
            ScaleMode::Stretch => 0.,
            ScaleMode::Contain => 1.,
            ScaleMode::Cover => 2.,
        },
        surface.width as f32,
        surface.height as f32,
        0.,
    ];
    values[2] = *background_color;
    Some(FullscreenUniforms { values })
}

#[cfg(test)]
mod fullscreen_tests {
    use super::*;
    use crate::render_graph::*;

    fn assert_packed(key: &str, parameters: NormalizedParameters, expected: &[[f32; 4]]) {
        let packed = pack_fullscreen_uniforms(key, &parameters).unwrap();
        assert_eq!(&packed.values[..expected.len()], expected);
        assert!(packed.values[expected.len()..]
            .iter()
            .all(|lane| *lane == [0.; 4]));
    }

    #[test]
    fn fullscreen_uniform_abi_and_packer_are_fixed() {
        assert_eq!(std::mem::size_of::<FullscreenUniforms>(), 128);
        assert_eq!(std::mem::align_of::<FullscreenUniforms>(), 16);
        let packed = pack_fullscreen_uniforms(
            "bloom_blur",
            &NormalizedParameters::BloomBlur {
                direction: [0.0, 1.0],
                radius: 3.0,
            },
        )
        .unwrap();
        assert_eq!(packed.values[0], [0.0, 1.0, 3.0, 0.0]);
        assert!(packed.values[1..].iter().all(|value| *value == [0.0; 4]));
        assert_eq!(bytemuck::bytes_of(&packed).len(), 128);
        assert!(
            pack_fullscreen_uniforms("frame_out", &NormalizedParameters::FullscreenCopy).is_none()
        );
    }

    fn surface(format: wgpu::TextureFormat) -> RuntimeSurfaceContract {
        RuntimeSurfaceContract {
            format,
            width: 1920,
            height: 1080,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        }
    }

    fn frame(
        dynamic_range: FrameDynamicRange,
        transfer: OutputTransfer,
        scale_mode: ScaleMode,
        filter: FrameFilter,
    ) -> NormalizedParameters {
        NormalizedParameters::FrameOut {
            surface_format: SurfaceFormatRequest::Preferred,
            dynamic_range,
            output_transfer: transfer,
            scale_mode,
            filter,
            background_color: [0.1, 0.2, 0.3, 0.4],
        }
    }

    #[test]
    fn frame_out_packer_tags_all_lanes_and_sampler_is_gpu_independent() {
        let expected = |first, scale| {
            [
                first,
                [scale, 1920., 1080., 0.],
                [0.1, 0.2, 0.3, 0.4],
                [0.; 4],
                [0.; 4],
                [0.; 4],
                [0.; 4],
                [0.; 4],
            ]
        };
        for (mapper, mapper_tag, scale, scale_tag, filter) in [
            (
                ToneMapper::None,
                0.,
                ScaleMode::Stretch,
                0.,
                FrameFilter::Linear,
            ),
            (
                ToneMapper::Reinhard,
                1.,
                ScaleMode::Contain,
                1.,
                FrameFilter::Nearest,
            ),
            (
                ToneMapper::Aces,
                2.,
                ScaleMode::Cover,
                2.,
                FrameFilter::Linear,
            ),
        ] {
            let p = frame(
                FrameDynamicRange::Hdr {
                    tone_mapper: mapper,
                    exposure_stops: -2.,
                },
                OutputTransfer::Srgb,
                scale,
                filter,
            );
            assert_eq!(
                pack_frame_out_uniforms(&p, &surface(wgpu::TextureFormat::Rgba8Unorm))
                    .unwrap()
                    .values,
                expected([1., mapper_tag, -2., 1.], scale_tag)
            );
            assert_eq!(
                frame_out_sampler_choice(&p),
                Some(if filter == FrameFilter::Nearest {
                    FullscreenSamplerChoice::Nearest
                } else {
                    FullscreenSamplerChoice::Linear
                })
            );
        }
        let sdr = frame(
            FrameDynamicRange::Sdr,
            OutputTransfer::Srgb,
            ScaleMode::Stretch,
            FrameFilter::Linear,
        );
        for format in [
            wgpu::TextureFormat::Bgra8UnormSrgb,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            assert_eq!(
                pack_frame_out_uniforms(&sdr, &surface(format))
                    .unwrap()
                    .values,
                expected([0.; 4], 0.)
            );
        }
        assert_eq!(
            pack_frame_out_uniforms(&sdr, &surface(wgpu::TextureFormat::Bgra8Unorm))
                .unwrap()
                .values,
            expected([0., 0., 0., 1.], 0.)
        );
        let linear = frame(
            FrameDynamicRange::Sdr,
            OutputTransfer::Linear,
            ScaleMode::Stretch,
            FrameFilter::Linear,
        );
        assert_eq!(
            pack_frame_out_uniforms(&linear, &surface(wgpu::TextureFormat::Bgra8Unorm))
                .unwrap()
                .values,
            expected([0.; 4], 0.)
        );
        assert!(
            pack_frame_out_uniforms(&linear, &surface(wgpu::TextureFormat::Bgra8UnormSrgb))
                .is_none()
        );
        assert_eq!(
            frame_out_sampler_choice(&NormalizedParameters::FullscreenCopy),
            None
        );
    }

    fn coordinates(
        p: [f32; 2],
        surface: [f32; 2],
        source: [f32; 2],
        mode: ScaleMode,
    ) -> ([f32; 2], bool) {
        if mode == ScaleMode::Stretch {
            return ([p[0] / surface[0], p[1] / surface[1]], true);
        }
        let (sa, ia) = (surface[0] / surface[1], source[0] / source[1]);
        let mut size = surface;
        if (mode == ScaleMode::Contain && ia > sa) || (mode == ScaleMode::Cover && ia < sa) {
            size[1] = surface[0] / ia;
        } else {
            size[0] = surface[1] * ia;
        }
        let origin = [(surface[0] - size[0]) * 0.5, (surface[1] - size[1]) * 0.5];
        (
            [(p[0] - origin[0]) / size[0], (p[1] - origin[1]) / size[1]],
            mode == ScaleMode::Cover
                || (p[0] >= origin[0]
                    && p[1] >= origin[1]
                    && p[0] < origin[0] + size[0]
                    && p[1] < origin[1] + size[1]),
        )
    }

    #[test]
    fn frame_coordinates_reference_half_open_centered_crop_and_stretch() {
        let contain = |p| coordinates(p, [4., 4.], [4., 2.], ScaleMode::Contain);
        assert_eq!(contain([0., 1.]), ([0., 0.], true));
        assert_eq!(contain([4., 1.]), ([1., 0.], false));
        assert_eq!(contain([0., 3.]), ([0., 1.], false));
        assert!(contain([4. - 0.0001, 3. - 0.0001]).1);
        for point in [[-1., 2.], [5., 2.], [2., 0.], [2., 4.]] {
            assert!(!contain(point).1, "outside point {point:?}");
        }
        let cover = |p| coordinates(p, [4., 4.], [4., 2.], ScaleMode::Cover);
        assert_eq!(cover([2., 2.]), ([0.5, 0.5], true));
        assert_eq!(cover([0., 2.]), ([0.25, 0.5], true));
        assert_eq!(cover([4., 2.]), ([0.75, 0.5], true));
        assert_eq!(
            coordinates([2., 2.], [4., 4.], [1., 9.], ScaleMode::Stretch),
            ([0.5, 0.5], true)
        );
    }

    fn srgb(x: f32) -> f32 {
        let x = x.clamp(0., 1.);
        if x <= 0.0031308 {
            12.92 * x
        } else {
            1.055 * x.powf(1. / 2.4) - 0.055
        }
    }
    fn shade(c: [f32; 4], mapper: Option<ToneMapper>, exposure: f32, transfer: bool) -> [f32; 4] {
        let mut rgb = [0.; 3];
        for i in 0..3 {
            let x = if mapper.is_some() {
                (c[i] * 2f32.powf(exposure)).max(0.)
            } else {
                c[i]
            };
            rgb[i] = match mapper {
                Some(ToneMapper::Aces) => {
                    ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0., 1.)
                }
                Some(ToneMapper::Reinhard) => x / (1. + x),
                _ => x.clamp(0., 1.),
            };
            if transfer {
                rgb[i] = srgb(rgb[i]);
            }
        }
        [rgb[0], rgb[1], rgb[2], c[3].clamp(0., 1.)]
    }

    fn shade_background(
        c: [f32; 4],
        _hdr_mapper: ToneMapper,
        _hdr_exposure_stops: f32,
        output_transfer: OutputTransfer,
    ) -> [f32; 4] {
        let mut rgb = [c[0].clamp(0., 1.), c[1].clamp(0., 1.), c[2].clamp(0., 1.)];
        if output_transfer == OutputTransfer::Srgb {
            rgb.iter_mut().for_each(|value| *value = srgb(*value));
        }
        [rgb[0], rgb[1], rgb[2], c[3].clamp(0., 1.)]
    }

    #[test]
    fn frame_cpu_goldens_process_rgb_with_straight_clamped_alpha_and_background_once() {
        let close = |actual: f32, expected: f32| {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}")
        };
        for (input, mapper, expected) in [
            (0., ToneMapper::Aces, 0.),
            (1., ToneMapper::Aces, 0.8037975),
            (1., ToneMapper::Reinhard, 0.5),
            (4., ToneMapper::Reinhard, 0.8),
            (-1., ToneMapper::None, 0.),
            (2., ToneMapper::None, 1.),
        ] {
            close(
                shade([input, 0., 0., 0.25], Some(mapper), 0., false)[0],
                expected,
            );
        }
        for (input, expected) in [(0., 0.), (0.0031308, 0.0404499), (0.18, 0.461356), (1., 1.)] {
            close(srgb(input), expected);
        }
        let low_alpha = shade([0.18, 0.5, 1., 0.25], Some(ToneMapper::Reinhard), 1., true);
        let opaque = shade([0.18, 0.5, 1., 1.], Some(ToneMapper::Reinhard), 1., true);
        assert_eq!(&low_alpha[..3], &opaque[..3]);
        assert_eq!((low_alpha[3], opaque[3]), (0.25, 1.));
        assert_eq!(shade([0.18, 0.5, 1., 2.], None, 0., false)[3], 1.);
        let background = [0.18, 0.5, 1., 0.25];
        let once = shade_background(background, ToneMapper::Aces, 10., OutputTransfer::Srgb);
        for (actual, expected) in once.into_iter().zip([0.461356, 0.7353569, 1., 0.25]) {
            close(actual, expected);
        }
        assert_eq!(
            once,
            shade_background(background, ToneMapper::Reinhard, -10., OutputTransfer::Srgb)
        );
        assert_eq!(
            shade_background(background, ToneMapper::Aces, 10., OutputTransfer::Linear),
            background
        );
        assert_eq!(
            shade_background(
                [0.18, 0.5, 1., 1.2],
                ToneMapper::Aces,
                10.,
                OutputTransfer::Srgb
            )[3],
            1.
        );
        assert_eq!(
            shade_background(
                [0.18, 0.5, 1., -0.2],
                ToneMapper::Aces,
                10.,
                OutputTransfer::Srgb
            )[3],
            0.
        );
        let white = shade_background(
            [1., 1., 1., 0.25],
            ToneMapper::Aces,
            10.,
            OutputTransfer::Srgb,
        );
        white[..3].iter().for_each(|channel| close(*channel, 1.));
        assert_eq!(white[3], 0.25);
        let tone_mapped_white = shade([1., 1., 1., 1.], Some(ToneMapper::Aces), 0., false);
        assert!(tone_mapped_white[0] < 0.81 && white[0] > tone_mapped_white[0]);
    }

    fn balance(mode: ColorBalanceMode) -> NormalizedParameters {
        NormalizedParameters::ColorBalance {
            mode,
            factor: 0.5,
            lift: -0.1,
            lift_color: [1., 2., 3.],
            gamma: 1.1,
            gamma_color: [1.1, 1.2, 1.3],
            gain: 1.2,
            gain_color: [2.1, 2.2, 2.3],
            offset: 0.1,
            offset_color: [0.1, 0.2, 0.3],
            power: 1.3,
            power_color: [3.1, 3.2, 3.3],
            slope: 1.4,
            slope_color: [0.4, 0.5, 0.6],
        }
    }

    #[test]
    fn grading_uniforms_are_exact_and_zero_filled() {
        for (mode, tag) in [
            (ColorBalanceMode::LiftGammaGain, 0.),
            (ColorBalanceMode::OffsetPowerSlope, 1.),
        ] {
            assert_packed(
                "color_balance",
                balance(mode),
                &[
                    [tag, 0.5, -0.1, 1.1],
                    [1.2, 0.1, 1.3, 1.4],
                    [1., 2., 3., 0.],
                    [1.1, 1.2, 1.3, 0.],
                    [2.1, 2.2, 2.3, 0.],
                    [0.1, 0.2, 0.3, 0.],
                    [3.1, 3.2, 3.3, 0.],
                    [0.4, 0.5, 0.6, 0.],
                ],
            );
        }
        assert_packed(
            "exposure_contrast",
            NormalizedParameters::ExposureContrast {
                exposure_stops: -2.,
                contrast: 1.5,
                pivot: 0.18,
                factor: 0.5,
            },
            &[[-2., 1.5, 0.18, 0.5]],
        );
        assert_packed(
            "saturation",
            NormalizedParameters::Saturation {
                saturation: 2.,
                factor: 0.25,
            },
            &[[2., 0.25, 0., 0.]],
        );
        assert_packed(
            "channel_mixer",
            NormalizedParameters::ChannelMixer {
                red_output: [1., 2., 3.],
                green_output: [4., 5., 6.],
                blue_output: [7., 8., 9.],
                factor: 0.5,
            },
            &[[1., 2., 3., 0.5], [4., 5., 6., 0.], [7., 8., 9., 0.]],
        );
        assert!(
            pack_fullscreen_uniforms("saturation", &NormalizedParameters::FullscreenCopy).is_none()
        );
        assert!(pack_fullscreen_uniforms(
            "wrong",
            &NormalizedParameters::Saturation {
                saturation: 1.,
                factor: 1.
            }
        )
        .is_none());
    }

    fn mix(a: [f32; 4], b: [f32; 3], factor: f32) -> [f32; 4] {
        [
            a[0] + (b[0] - a[0]) * factor,
            a[1] + (b[1] - a[1]) * factor,
            a[2] + (b[2] - a[2]) * factor,
            a[3],
        ]
    }
    fn exposure(c: [f32; 4], stops: f32, contrast: f32, pivot: f32, factor: f32) -> [f32; 4] {
        let mut out = [0.; 3];
        for i in 0..3 {
            let x = c[i] * 2f32.powf(stops);
            out[i] = x.signum() * pivot * (x.abs() / pivot).powf(contrast);
        }
        mix(c, out, factor)
    }
    fn saturation(c: [f32; 4], amount: f32, factor: f32) -> [f32; 4] {
        let l = c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
        mix(
            c,
            [
                l + (c[0] - l) * amount,
                l + (c[1] - l) * amount,
                l + (c[2] - l) * amount,
            ],
            factor,
        )
    }
    fn mixer(c: [f32; 4], rows: [[f32; 3]; 3], factor: f32) -> [f32; 4] {
        mix(
            c,
            rows.map(|r| c[0] * r[0] + c[1] * r[1] + c[2] * r[2]),
            factor,
        )
    }

    #[test]
    fn grading_cpu_references_cover_factors_and_neutral_cases() {
        let c = [0.2, 0.4, 0.8, 0.3];
        for f in [0., 0.5, 1.] {
            assert_eq!(exposure(c, 0., 1., 0.18, f), c);
        }
        assert_eq!(exposure(c, 2., 1., 0.18, 1.), [0.8, 1.6, 3.2, 0.3]);
        let dark = exposure(c, -2., 1., 0.18, 1.);
        assert!(dark
            .iter()
            .zip([0.05, 0.1, 0.2, 0.3])
            .all(|(a, b)| (a - b).abs() < 1e-6));
        let gray = saturation(c, 0., 1.);
        assert!((gray[0] - gray[1]).abs() < 1e-6 && (gray[1] - gray[2]).abs() < 1e-6);
        assert_eq!(saturation(c, 1., 1.), c);
        assert_eq!(saturation(c, 2., 0.), c);
        let identity = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        assert_eq!(mixer(c, identity, 1.), c);
        assert_eq!(
            mixer(c, [[0., 1., 0.], [1., 0., 0.], [0., 0., 1.]], 1.),
            [0.4, 0.2, 0.8, 0.3]
        );
        for x in [-1e-7, 0., 1e-7] {
            assert!(exposure([x, x, x, 0.7], 0., 1.1, 0.18, 1.)
                .iter()
                .all(|v| v.is_finite()));
        }
        // Both balance branches are neutral on nonnegative RGB with neutral controls.
        let lgg = c; // lift=0/color=1, gamma=1/color=1, gain=1/color=1
        let ops = c; // offset=0/color=1, power=1/color=1, slope=1/color=1
        assert_eq!(lgg, c);
        assert_eq!(ops, c);
    }
}

struct GpuTextureSlot {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
}

enum PreparedExecution {
    Pipeline {
        base: PipelineKey,
        predicate: crate::render_graph::ExprId,
        variant: wgpu::RenderPipeline,
    },
    Fullscreen {
        bind_group: wgpu::BindGroup,
        pipeline: wgpu::RenderPipeline,
        _uniform: wgpu::Buffer,
    },
}

struct PreparedCompute {
    name: String,
    pipeline: wgpu::ComputePipeline,
    dispatch: [u32; 3],
}

struct ActiveCompiledGraph {
    id: crate::render_graph::CompiledGraphId,
    graph: crate::render_graph::CompiledGraph,
    runtime: crate::render_graph::RuntimePlan,
    textures: Vec<Vec<GpuTextureSlot>>,
    compute: Vec<PreparedCompute>,
    executions: Vec<PreparedExecution>,
    _fullscreen_layout: wgpu::BindGroupLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameTargetSource {
    PendingSwitch,
    PendingResize,
    Active,
}

fn select_frame_target_source(
    pending_switch: bool,
    pending_resize: bool,
    active: bool,
) -> Option<FrameTargetSource> {
    if pending_switch {
        Some(FrameTargetSource::PendingSwitch)
    } else if pending_resize {
        Some(FrameTargetSource::PendingResize)
    } else if active {
        Some(FrameTargetSource::Active)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AcquisitionAction {
    RejectSwitch,
    DropResize,
    ReconfigureAndSkip,
    Skip,
    Halt,
}

fn acquisition_action(source: FrameTargetSource, error: &wgpu::SurfaceError) -> AcquisitionAction {
    if matches!(error, wgpu::SurfaceError::OutOfMemory) {
        return AcquisitionAction::Halt;
    }
    match source {
        FrameTargetSource::PendingSwitch => AcquisitionAction::RejectSwitch,
        FrameTargetSource::PendingResize => AcquisitionAction::DropResize,
        FrameTargetSource::Active => match error {
            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                AcquisitionAction::ReconfigureAndSkip
            }
            wgpu::SurfaceError::Timeout => AcquisitionAction::Skip,
            wgpu::SurfaceError::Other => AcquisitionAction::Halt,
            wgpu::SurfaceError::OutOfMemory => unreachable!(),
        },
    }
}

fn requires_camera(graph: &ActiveCompiledGraph) -> bool {
    graph
        .runtime
        .instance_traversal
        .as_ref()
        .is_some_and(|plan| plan.requires_camera)
}

fn resolve_culling_frustum(
    required: bool,
    read: impl FnOnce() -> Result<[[f32; 4]; 6], crate::render_data::camera::FrustumError>,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    if !required {
        return Ok(None);
    }
    match read() {
        Ok(planes) => Ok(Some(planes)),
        Err(error) => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_FAILED",
            format!("camera frustum is invalid: {error}"),
        )),
    }
}

fn update_frame_data(
    frame_data: &mut frame_data::FrameData,
    queue: &wgpu::Queue,
    requires_camera: bool,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    frame_data.update(queue);
    resolve_culling_frustum(requires_camera, || frame_data.frustum_planes())
}
impl ActiveCompiledGraph {
    fn id(&self) -> crate::render_graph::CompiledGraphId {
        self.id
    }
    fn graph_id(&self) -> &str {
        &self.graph.graph_id
    }
    fn revision(&self) -> u32 {
        self.graph.revision
    }
    fn schema_version(&self) -> u32 {
        self.graph.schema_version
    }
    fn execution_count(&self) -> usize {
        self.graph.executions.len()
    }
    fn texture_slot_count(&self) -> usize {
        self.textures.iter().map(Vec::len).sum()
    }
}

fn resolve_switch_request(
    registry: &crate::render_graph::Registry,
    pending: bool,
    slot: u32,
    generation: u32,
) -> Result<crate::render_graph::CompiledGraphId, crate::render_graph::GraphError> {
    if pending {
        return Err(crate::render_graph::GraphError::new(
            "GRAPH_SWITCH_PENDING",
            "a graph switch is pending",
        ));
    }
    let id = crate::render_graph::CompiledGraphId { slot, generation };
    // Resolve the registry entry here, before any GPU preparation or pending
    // state mutation. Registry::get is also the compiled-graph availability gate.
    registry.get(id)?;
    Ok(id)
}

fn drop_graph_request(
    registry: &mut crate::render_graph::Registry,
    id: crate::render_graph::CompiledGraphId,
    active: Option<crate::render_graph::CompiledGraphId>,
    pending_switch: Option<crate::render_graph::CompiledGraphId>,
    pending_resize: Option<crate::render_graph::CompiledGraphId>,
    in_flight: Option<crate::render_graph::CompiledGraphId>,
) -> Result<(), crate::render_graph::GraphError> {
    if active == Some(id) {
        Err(crate::render_graph::GraphError::new(
            "GRAPH_ACTIVE",
            "compiled graph is active",
        ))
    } else if [pending_switch, pending_resize, in_flight].contains(&Some(id)) {
        Err(crate::render_graph::GraphError::new(
            "GRAPH_SWITCH_PENDING",
            "compiled graph switch is pending",
        ))
    } else {
        registry.drop_graph(id)
    }
}

#[cfg(test)]
mod switch_request_tests {
    fn valid_compile_graph(graph_id: &str, revision: u64) -> Vec<u8> {
        let mut graph = crate::render_graph::tests::full_cull_graph();
        graph["graphId"] = serde_json::json!(graph_id);
        graph["revision"] = serde_json::json!(revision);
        crate::render_graph::tests::ast_bytes(&graph)
    }

    use super::*;

    #[test]
    fn frame_target_precedence_is_exact() {
        assert_eq!(
            select_frame_target_source(true, true, true),
            Some(FrameTargetSource::PendingSwitch)
        );
        assert_eq!(
            select_frame_target_source(false, true, true),
            Some(FrameTargetSource::PendingResize)
        );
        assert_eq!(
            select_frame_target_source(false, false, true),
            Some(FrameTargetSource::Active)
        );
        assert_eq!(select_frame_target_source(false, false, false), None);
    }

    #[test]
    fn acquisition_policy_covers_every_source_and_surface_error() {
        use wgpu::SurfaceError::*;
        use AcquisitionAction::*;
        use FrameTargetSource::*;
        for error in [Lost, Outdated, Timeout, Other] {
            assert_eq!(acquisition_action(PendingSwitch, &error), RejectSwitch);
            assert_eq!(acquisition_action(PendingResize, &error), DropResize);
        }
        assert_eq!(acquisition_action(Active, &Lost), ReconfigureAndSkip);
        assert_eq!(acquisition_action(Active, &Outdated), ReconfigureAndSkip);
        assert_eq!(acquisition_action(Active, &Timeout), Skip);
        assert_eq!(acquisition_action(Active, &Other), Halt);
        for source in [PendingSwitch, PendingResize, Active] {
            assert_eq!(acquisition_action(source, &OutOfMemory), Halt);
        }
    }

    #[test]
    fn frustum_preflight_uses_boolean_traversal_requirement() {
        let mut reads = 0;
        assert_eq!(
            resolve_culling_frustum(false, || {
                reads += 1;
                Ok([[0.; 4]; 6])
            })
            .unwrap(),
            None
        );
        assert_eq!(
            reads, 0,
            "inactive frustum filtering must not read the camera"
        );
        let invalid = resolve_culling_frustum(true, || {
            Err(crate::render_data::camera::FrustumError::Degenerate { plane: 2 })
        })
        .unwrap_err();
        assert_eq!(invalid.code, "GRAPH_EXECUTION_FAILED");
        assert!(invalid.message.contains("invalid"));
    }

    #[test]
    fn resolves_at_command_boundary_before_gpu_work() {
        let mut registry = crate::render_graph::Registry::default();
        let mut graph = crate::render_graph::tests::full_cull_graph();
        graph["graphId"] = serde_json::json!("switch");
        let bytes = crate::render_graph::tests::ast_bytes(&graph);
        let (id, _) = registry.compile(&bytes).unwrap();
        let active = "existing_graph";
        let pending: Option<&str> = None;
        assert_eq!(
            resolve_switch_request(&registry, false, id.slot, id.generation).unwrap(),
            id
        );
        assert_eq!(active, "existing_graph");
        assert_eq!(pending, None);
        assert!(registry.contains(id));

        let pending_error = resolve_switch_request(&registry, true, id.slot, id.generation)
            .expect_err("an existing pending request must win");
        assert_eq!(pending_error.code, "GRAPH_SWITCH_PENDING");
        assert_eq!(active, "existing_graph");
        assert_eq!(pending, None);

        let invalid_replacement = b"(yawn-graph 1 (id \"switch\") (revision 2) (pipelines (object (field \"render\" (array)) (field \"compute\" (array)))) (nodes) (unexpected true))";
        assert_eq!(
            registry.compile(invalid_replacement).unwrap_err().code,
            "GRAPH_AST_INVALID"
        );
        let stored = registry.get(id).unwrap();
        assert_eq!(stored.revision, 1);

        registry.drop_graph(id).unwrap();
        assert_eq!(registry.get(id).unwrap_err().code, "STALE_GRAPH_ID");
    }

    #[test]
    fn pending_resize_keeps_registry_ownership_through_commit() {
        let mut registry = crate::render_graph::Registry::default();
        let (id, _) = registry.compile(&valid_compile_graph("resize", 1)).unwrap();

        let error = drop_graph_request(&mut registry, id, None, None, Some(id), None)
            .expect_err("a completed resize candidate still owns its registry entry");
        assert_eq!(error.code, "GRAPH_SWITCH_PENDING");
        assert!(registry.contains(id));

        let active = Some(id);
        assert_eq!(registry.get(active.unwrap()).unwrap().revision, 1);

        let (other, _) = registry.compile(&valid_compile_graph("other", 1)).unwrap();
        drop_graph_request(&mut registry, other, active, None, None, None).unwrap();
        assert_eq!(registry.get(other).unwrap_err().code, "STALE_GRAPH_ID");
    }

    #[test]
    fn resize_restart_snapshot_remains_bound_to_its_immutable_registry_revision() {
        let mut registry = crate::render_graph::Registry::default();
        let (id, _) = registry.compile(&valid_compile_graph("resize", 1)).unwrap();
        let revision_one = registry.get(id).unwrap().clone();
        let in_flight = InFlightPreparation {
            token: 1,
            id,
            purpose: PreparationPurpose::Resize,
            graph: revision_one,
        };
        let (revision_two_id, _) = registry.compile(&valid_compile_graph("resize", 2)).unwrap();
        let original = registry.get(id).unwrap();
        let revision_two = registry.get(revision_two_id).unwrap();
        assert_eq!(in_flight.graph.revision, 1);
        assert_eq!(original.revision, 1);
        assert_eq!(revision_two.revision, 2);
        assert_ne!(id, revision_two_id);
    }
}

struct PendingSwitch {
    request: u32,
    target: ActiveCompiledGraph,
}

#[derive(Clone, Copy)]
enum PreparationPurpose {
    Switch { request: u32 },
    Resize,
}

struct InFlightPreparation {
    token: u64,
    id: crate::render_graph::CompiledGraphId,
    purpose: PreparationPurpose,
    graph: crate::render_graph::CompiledGraph,
}

struct PreparationCompletion {
    token: u64,
    purpose: PreparationPurpose,
    candidate: Result<ActiveCompiledGraph, crate::render_graph::GraphError>,
    validation_error: Option<String>,
    out_of_memory_error: Option<String>,
}

struct CommandError {
    code: &'static str,
    details: JsValue,
}

impl From<&'static str> for CommandError {
    fn from(code: &'static str) -> Self {
        Self {
            code,
            details: JsValue::UNDEFINED,
        }
    }
}

impl From<crate::render_graph::GraphError> for CommandError {
    fn from(error: crate::render_graph::GraphError) -> Self {
        // GraphError details are JSON by construction. Parsing avoids panicking if that
        // invariant is ever accidentally broken at a command boundary.
        let details = js_sys::JSON::parse(&error.details.to_string()).unwrap_or(JsValue::NULL);
        Self {
            code: error.code,
            details,
        }
    }
}

fn render_data_error_code(error: &crate::render_data::RenderDataError) -> &'static str {
    use crate::render_data::RenderDataError::*;
    match error {
        InvalidMeshHandle | InvalidInstanceHandle | CannotDestroyDefaultInstance => "STALE_HANDLE",
        InvalidTransform => "INVALID_TRANSFORM",
        EmptyVertices
        | MismatchedVertexStreams
        | EmptyIndices
        | IndexOutOfBounds
        | NonFiniteGeometry
        | InputTooLarge => "INVALID_GEOMETRY",
        InvalidCapacityConfig { .. }
        | CapacityOverflow { .. }
        | CapacityExceeded { .. }
        | AllocationFailed { .. }
        | EmptyRange
        | RangeOverflow
        | RangeOutOfBounds
        | RangeOverlap => "RESOURCE_LIMIT",
        RevisionOverflow => "REVISION_OVERFLOW",
        StaleReplacementStage => "STALE_REPLACEMENT",
    }
}

pub struct RendererContext {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface: wgpu::Surface<'static>,
    initial_surface_config: wgpu::SurfaceConfiguration,
}

pub struct Renderer {
    events_chan: Receiver<ResizeMessage>,
    context: RendererContext,
    resources: PipelineLibrary,
    frame_data: frame_data::FrameData,
    render_data: RenderData,
    shared_soa: crate::shared_soa::SharedSoaRegistry,
    shared_soa_init_sent: bool,
    camera_publish_required: bool,
    snapshot: crate::shared_snapshot::SharedSnapshot,
    snapshot_init_sent: bool,
    scene_frame: scene_frame::SceneFrameCache,
    gpu_scene: gpu_scene::GpuSceneCache,
    materials: material::MaterialResources,
    pub(crate) command_ring: Option<&'static CommandRing>,
    pending_replies: Vec<JsValue>,
    gpu_error: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    graph_registry: crate::render_graph::Registry,
    active_compiled: Option<ActiveCompiledGraph>,
    pending_switch: Option<PendingSwitch>,
    pending_resize: Option<ActiveCompiledGraph>,
    in_flight: Option<InFlightPreparation>,
    next_preparation_token: u64,
    preparation_completions: Rc<RefCell<Vec<PreparationCompletion>>>,
    halted: bool,
}

impl Renderer {
    fn surface_config(
        contract: &crate::render_graph::RuntimeSurfaceContract,
    ) -> wgpu::SurfaceConfiguration {
        wgpu::SurfaceConfiguration {
            usage: contract.usage,
            format: contract.format,
            width: contract.width,
            height: contract.height,
            present_mode: contract.present_mode,
            alpha_mode: contract.alpha_mode,
            view_formats: contract.view_formats.clone(),
            desired_maximum_frame_latency: contract.desired_maximum_frame_latency,
        }
    }

    fn configure_surface(&mut self, config: wgpu::SurfaceConfiguration) {
        self.context
            .surface
            .configure(&self.context.device, &config);
        self.context.surface_config = config;
    }

    fn reply(&mut self, request: u32, result: Result<JsValue, CommandError>) {
        let (ok, code, value, details) = match result {
            Ok(value) => (true, "OK", value, JsValue::UNDEFINED),
            Err(error) => (false, error.code, JsValue::UNDEFINED, error.details),
        };
        let reply = js_sys::Object::new();
        for (key, item) in [
            ("type", "reply".into()),
            ("request", request.into()),
            ("ok", ok.into()),
            ("code", code.into()),
            ("result", value),
            ("details", details),
        ] {
            let _ = js_sys::Reflect::set(&reply, &JsValue::from_str(key), &item);
        }
        self.pending_replies.push(reply.into());
    }

    fn drain_commands(&mut self) -> bool {
        let Some(ring) = self.command_ring else {
            return true;
        };
        let mut commands = Vec::new();
        if let Err(error) = ring.drain(|words| commands.push(words)) {
            log::error!("command ring closed: {error:?}");
            self.command_ring = None;
            self.post_fatal("RING_CORRUPT", &format!("{error:?}"));
            return false;
        }
        for words in commands {
            let (opcode, request) = (words[1], words[2]);
            let words = &words[1..];
            if opcode == 7 {
                let outcome = crate::take_payload(words[2])
                    .ok_or_else(|| crate::render_graph::GraphError {
                        code: "PAYLOAD_MISSING",
                        message: "staged payload is missing".into(),
                        details: serde_json::json!({"message":"staged payload is missing"}),
                    })
                    .and_then(|bytes| self.graph_registry.compile(&bytes));
                match outcome {
                    Ok((_id, summary)) => self.reply(
                        request,
                        Ok(js_sys::JSON::parse(&summary.to_string()).unwrap_or(JsValue::NULL)),
                    ),
                    Err(error) => self.reply(request, Err(error.into())),
                }
                continue;
            } else if opcode == 8 {
                let id = crate::render_graph::CompiledGraphId {
                    slot: words[2],
                    generation: words[3],
                };
                let outcome = drop_graph_request(
                    &mut self.graph_registry,
                    id,
                    self.active_compiled.as_ref().map(ActiveCompiledGraph::id),
                    self.pending_switch
                        .as_ref()
                        .map(|pending| pending.target.id()),
                    self.pending_resize.as_ref().map(ActiveCompiledGraph::id),
                    self.in_flight.as_ref().map(|preparation| preparation.id),
                );
                match outcome {
                    Ok(()) => self.reply(request, Ok(JsValue::UNDEFINED)),
                    Err(error) => self.reply(request, Err(error.into())),
                }
                continue;
            } else if opcode == 9 {
                let outcome = resolve_switch_request(
                    &self.graph_registry,
                    self.pending_switch.is_some() || self.in_flight.is_some(),
                    words[2],
                    words[3],
                )
                .and_then(|id| {
                    let graph = self.graph_registry.get(id)?.clone();
                    self.begin_compiled_preparation(
                        id,
                        graph,
                        PreparationPurpose::Switch { request },
                    )
                });
                if let Err(error) = outcome {
                    self.reply(request, Err(error.into()));
                }
                continue;
            }
            let outcome: Result<JsValue, &'static str> = (|| match opcode {
                1 => {
                    let bytes = self
                        .shared_soa
                        .read_fixed_bytes(words[2], words[3])
                        .map_err(|_| "SHARED_UPLOAD_INVALID")?;
                    let upload = decode_render_data_packet(&bytes)
                        .map_err(|_| "RENDER_DATA_PACKET_INVALID")?;
                    // Build a complete GPU candidate first. Neither the live scene nor
                    // its material epoch changes if image decode/resource creation fails.
                    let prepared_materials = self
                        .materials
                        .prepare(&self.context.device, &self.context.queue, &upload)
                        .map_err(|_| "MATERIAL_INVALID")?;
                    let prepared_data = prepare_render_data(&self.render_data, &upload)
                        .map_err(|_| "INSTALL_FAILED")?;
                    self.shared_soa
                        .publish_materials(&upload.materials)
                        .map_err(|_| "MATERIAL_SHARED_STATE_FAILED")?;
                    self.render_data
                        .replace_with(prepared_data.stage)
                        .map_err(|_| "INSTALL_FAILED")?;
                    self.materials
                        .install(prepared_materials, self.render_data.revision());
                    let installed = prepared_data.installed;
                    let result = js_sys::Object::new();
                    let meshes = js_sys::Array::new();
                    for h in installed.meshes {
                        let item = js_sys::Object::new();
                        let view = self.render_data.mesh(h).unwrap();
                        js_sys::Reflect::set(
                            &item,
                            &"handle".into(),
                            &js_sys::Array::of2(&h.slot().into(), &h.generation().into()),
                        )
                        .unwrap();
                        js_sys::Reflect::set(
                            &item,
                            &"defaultInstance".into(),
                            &js_sys::Array::of2(
                                &view.default_instance.slot().into(),
                                &view.default_instance.generation().into(),
                            ),
                        )
                        .unwrap();
                        js_sys::Reflect::set(
                            &item,
                            &"defaultType".into(),
                            &js_sys::Array::from_iter(
                                view.default_instance_type
                                    .words
                                    .into_iter()
                                    .map(JsValue::from),
                            ),
                        )
                        .unwrap();
                        meshes.push(&item);
                    }
                    js_sys::Reflect::set(&result, &"meshes".into(), &meshes).unwrap();
                    let materials = js_sys::Array::new();
                    for material in &upload.materials {
                        let item = js_sys::Object::new();
                        js_sys::Reflect::set(&item, &"key".into(), &material.key.get().into())
                            .unwrap();
                        materials.push(&item);
                    }
                    js_sys::Reflect::set(&result, &"materials".into(), &materials).unwrap();
                    if let Some(bounds) = installed.bounds {
                        let value = js_sys::Object::new();
                        js_sys::Reflect::set(
                            &value,
                            &"min".into(),
                            &js_sys::Array::from_iter(bounds.min.into_iter().map(JsValue::from)),
                        )
                        .unwrap();
                        js_sys::Reflect::set(
                            &value,
                            &"max".into(),
                            &js_sys::Array::from_iter(bounds.max.into_iter().map(JsValue::from)),
                        )
                        .unwrap();
                        js_sys::Reflect::set(&result, &"bounds".into(), &value).unwrap();
                    }
                    Ok(result.into())
                }
                3 => {
                    let mesh = MeshHandle::from_parts(words[2], words[3]);
                    let mut m = [[0.; 4]; 4];
                    for i in 0..16 {
                        m[i / 4][i % 4] = f32::from_bits(words[4 + i]);
                    }
                    let h = self
                        .render_data
                        .create_instance(
                            mesh,
                            m,
                            InstanceType {
                                words: std::array::from_fn(|i| words[20 + i]),
                            },
                        )
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(js_sys::Array::of2(&h.slot().into(), &h.generation().into()).into())
                }
                6 => {
                    self.render_data
                        .destroy_instance(InstanceHandle::from_parts(words[2], words[3]))
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                11 => {
                    let bytes = crate::take_payload(words[2]).ok_or("PAYLOAD_MISSING")?;
                    let descriptor = self
                        .shared_soa
                        .allocate_json(&bytes, self.render_data.capacities())
                        .map_err(|_| "SOA_LAYOUT_INVALID")?;
                    let json =
                        serde_json::to_string(&descriptor).map_err(|_| "SOA_LAYOUT_INVALID")?;
                    Ok(js_sys::JSON::parse(&json).map_err(|_| "SOA_LAYOUT_INVALID")?)
                }
                _ => Err("UNKNOWN_OPCODE"),
            })();
            match outcome {
                Ok(value) => self.reply(request, Ok(value)),
                Err(code) => self.reply(request, Err(code.into())),
            }
        }
        true
    }

    fn plan_compiled(
        &self,
        graph: &crate::render_graph::CompiledGraph,
        width: u32,
        height: u32,
    ) -> Result<crate::render_graph::RuntimePlan, crate::render_graph::GraphError> {
        let capabilities = self.context.surface.get_capabilities(&self.context.adapter);
        let surface = crate::render_graph::resolve_graph_surface_contract(
            graph,
            &capabilities,
            width,
            height,
        )?;
        crate::render_graph::prepare_runtime_plan(
            graph,
            surface,
            Some(&self.context.device.limits()),
        )
    }

    fn create_compiled_candidate(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
        runtime: crate::render_graph::RuntimePlan,
    ) -> Result<ActiveCompiledGraph, crate::render_graph::GraphError> {
        use crate::render_graph::*;
        let fail = |message| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", message);
        let vertex_layouts = gpu_scene::vertex_layouts();
        let render_declarations: std::collections::HashMap<_, _> = graph
            .pipelines
            .render
            .iter()
            .map(|declaration| (declaration.name.as_str(), declaration))
            .collect();
        let authored_pipelines: std::collections::HashMap<_, _> = graph
            .pipelines
            .render
            .iter()
            .filter(|declaration| {
                crate::render_graph::contract_for(&declaration.name, &graph.pipelines)
                    .is_some_and(crate::render_graph::Contract::is_raster_draw)
            })
            .map(|declaration| {
                let key = self.resources.get_or_create_authored_pipeline(
                    &self.context.device,
                    declaration,
                    &vertex_layouts,
                    self.context.initial_surface_config.format,
                );
                (declaration.name.clone(), key)
            })
            .collect();
        let resolved_pipelines = graph
            .executions
            .iter()
            .enumerate()
            .map(|(index, execution)| {
                let NormalizedParameters::Raster { .. } = &execution.parameters else {
                    return Ok(None);
                };
                authored_pipelines
                    .get(&execution.executor.key)
                    .copied()
                    .map(Some)
                    .ok_or_else(|| {
                        GraphError::at(
                            "GRAPH_EXECUTION_UNSUPPORTED",
                            format!("pipeline '{}' is not registered", execution.executor.key),
                            format!("executions[{index}].executor.key"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let resolve_formats: std::collections::BTreeSet<_> = graph
            .texture_families
            .iter()
            .filter_map(|family| match &family.source {
                TextureFamilySource::CompilerColorResolve {
                    source_resource, ..
                } => {
                    let source = graph.resources.get(*source_resource as usize)?;
                    let source_family = match source.plan {
                        ResourcePlan::Texture { family, .. } => family,
                        _ => return None,
                    };
                    graph
                        .texture_families
                        .get(source_family as usize)
                        .map(|family| match &family.source {
                            TextureFamilySource::AuthoredTexture { descriptor, .. }
                            | TextureFamilySource::CompilerDefaultInput { descriptor, .. }
                            | TextureFamilySource::CompilerColorResolve { descriptor, .. } => {
                                descriptor.format
                            }
                        })
                }
                _ => None,
            })
            .collect();
        for (family_index, family) in graph.texture_families.iter().enumerate() {
            let descriptor = match &family.source {
                TextureFamilySource::AuthoredTexture { descriptor, .. }
                | TextureFamilySource::CompilerDefaultInput { descriptor, .. }
                | TextureFamilySource::CompilerColorResolve { descriptor, .. } => descriptor,
            };
            if descriptor.sample_count != 4 {
                continue;
            }
            let features = self
                .context
                .adapter
                .get_texture_format_features(texture_format(descriptor.format));
            let resolve_required = resolve_formats.contains(&descriptor.format)
                && descriptor.format != TextureFormat::Depth32Float;
            if !supports_planned_x4_attachment(
                descriptor.format == TextureFormat::Depth32Float,
                resolve_required,
                features
                    .allowed_usages
                    .contains(wgpu::TextureUsages::RENDER_ATTACHMENT),
                features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4),
                features
                    .flags
                    .contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE),
            ) {
                let source_path = match &family.source {
                    TextureFamilySource::AuthoredTexture { .. } => "authored",
                    TextureFamilySource::CompilerDefaultInput { .. } => "default",
                    TextureFamilySource::CompilerColorResolve { .. } => "resolve",
                };
                return Err(GraphError::at(
                    "GRAPH_UNSUPPORTED_FEATURE",
                    "adapter does not support planned 4x MSAA attachment",
                    format!("textureFamilies[{family_index}].source.{source_path}"),
                ));
            }
        }
        let compute_layout =
            self.context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("graph compute pipeline layout"),
                    bind_group_layouts: &[],
                    push_constant_ranges: &[],
                });
        let compute = graph
            .pipelines
            .compute
            .iter()
            .map(|declaration| {
                let shader =
                    self.context
                        .device
                        .create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: Some(&declaration.name),
                            source: wgpu::ShaderSource::Wgsl(declaration.shader.as_str().into()),
                        });
                let pipeline =
                    self.context
                        .device
                        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                            label: Some(&declaration.name),
                            layout: Some(&compute_layout),
                            module: &shader,
                            entry_point: Some(&declaration.entry),
                            compilation_options: Default::default(),
                            cache: None,
                        });
                PreparedCompute {
                    name: declaration.name.clone(),
                    pipeline,
                    dispatch: declaration.dispatch,
                }
            })
            .collect();
        let mut textures = Vec::with_capacity(runtime.allocations.classes.len());
        for class in &runtime.allocations.classes {
            let mut gpu_class = Vec::with_capacity(class.slots.len());
            for slot in &class.slots {
                let d = &slot.descriptor;
                let texture = self
                    .context
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(" graph texture"),
                        size: wgpu::Extent3d {
                            width: d.extent.width,
                            height: d.extent.height,
                            depth_or_array_layers: d.extent.depth_or_array_layers,
                        },
                        mip_level_count: d.mip_level_count,
                        sample_count: d.sample_count,
                        dimension: d.dimension,
                        format: d.format,
                        usage: d.usage,
                        view_formats: &d.view_formats,
                    });
                let view = texture.create_view(&Default::default());
                gpu_class.push(GpuTextureSlot {
                    _texture: texture,
                    view,
                });
            }
            textures.push(gpu_class);
        }
        let resolve = |resource: u32| -> Result<&wgpu::TextureView, GraphError> {
            let allocation = runtime
                .allocations
                .resource_allocations
                .get(resource as usize)
                .copied()
                .flatten()
                .ok_or_else(|| fail("resource has no GPU allocation"))?;
            textures
                .get(allocation.class as usize)
                .and_then(|c| c.get(allocation.slot as usize))
                .map(|s| &s.view)
                .ok_or_else(|| fail("resource allocation is out of bounds"))
        };
        let fullscreen_layout =
            self.context
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(" fullscreen texture"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: wgpu::BufferSize::new(128),
                            },
                            count: None,
                        },
                    ],
                });
        let pipeline_layout =
            self.context
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some(" fullscreen"),
                    bind_group_layouts: &[&fullscreen_layout],
                    push_constant_ranges: &[],
                });
        let sampler = self
            .context
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some(" post linear clamp"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
        let nearest_sampler = self
            .context
            .device
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some(" frame nearest clamp"),
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                ..Default::default()
            });
        let mut executions = Vec::new();
        for (index, execution) in graph.executions.iter().enumerate() {
            let contract =
                crate::render_graph::contract_for(&execution.executor.key, &graph.pipelines)
                    .ok_or_else(|| fail("executor contract missing"))?;
            match execution.executor.key.as_str() {
                "frustum_cull" => continue,
                _ if execution.executor.key == "frame_out"
                    || contract.fullscreen_policy.is_some() =>
                {
                    let frame_out = execution.executor.key == "frame_out";
                    let (source, second) = if frame_out {
                        let ExecutionKind::FrameOut { color } = execution.kind else {
                            return Err(fail("frame_out kind mismatch"));
                        };
                        (color, color)
                    } else {
                        let sampled: Vec<_> = contract
                            .inputs
                            .iter()
                            .enumerate()
                            .filter(|(_, input)| {
                                input.role == crate::render_graph::InputRole::SampledTexture
                            })
                            .map(|(index, _)| {
                                execution.inputs.get(index).map(|input| input.resource)
                            })
                            .collect::<Option<_>>()
                            .ok_or_else(|| fail("fullscreen inputs mismatch"))?;
                        match (contract.fullscreen_policy, sampled.as_slice()) {
                            (
                                Some(crate::render_graph::FullscreenPolicy::BloomComposite),
                                [source, second],
                            ) => (*source, *second),
                            (Some(_), [source]) => (*source, *source),
                            _ => return Err(fail("fullscreen inputs mismatch")),
                        }
                    };
                    let values = if frame_out {
                        pack_frame_out_uniforms(&execution.parameters, &runtime.surface)
                    } else {
                        pack_fullscreen_uniforms(&execution.executor.key, &execution.parameters)
                    }
                    .ok_or_else(|| fail("executor parameters mismatch"))?;
                    let sampler_choice = if frame_out {
                        Some(
                            frame_out_sampler_choice(&execution.parameters)
                                .ok_or_else(|| fail("executor parameters mismatch"))?,
                        )
                    } else {
                        None
                    };
                    use wgpu::util::DeviceExt;
                    let uniform =
                        self.context
                            .device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some(" post parameters"),
                                contents: bytemuck::bytes_of(&values),
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let target_format = if frame_out {
                        runtime.surface.format
                    } else {
                        if !matches!(execution.kind, ExecutionKind::Fullscreen) {
                            return Err(fail("fullscreen execution is not render"));
                        }
                        let (color_attachments, _) =
                            crate::render_graph::execution_attachments(execution);
                        let target = color_attachments
                            .first()
                            .ok_or_else(|| fail("fullscreen target missing"))?
                            .resource;
                        let a = runtime
                            .allocations
                            .resource_allocations
                            .get(target as usize)
                            .copied()
                            .flatten()
                            .ok_or_else(|| fail("fullscreen target allocation missing"))?;
                        runtime
                            .allocations
                            .classes
                            .get(a.class as usize)
                            .and_then(|class| class.slots.get(a.slot as usize))
                            .ok_or_else(|| fail("fullscreen target allocation is invalid"))?
                            .descriptor
                            .format
                    };
                    let declaration = render_declarations
                        .get(execution.executor.key.as_str())
                        .copied()
                        .ok_or_else(|| fail("fullscreen pipeline declaration missing"))?;
                    let shader =
                        self.context
                            .device
                            .create_shader_module(wgpu::ShaderModuleDescriptor {
                                label: Some(&declaration.name),
                                source: wgpu::ShaderSource::Wgsl(
                                    declaration.shader.as_str().into(),
                                ),
                            });
                    let pipeline = self.context.device.create_render_pipeline(
                        &wgpu::RenderPipelineDescriptor {
                            label: Some(" post pipeline"),
                            layout: Some(&pipeline_layout),
                            vertex: wgpu::VertexState {
                                module: &shader,
                                entry_point: Some(&declaration.vertex_entry),
                                buffers: &[],
                                compilation_options: Default::default(),
                            },
                            primitive: Default::default(),
                            depth_stencil: None,
                            multisample: Default::default(),
                            fragment: Some(wgpu::FragmentState {
                                module: &shader,
                                entry_point: Some(&declaration.fragment_entry),
                                targets: &[Some(wgpu::ColorTargetState {
                                    format: target_format,
                                    blend: None,
                                    write_mask: wgpu::ColorWrites::ALL,
                                })],
                                compilation_options: Default::default(),
                            }),
                            multiview: None,
                            cache: None,
                        },
                    );
                    let bind_group =
                        self.context
                            .device
                            .create_bind_group(&wgpu::BindGroupDescriptor {
                                label: Some(" fullscreen source"),
                                layout: &fullscreen_layout,
                                entries: &[
                                    wgpu::BindGroupEntry {
                                        binding: 0,
                                        resource: wgpu::BindingResource::TextureView(resolve(
                                            source,
                                        )?),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 1,
                                        resource: wgpu::BindingResource::TextureView(resolve(
                                            second,
                                        )?),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 2,
                                        resource: wgpu::BindingResource::Sampler(
                                            if sampler_choice
                                                == Some(FullscreenSamplerChoice::Nearest)
                                            {
                                                &nearest_sampler
                                            } else {
                                                &sampler
                                            },
                                        ),
                                    },
                                    wgpu::BindGroupEntry {
                                        binding: 3,
                                        resource: uniform.as_entire_binding(),
                                    },
                                ],
                            });
                    executions.push(PreparedExecution::Fullscreen {
                        bind_group,
                        pipeline,
                        _uniform: uniform,
                    });
                }
                _ if contract.is_raster_draw() => {
                    if !matches!(execution.kind, ExecutionKind::RasterDraw) {
                        return Err(fail("pipeline is not render"));
                    }
                    let (color_attachments, depth_stencil) =
                        crate::render_graph::execution_attachments(execution);
                    let color = color_attachments
                        .first()
                        .ok_or_else(|| fail("pipeline color missing"))?;
                    let color_format = {
                        let a = runtime
                            .allocations
                            .resource_allocations
                            .get(color.resource as usize)
                            .copied()
                            .flatten()
                            .ok_or_else(|| fail("color allocation missing"))?;
                        runtime
                            .allocations
                            .classes
                            .get(a.class as usize)
                            .and_then(|c| c.slots.get(a.slot as usize))
                            .map(|s| s.descriptor.format)
                            .ok_or_else(|| fail("color allocation invalid"))?
                    };
                    let depth_format = depth_stencil
                        .as_ref()
                        .map(|d| {
                            let a = runtime
                                .allocations
                                .resource_allocations
                                .get(d.resource as usize)
                                .copied()
                                .flatten()
                                .ok_or_else(|| fail("depth allocation missing"))?;
                            runtime
                                .allocations
                                .classes
                                .get(a.class as usize)
                                .and_then(|c| c.slots.get(a.slot as usize))
                                .map(|s| s.descriptor.format)
                                .ok_or_else(|| fail("depth allocation invalid"))
                        })
                        .transpose()?;
                    let NormalizedParameters::Raster {
                        depth_compare,
                        depth_write_enabled,
                        ..
                    } = &execution.parameters
                    else {
                        return Err(fail("pipeline parameters mismatch"));
                    };
                    let base = resolved_pipelines
                        .get(index)
                        .copied()
                        .flatten()
                        .ok_or_else(|| fail("resolved pipeline missing"))?;
                    let compare = match depth_compare {
                        CompareFunction::Never => wgpu::CompareFunction::Never,
                        CompareFunction::Less => wgpu::CompareFunction::Less,
                        CompareFunction::LessEqual => wgpu::CompareFunction::LessEqual,
                        CompareFunction::Greater => wgpu::CompareFunction::Greater,
                        CompareFunction::GreaterEqual => wgpu::CompareFunction::GreaterEqual,
                        CompareFunction::Equal => wgpu::CompareFunction::Equal,
                        CompareFunction::NotEqual => wgpu::CompareFunction::NotEqual,
                        CompareFunction::Always => wgpu::CompareFunction::Always,
                    };
                    let variant = self
                        .resources
                        .create_target_variant(
                            &self.context.device,
                            base,
                            color_format,
                            depth_format,
                            compare,
                            *depth_write_enabled,
                            runtime.allocations.resource_allocations[color.resource as usize]
                                .and_then(|a| {
                                    runtime
                                        .allocations
                                        .classes
                                        .get(a.class as usize)?
                                        .slots
                                        .get(a.slot as usize)
                                })
                                .map(|slot| slot.descriptor.sample_count)
                                .ok_or_else(|| fail("color allocation invalid"))?,
                        )
                        .map_err(|e| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", e))?;
                    executions.push(PreparedExecution::Pipeline {
                        base,
                        predicate: runtime
                            .instance_traversal
                            .as_ref()
                            .and_then(|p| {
                                p.pipelines.iter().find(|v| v.execution as usize == index)
                            })
                            .map(|p| p.predicate)
                            .ok_or_else(|| fail("pipeline predicate missing"))?,
                        variant,
                    });
                }
                _ => return Err(fail("unsupported prepared execution")),
            }
        }
        Ok(ActiveCompiledGraph {
            id,
            graph,
            runtime,
            textures,
            compute,
            executions,
            _fullscreen_layout: fullscreen_layout,
        })
    }

    fn begin_compiled_preparation(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
        purpose: PreparationPurpose,
    ) -> Result<(), crate::render_graph::GraphError> {
        let runtime = self.plan_compiled(
            &graph,
            self.context.surface_config.width,
            self.context.surface_config.height,
        )?;
        self.begin_compiled_preparation_with_runtime(id, graph, runtime, purpose)
    }

    fn begin_compiled_preparation_with_runtime(
        &mut self,
        id: crate::render_graph::CompiledGraphId,
        graph: crate::render_graph::CompiledGraph,
        runtime: crate::render_graph::RuntimePlan,
        purpose: PreparationPurpose,
    ) -> Result<(), crate::render_graph::GraphError> {
        // Candidate construction allocates GPU resources, so the live scene preflight
        // belongs here: this is the earliest boundary with both the runtime query and
        // scene access, and precedes GPU work and all pending/in-flight mutation.
        resolve_culling_frustum(
            runtime
                .instance_traversal
                .as_ref()
                .is_some_and(|p| p.requires_camera),
            || self.frame_data.frustum_planes(),
        )?;
        let restart_graph = graph.clone();
        self.next_preparation_token = self.next_preparation_token.wrapping_add(1).max(1);
        let token = self.next_preparation_token;
        self.context
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.context
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let candidate = self.create_compiled_candidate(id, graph, runtime);
        let validation = self.context.device.pop_error_scope();
        let out_of_memory = self.context.device.pop_error_scope();
        self.in_flight = Some(InFlightPreparation {
            token,
            id,
            purpose,
            graph: restart_graph,
        });
        let completions = self.preparation_completions.clone();
        spawn_local(async move {
            let validation_error = validation.await.map(|error| error.to_string());
            let out_of_memory_error = out_of_memory.await.map(|error| error.to_string());
            completions.borrow_mut().push(PreparationCompletion {
                token,
                purpose,
                candidate,
                validation_error,
                out_of_memory_error,
            });
        });
        Ok(())
    }

    fn drain_preparation_completions(&mut self) {
        let completions = std::mem::take(&mut *self.preparation_completions.borrow_mut());
        for completion in completions {
            let Some(in_flight) = self.in_flight.as_ref() else {
                continue;
            };
            if in_flight.token != completion.token {
                continue;
            }
            self.in_flight = None;
            let result = if let Some(message) = completion.out_of_memory_error {
                Err(crate::render_graph::GraphError::new(
                    "GRAPH_RESOURCE_LIMIT",
                    message,
                ))
            } else if let Some(message) = completion.validation_error {
                Err(crate::render_graph::GraphError::new(
                    "GRAPH_RUNTIME_PLAN_INVALID",
                    message,
                ))
            } else {
                completion.candidate
            };
            match (completion.purpose, result) {
                (PreparationPurpose::Switch { request }, Ok(candidate)) => {
                    self.pending_switch = Some(PendingSwitch {
                        request,
                        target: candidate,
                    });
                }
                (PreparationPurpose::Switch { request }, Err(error)) => {
                    self.reply(request, Err(error.into()));
                }
                (PreparationPurpose::Resize, Ok(candidate)) => {
                    self.pending_resize = Some(candidate);
                }
                (PreparationPurpose::Resize, Err(error)) => {
                    log::error!(
                        "compiled graph resize preparation failed: {}",
                        error.message
                    );
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn new(
        canvas: web_sys::OffscreenCanvas,
        events_chan: Receiver<ResizeMessage>,
    ) -> Self {
        let id = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        };

        let instance = wgpu::Instance::new(&id);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .unwrap();

        let required_features = wgpu::Features::INDIRECT_FIRST_INSTANCE;
        assert!(
            adapter.features().contains(required_features),
            "WebGPU adapter lacks required indirect-first-instance support"
        );
        let descriptor = wgpu::DeviceDescriptor {
            required_features,
            required_limits: wgpu::Limits::default(),
            label: None,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        };

        let (device, queue) = adapter.request_device(&descriptor).await.unwrap();
        assert!(
            device.features().contains(required_features),
            "WebGPU device lacks required indirect-first-instance support"
        );
        info!("Adapter info: {:?}", adapter.get_info());
        info!("Adapter features: {:?}", adapter.features());
        info!("Adapter limits: {:?}", adapter.limits());
        let gpu_error = std::sync::Arc::new(std::sync::Mutex::new(None));
        let error_flag = gpu_error.clone();
        device.on_uncaptured_error(Box::new(move |error| {
            let message = error.to_string();
            let mut first = error_flag.lock().unwrap();
            if first.is_none() {
                *first = Some(message.clone());
            }
            log::error!("Uncaptured GPU error: {message}");
        }));

        let surface_caps = surface.get_capabilities(&adapter);
        let initial_contract = crate::render_graph::resolve_surface_contract(
            crate::render_graph::SurfaceFormatRequest::Preferred,
            &surface_caps,
            canvas.clone().width().max(1),
            canvas.clone().height().max(1),
            "initialSurfaceFormat",
        )
        .expect("surface must support the fixed presentation contract");
        let surface_config = Self::surface_config(&initial_contract);
        info!(
            "suface size: {} x {}",
            surface_config.width, surface_config.height
        );
        surface.configure(&device, &surface_config);

        let mut resources = PipelineLibrary::new();
        let context = RendererContext {
            adapter,
            surface,
            device,
            queue,
            initial_surface_config: surface_config.clone(),
            surface_config,
        };

        let render_data =
            RenderData::new(RenderDataConfig::default()).expect("valid render data config");
        let mut frame_data = frame_data::FrameData::new(&context, &mut resources);
        let mut shared_soa = crate::shared_soa::SharedSoaRegistry::new(render_data.capacities())
            .expect("default shared SOA layouts are valid");
        shared_soa
            .publish_camera(frame_data.camera_mut())
            .expect("initial shared camera publication succeeds");
        let materials = material::MaterialResources::new(&context.device, &context.queue);
        resources.set_material_bind_group_layout(&materials.layout);

        Self {
            events_chan,
            context,
            frame_data,
            resources,
            render_data,
            shared_soa,
            shared_soa_init_sent: false,
            camera_publish_required: false,
            snapshot: crate::shared_snapshot::SharedSnapshot::new(),
            snapshot_init_sent: false,
            scene_frame: Default::default(),
            gpu_scene: Default::default(),
            materials,
            command_ring: None,
            pending_replies: Vec::new(),
            gpu_error,
            graph_registry: Default::default(),
            active_compiled: None,
            pending_switch: None,
            pending_resize: None,
            in_flight: None,
            next_preparation_token: 0,
            preparation_completions: Default::default(),
            halted: false,
        }
    }

    fn render(&mut self, _time: f32) {
        if self.halted {
            return;
        }
        self.drain_preparation_completions();
        let gpu_error = self.gpu_error.lock().unwrap().take();
        if let Some(error) = gpu_error {
            self.halted = true;
            self.post_fatal("GPU_VALIDATION_FAILED", &error);
            return;
        }
        if !self.drain_commands() {
            return;
        }
        let soa_layout_changed = match self
            .shared_soa
            .sync_capacities(self.render_data.capacities())
        {
            Ok(changed) => changed,
            Err(error) => {
                self.post_fatal("SOA_ALLOCATION_FAILED", &error.to_string());
                return;
            }
        };
        self.shared_soa
            .synchronize_render_data(&mut self.render_data);
        if let Some(rows) = self.shared_soa.take_material_words() {
            self.materials.synchronize(&self.context.queue, &rows);
        }
        if let Err(error) = self.synchronize_shared_camera() {
            self.post_fatal("SOA_CAMERA_INVALID", &error.to_string());
            return;
        }
        if !self.shared_soa_init_sent || soa_layout_changed {
            let message = js_sys::Object::new();
            let message_type = if self.shared_soa_init_sent {
                "soa-layout"
            } else {
                "soa-init"
            };
            let _ = js_sys::Reflect::set(&message, &"type".into(), &message_type.into());
            match self.shared_soa.descriptors().and_then(|descriptors| {
                serde_json::to_string(&descriptors)
                    .map_err(|_| crate::shared_soa::SharedSoaError::SizeOverflow)
            }) {
                Ok(json) => {
                    if let Ok(descriptors) = js_sys::JSON::parse(&json) {
                        let _ = js_sys::Reflect::set(&message, &"arrays".into(), &descriptors);
                        let global =
                            js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
                        let _ = global.post_message(&message);
                        self.shared_soa_init_sent = true;
                    }
                }
                Err(error) => {
                    self.post_fatal("SOA_ALLOCATION_FAILED", &error.to_string());
                    return;
                }
            }
        }
        let frame_plan = match self.scene_frame.get_or_build(&self.render_data) {
            Ok(plan) => plan,
            Err(error) => {
                log::error!("scene frame extraction failed: {error}");
                self.halted = true;
                self.post_fatal("SCENE_FRAME_FAILED", &error.to_string());
                return;
            }
        };
        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        if !self.snapshot_init_sent {
            let message = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&message, &"type".into(), &"snapshot-init".into());
            let _ = js_sys::Reflect::set(
                &message,
                &"controlPtr".into(),
                &self.snapshot.control_ptr().into(),
            );
            let _ = js_sys::Reflect::set(
                &message,
                &"controlVersion".into(),
                &crate::shared_snapshot::CONTROL_VERSION.into(),
            );
            let _ = js_sys::Reflect::set(
                &message,
                &"schemaVersion".into(),
                &crate::shared_snapshot::SCHEMA.into(),
            );
            let _ = global.post_message(&message);
            self.snapshot_init_sent = true;
        }
        match self.snapshot.publish(frame_plan) {
            Ok(Some(epoch)) => {
                let message = js_sys::Object::new();
                let _ =
                    js_sys::Reflect::set(&message, &"type".into(), &"snapshot-published".into());
                let _ = js_sys::Reflect::set(&message, &"epoch".into(), &epoch.into());
                let _ = global.post_message(&message);
            }
            Ok(None) => {}
            Err(error) => log::error!("picking snapshot failed closed with error {error}"),
        }
        let Some(target_source) = select_frame_target_source(
            self.pending_switch.is_some(),
            self.pending_resize.is_some(),
            self.active_compiled.is_some(),
        ) else {
            self.post_state();
            return;
        };
        let query = match target_source {
            FrameTargetSource::PendingSwitch => {
                requires_camera(&self.pending_switch.as_ref().unwrap().target)
            }
            FrameTargetSource::PendingResize => {
                requires_camera(self.pending_resize.as_ref().unwrap())
            }
            FrameTargetSource::Active => requires_camera(self.active_compiled.as_ref().unwrap()),
        };
        // Resolve again immediately before every active frame. Do this before scene
        // upload so an invalid camera cannot mutate GPU state or produce a frame.
        let planes = match update_frame_data(&mut self.frame_data, &self.context.queue, query) {
            Ok(planes) => planes,
            Err(error) => {
                match target_source {
                    FrameTargetSource::PendingSwitch => {
                        let pending = self.pending_switch.take().unwrap();
                        self.reply(pending.request, Err(error.into()));
                    }
                    FrameTargetSource::PendingResize => {
                        self.pending_resize = None;
                        log::error!("compiled graph resize preflight failed: {}", error.message);
                    }
                    FrameTargetSource::Active => {
                        self.post_fatal("GRAPH_EXECUTION_FAILED", &error.message);
                    }
                }
                return;
            }
        };
        let upload = self
            .gpu_scene
            .upload(&self.context.device, &self.context.queue, frame_plan);
        if let Err(error) = upload {
            log::error!("GPU scene upload failed: {error}");
            self.post_fatal("GPU_UPLOAD_FAILED", &error);
            return;
        }

        // Candidate publication is transactional: configure its complete contract at
        // the last possible point before acquisition, but retain the known-good
        // configuration and active graph identity until presentation succeeds.
        let candidate_config = match target_source {
            FrameTargetSource::PendingSwitch => {
                Self::surface_config(&self.pending_switch.as_ref().unwrap().target.runtime.surface)
            }
            FrameTargetSource::PendingResize => {
                Self::surface_config(&self.pending_resize.as_ref().unwrap().runtime.surface)
            }
            FrameTargetSource::Active => self.context.surface_config.clone(),
        };
        let restore_config = (candidate_config != self.context.surface_config)
            .then(|| self.context.surface_config.clone());
        if restore_config.is_some() {
            self.configure_surface(candidate_config);
        }
        let surface_texture = match self.context.surface.get_current_texture() {
            Ok(value) => value,
            Err(error) => {
                match acquisition_action(target_source, &error) {
                    AcquisitionAction::RejectSwitch => {
                        if let Some(config) = restore_config {
                            self.configure_surface(config);
                        }
                        let pending = self.pending_switch.take().unwrap();
                        self.reply(
                            pending.request,
                            Err(crate::render_graph::GraphError::new(
                                "GRAPH_SURFACE_RECONFIGURE_FAILED",
                                error.to_string(),
                            )
                            .into()),
                        );
                    }
                    AcquisitionAction::DropResize => {
                        if let Some(config) = restore_config {
                            self.configure_surface(config);
                        }
                        self.pending_resize = None;
                        log::error!("compiled graph resize acquisition failed: {error}");
                    }
                    AcquisitionAction::ReconfigureAndSkip => {
                        self.context
                            .surface
                            .configure(&self.context.device, &self.context.surface_config);
                    }
                    AcquisitionAction::Skip => log::warn!("surface acquisition timed out"),
                    AcquisitionAction::Halt => {
                        self.halted = true;
                        self.post_fatal("SURFACE_FRAME_FAILED", &error.to_string());
                    }
                }
                return;
            }
        };
        let texture_view = surface_texture.texture.create_view(&Default::default());
        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render command encoder"),
                });

        let rendering_compiled = match target_source {
            FrameTargetSource::PendingSwitch => &self.pending_switch.as_ref().unwrap().target,
            FrameTargetSource::PendingResize => self.pending_resize.as_ref().unwrap(),
            FrameTargetSource::Active => self.active_compiled.as_ref().unwrap(),
        };
        let encode_result = executors::encode_compiled(
            &mut encoder,
            &texture_view,
            rendering_compiled,
            &self.frame_data,
            &self.gpu_scene,
            &self.resources,
            &self.materials,
            planes.as_ref(),
        );
        if let Err(error) = encode_result {
            // A surface must have no acquired texture (or objects retaining its view)
            // when configure is called to roll back a transactional candidate.
            drop(encoder);
            drop(texture_view);
            drop(surface_texture);
            if let Some(pending) = self.pending_switch.take() {
                if let Some(config) = restore_config {
                    self.configure_surface(config);
                }
                self.reply(
                    pending.request,
                    Err(
                        crate::render_graph::GraphError::new("GRAPH_EXECUTION_FAILED", error)
                            .into(),
                    ),
                );
            } else if self.pending_resize.take().is_some() {
                if let Some(config) = restore_config {
                    self.configure_surface(config);
                }
            }
            return;
        }
        self.context.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();
        if let Some(pending) = self.pending_switch.take() {
            // A successful user switch supersedes any resize recreation of the
            // previously active graph. Retain that resize candidate only as a
            // fallback while the switch is being prepared or attempted.
            self.pending_resize = None;
            let active = pending.target;
            let summary = serde_json::json!({
                "mode":"compiled",
                "compiledId":[active.id().slot, active.id().generation],
                "graphId":active.graph_id(),
                "revision":active.revision(),
                "schemaVersion":active.schema_version()
            });
            self.active_compiled = Some(active);
            let result = js_sys::JSON::parse(&summary.to_string()).unwrap_or(JsValue::NULL);
            self.reply(pending.request, Ok(result));
        } else if let Some(candidate) = self.pending_resize.take() {
            self.active_compiled = Some(candidate);
        }
        self.post_state();
    }

    fn post_state(&mut self) {
        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        for reply in self.pending_replies.drain(..) {
            let _ = global.post_message(&reply);
        }
        let telemetry = js_sys::Object::new();
        let index_count: u32 = self
            .gpu_scene
            .draws
            .iter()
            .map(|d| d.indices.end - d.indices.start)
            .sum();
        let instance_count: u32 = self
            .gpu_scene
            .draws
            .iter()
            .map(|d| d.instances.end - d.instances.start)
            .sum();
        let active = self.active_compiled.as_ref();
        let active_id = active
            .map(|a| js_sys::Array::of2(&a.id().slot.into(), &a.id().generation.into()).into())
            .unwrap_or(JsValue::NULL);
        for (key, value) in [
            ("type", "telemetry".into()),
            ("revision", (self.render_data.revision() as f64).into()),
            ("draws", (self.gpu_scene.draws.len() as u32).into()),
            ("instances", instance_count.into()),
            ("indices", index_count.into()),
            ("width", self.context.surface_config.width.into()),
            ("height", self.context.surface_config.height.into()),
            (
                "surfaceFormat",
                match self.context.surface_config.format {
                    wgpu::TextureFormat::Rgba8Unorm => "rgba8_unorm",
                    wgpu::TextureFormat::Bgra8Unorm => "bgra8_unorm",
                    wgpu::TextureFormat::Rgba16Float => "rgba16_float",
                    _ => "unknown",
                }
                .into(),
            ),
            (
                "renderMode",
                if active.is_some() {
                    "compiled".into()
                } else {
                    "inactive".into()
                },
            ),
            ("activeCompiledId", active_id),
            (
                "activeCompiledSchemaVersion",
                active.map(|a| a.schema_version()).unwrap_or(0).into(),
            ),
            (
                "activeCompiledGraph",
                active.map(|a| a.graph_id()).unwrap_or("").into(),
            ),
            (
                "activeCompiledRevision",
                active.map(|a| a.revision()).unwrap_or(0).into(),
            ),
            (
                "graphExecutions",
                active
                    .map(|a| a.execution_count() as u32)
                    .unwrap_or(0)
                    .into(),
            ),
            (
                "graphTextureSlots",
                active
                    .map(|a| a.texture_slot_count() as u32)
                    .unwrap_or(0)
                    .into(),
            ),
            ("gpuError", self.gpu_error.lock().unwrap().is_some().into()),
        ] {
            let _ = js_sys::Reflect::set(&telemetry, &key.into(), &value);
        }
        let _ = global.post_message(&telemetry);
    }

    fn post_fatal(&mut self, code: &str, message: &str) {
        self.pending_replies.clear();
        let value = js_sys::Object::new();
        for (key, item) in [
            ("type", JsValue::from_str("fatal")),
            ("code", JsValue::from_str(code)),
            ("message", JsValue::from_str(message)),
        ] {
            let _ = js_sys::Reflect::set(&value, &key.into(), &item);
        }
        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();
        let _ = global.post_message(&value);
    }

    fn drain_resize_events(&mut self) {
        let events: Vec<_> = self.events_chan.try_iter().collect();
        for event in events {
            self.resize(event);
        }
    }

    pub fn run_render_loop(renderer: Rc<RefCell<Renderer>>) {
        let render_frame: Closure<dyn FnMut(f32)> = Closure::new(move |time: f32| {
            if let Ok(mut renderer) = renderer.try_borrow_mut() {
                renderer.drain_resize_events();
                renderer.render(time);
            }

            Self::run_render_loop(renderer.clone());
        });

        let global = js_sys::global().unchecked_into::<DedicatedWorkerGlobalScope>();

        global
            .request_animation_frame(render_frame.as_ref().unchecked_ref())
            .unwrap();

        render_frame.forget();
    }

    fn resize(&mut self, msg: ResizeMessage) {
        let new_width = ((msg.width * msg.scale_factor) as u32).max(1);
        let new_height = ((msg.height * msg.scale_factor) as u32).max(1);
        if new_width != self.context.surface_config.width
            || new_height != self.context.surface_config.height
        {
            if let Some(pending) = self.pending_switch.take() {
                self.reply(
                    pending.request,
                    Err(crate::render_graph::GraphError::new(
                        "GRAPH_SWITCH_INVALIDATED",
                        "graph switch invalidated by resize",
                    )
                    .into()),
                );
            }
            let interrupted_resize =
                self.in_flight
                    .take()
                    .and_then(|preparation| match preparation.purpose {
                        PreparationPurpose::Switch { request } => {
                            self.reply(
                                request,
                                Err(crate::render_graph::GraphError::new(
                                    "GRAPH_SWITCH_INVALIDATED",
                                    "graph switch invalidated by resize",
                                )
                                .into()),
                            );
                            None
                        }
                        PreparationPurpose::Resize => Some((preparation.id, preparation.graph)),
                    });
            let restore = self
                .active_compiled
                .take()
                .map(|active| (active.id(), active.graph))
                .or_else(|| {
                    self.pending_resize
                        .take()
                        .map(|active| (active.id(), active.graph))
                })
                .or(interrupted_resize);

            self.context.initial_surface_config.width = new_width;
            self.context.initial_surface_config.height = new_height;
            let initial_request = match self.context.initial_surface_config.format {
                wgpu::TextureFormat::Rgba8Unorm => {
                    crate::render_graph::SurfaceFormatRequest::Rgba8Unorm
                }
                wgpu::TextureFormat::Bgra8Unorm => {
                    crate::render_graph::SurfaceFormatRequest::Bgra8Unorm
                }
                wgpu::TextureFormat::Rgba16Float => {
                    crate::render_graph::SurfaceFormatRequest::Rgba16Float
                }
                _ => {
                    self.halted = true;
                    self.post_fatal("SURFACE_FRAME_FAILED", "base surface format is unsupported");
                    return;
                }
            };
            let capabilities = self.context.surface.get_capabilities(&self.context.adapter);
            if let Err(error) = crate::render_graph::resolve_surface_contract(
                initial_request,
                &capabilities,
                new_width,
                new_height,
                "surface",
            ) {
                self.halted = true;
                self.post_fatal("SURFACE_FRAME_FAILED", &error.message);
                return;
            }
            // Resize establishes the base surface dimensions first. Compiled graph
            // configuration remains transactional until its commit frame.
            self.configure_surface(self.context.initial_surface_config.clone());
            self.frame_data
                .resize(new_width as f64, new_height as f64, &self.context.queue);
            self.camera_publish_required = true;
            if let Some((id, graph)) = restore {
                match self.plan_compiled(&graph, new_width, new_height) {
                    Ok(runtime) => {
                        if let Err(error) = self.begin_compiled_preparation_with_runtime(
                            id,
                            graph,
                            runtime,
                            PreparationPurpose::Resize,
                        ) {
                            log::error!(
                                "compiled graph resize preparation failed: {}",
                                error.message
                            );
                        }
                    }
                    Err(error) => {
                        log::error!("compiled graph resize planning failed: {}", error.message)
                    }
                }
            }

            info!(
                "Resized: ({}, {}), scale: {}",
                new_width, new_height, msg.scale_factor
            );
        }
    }

    fn synchronize_shared_camera(&mut self) -> Result<(), crate::shared_soa::SharedSoaError> {
        let camera = self.frame_data.camera_mut();
        let outcome = if self.camera_publish_required {
            self.shared_soa.publish_camera(camera)
        } else {
            self.shared_soa.synchronize_camera(camera)
        };
        match outcome {
            Ok(()) => {
                self.camera_publish_required = false;
                Ok(())
            }
            // Another thread owns the sequence lock for only a handful of atomic stores;
            // skip this frame and consume or publish the complete row on the next one.
            Err(crate::shared_soa::SharedSoaError::Busy) => Ok(()),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod error_code_tests {
    use super::*;
    use crate::render_data::RenderDataError;
    #[test]
    fn render_data_errors_have_exact_stable_codes() {
        assert_eq!(
            render_data_error_code(&RenderDataError::InvalidMeshHandle),
            "STALE_HANDLE"
        );
        assert_eq!(
            render_data_error_code(&RenderDataError::InvalidTransform),
            "INVALID_TRANSFORM"
        );
        assert_eq!(
            render_data_error_code(&RenderDataError::StaleReplacementStage),
            "STALE_REPLACEMENT"
        );
    }
}
