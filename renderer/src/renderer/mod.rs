use std::{cell::RefCell, rc::Rc, sync::mpsc::Receiver};

use futures::channel::oneshot;
use log::info;
use ultraviolet::Vec4;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use wasm_bindgen_futures::spawn_local;
use web_sys::DedicatedWorkerGlobalScope;

use crate::{
    command_ring::CommandRing,
    gltf::{install_imported, ModelBounds},
    message::{camera_drag, CameraDrag, DrainEventError, MouseMessage, ResizeMessage, WindowEvent},
    render_data::{InstanceHandle, MeshHandle, RenderData, RenderDataConfig, RenderFlags},
    renderer::scene::Scene,
};

pub mod executors;
pub mod gpu_scene;
pub mod material;
pub mod pipeline_library;
pub mod profiler;
pub mod scene;
pub mod scene_frame;

pub use pipeline_library::PipelineLibrary;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

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

fn resolve_fullscreen_entry(key: &str) -> Option<&'static str> {
    match key {
        "fullscreen_copy" => Some("fs_copy"),
        "frame_out" => Some("fs_frame_out"),
        "color_balance" => Some("fs_color_balance"),
        "exposure_contrast" => Some("fs_exposure_contrast"),
        "saturation" => Some("fs_saturation"),
        "channel_mixer" => Some("fs_channel_mixer"),
        "bloom_extract" => Some("fs_bloom_extract"),
        "bloom_blur" => Some("fs_bloom_blur"),
        "bloom_composite" => Some("fs_bloom_composite"),
        "luminance_edge" => Some("fs_luminance_edge"),
        _ => None,
    }
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

    #[test]
    fn fullscreen_entries_are_explicit() {
        assert_eq!(resolve_fullscreen_entry("fullscreen_copy"), Some("fs_copy"));
        assert_eq!(resolve_fullscreen_entry("frame_out"), Some("fs_frame_out"));
        assert_eq!(resolve_fullscreen_entry("tone_map"), None);
        assert_eq!(
            resolve_fullscreen_entry("color_balance"),
            Some("fs_color_balance")
        );
        assert_eq!(
            resolve_fullscreen_entry("exposure_contrast"),
            Some("fs_exposure_contrast")
        );
        assert_eq!(
            resolve_fullscreen_entry("saturation"),
            Some("fs_saturation")
        );
        assert_eq!(
            resolve_fullscreen_entry("channel_mixer"),
            Some("fs_channel_mixer")
        );
        assert_eq!(
            resolve_fullscreen_entry("bloom_extract"),
            Some("fs_bloom_extract")
        );
        assert_eq!(
            resolve_fullscreen_entry("bloom_blur"),
            Some("fs_bloom_blur")
        );
        assert_eq!(
            resolve_fullscreen_entry("bloom_composite"),
            Some("fs_bloom_composite")
        );
        assert_eq!(
            resolve_fullscreen_entry("luminance_edge"),
            Some("fs_luminance_edge")
        );
        assert_eq!(resolve_fullscreen_entry("unknown"), None);
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
        // Both WGSL balance branches are neutral on nonnegative RGB with neutral controls.
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
    FrustumCull,
    MeshQuery,
    PipelineRegistry,
    Pipeline {
        execution: usize,
        base: crate::render_data::PipelineKey,
        variant: wgpu::RenderPipeline,
    },
    Fullscreen {
        execution: usize,
        frame_out: bool,
        bind_group: wgpu::BindGroup,
        pipeline: wgpu::RenderPipeline,
        _uniform: wgpu::Buffer,
    },
}

struct ActiveCompiledGraph {
    id: crate::render_graph::CompiledGraphId,
    graph: crate::render_graph::CompiledGraph,
    runtime: crate::render_graph::RuntimePlan,
    textures: Vec<Vec<GpuTextureSlot>>,
    executions: Vec<PreparedExecution>,
    _fullscreen_layout: wgpu::BindGroupLayout,
}

#[derive(Clone, Copy)]
enum UploadGraph {
    Immediate,
    Compiled(crate::render_graph::MeshQueryRuntimeKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameTargetSource {
    PendingSwitch,
    PendingResize,
    Active,
    Immediate,
}

fn select_frame_target_source(
    pending_switch: bool,
    pending_resize: bool,
    active: bool,
) -> FrameTargetSource {
    if pending_switch {
        FrameTargetSource::PendingSwitch
    } else if pending_resize {
        FrameTargetSource::PendingResize
    } else if active {
        FrameTargetSource::Active
    } else {
        FrameTargetSource::Immediate
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
        FrameTargetSource::Active | FrameTargetSource::Immediate => match error {
            wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated => {
                AcquisitionAction::ReconfigureAndSkip
            }
            wgpu::SurfaceError::Timeout => AcquisitionAction::Skip,
            wgpu::SurfaceError::Other => AcquisitionAction::Halt,
            wgpu::SurfaceError::OutOfMemory => unreachable!(),
        },
    }
}

fn classify_upload_graph(graph: &ActiveCompiledGraph) -> UploadGraph {
    UploadGraph::Compiled(graph.runtime.allocations.query)
}

fn upload_query_for_render(
    pending: Option<UploadGraph>,
    active: Option<UploadGraph>,
) -> Option<crate::render_graph::MeshQueryRuntimeKey> {
    match pending.or(active) {
        Some(UploadGraph::Compiled(query)) => Some(query),
        Some(UploadGraph::Immediate) | None => None,
    }
}

fn resolve_culling_frustum(
    query: crate::render_graph::MeshQueryRuntimeKey,
    read: impl FnOnce() -> Option<Result<[[f32; 4]; 6], crate::camera::FrustumError>>,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    if matches!(
        query.frustum_culled,
        crate::render_graph::RuntimePredicate::Any | crate::render_graph::RuntimePredicate::Never
    ) {
        return Ok(None);
    }
    match read() {
        Some(Ok(planes)) => Ok(Some(planes)),
        Some(Err(error)) => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_FAILED",
            format!("camera frustum is invalid: {error}"),
        )),
        None => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_FAILED",
            "culling graph requires a camera frustum, but the scene has no camera",
        )),
    }
}

fn update_validate_write_scene<S: scene::Scene>(
    scene: &mut S,
    queue: &wgpu::Queue,
    query: Option<crate::render_graph::MeshQueryRuntimeKey>,
) -> Result<Option<[[f32; 4]; 6]>, crate::render_graph::GraphError> {
    scene.update_cpu();
    let planes = match query {
        Some(query) => resolve_culling_frustum(query, || scene.frustum_planes())?,
        None => None,
    };
    scene.write_uniforms(queue);
    Ok(planes)
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

enum SwitchTarget {
    Immediate,
    Compiled(ActiveCompiledGraph),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolvedSwitchRequest {
    Immediate,
    Compiled(crate::render_graph::CompiledGraphId),
}

fn resolve_switch_request(
    registry: &crate::render_graph::Registry,
    pending: bool,
    mode: u32,
    slot: u32,
    generation: u32,
) -> Result<ResolvedSwitchRequest, crate::render_graph::GraphError> {
    if pending {
        return Err(crate::render_graph::GraphError::new(
            "GRAPH_SWITCH_PENDING",
            "a graph switch is pending",
        ));
    }
    match mode {
        0 if slot == 0 && generation == 0 => Ok(ResolvedSwitchRequest::Immediate),
        0 => Err(crate::render_graph::GraphError::new(
            "STALE_GRAPH_ID",
            "immediate mode requires a zero id",
        )),
        1 => {
            let id = crate::render_graph::CompiledGraphId { slot, generation };
            // Resolve the registry entry here, before any GPU preparation or pending
            // state mutation. Registry::get is also the Phase 4 activation gate.
            registry.get(id)?;
            Ok(ResolvedSwitchRequest::Compiled(id))
        }
        _ => Err(crate::render_graph::GraphError::new(
            "GRAPH_EXECUTION_UNSUPPORTED",
            "unknown render mode",
        )),
    }
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
        serde_json::to_vec(&graph).unwrap()
    }

    use super::*;

    fn query(visible: crate::render_graph::RuntimePredicate) -> UploadGraph {
        UploadGraph::Compiled(crate::render_graph::MeshQueryRuntimeKey {
            visible,
            frustum_culled: crate::render_graph::RuntimePredicate::Any,
        })
    }

    #[test]
    fn upload_selection_follows_the_graph_rendered_for_the_commit_frame() {
        use crate::render_graph::RuntimePredicate::{Any, RequiredFalse, RequiredTrue};
        let selected =
            |pending, active| upload_query_for_render(pending, active).map(|query| query.visible);
        assert_eq!(
            selected(Some(query(RequiredFalse)), Some(query(RequiredTrue))),
            Some(RequiredFalse)
        );
        assert_eq!(
            selected(Some(UploadGraph::Immediate), Some(query(RequiredTrue))),
            None
        );
        assert_eq!(
            selected(Some(UploadGraph::Immediate), Some(query(RequiredTrue))),
            None
        );
        assert_eq!(
            selected(None, Some(query(RequiredTrue))),
            Some(RequiredTrue)
        );
        assert_eq!(selected(None, Some(UploadGraph::Immediate)), None);
        assert_eq!(selected(None, None), None);
        assert_eq!(selected(Some(query(Any)), None), Some(Any));
    }

    #[test]
    fn frame_target_precedence_and_pending_resize_upload_are_exact() {
        use crate::render_graph::RuntimePredicate::{RequiredFalse, RequiredTrue};
        assert_eq!(
            select_frame_target_source(true, true, true),
            FrameTargetSource::PendingSwitch
        );
        assert_eq!(
            select_frame_target_source(false, true, true),
            FrameTargetSource::PendingResize
        );
        assert_eq!(
            select_frame_target_source(false, false, true),
            FrameTargetSource::Active
        );
        assert_eq!(
            select_frame_target_source(false, false, false),
            FrameTargetSource::Immediate
        );
        let pending_resize = query(RequiredFalse);
        let active = query(RequiredTrue);
        assert_eq!(
            upload_query_for_render(Some(pending_resize), Some(active))
                .unwrap()
                .visible,
            RequiredFalse
        );
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
        for source in [Active, Immediate] {
            assert_eq!(acquisition_action(source, &Lost), ReconfigureAndSkip);
            assert_eq!(acquisition_action(source, &Outdated), ReconfigureAndSkip);
            assert_eq!(acquisition_action(source, &Timeout), Skip);
            assert_eq!(acquisition_action(source, &Other), Halt);
        }
        for source in [PendingSwitch, PendingResize, Active, Immediate] {
            assert_eq!(acquisition_action(source, &OutOfMemory), Halt);
        }
    }

    #[test]
    fn frustum_preflight_skips_any_and_distinguishes_missing_from_invalid() {
        use crate::render_graph::RuntimePredicate::{Any, RequiredFalse, RequiredTrue};
        let query = |frustum_culled| crate::render_graph::MeshQueryRuntimeKey {
            visible: RequiredTrue,
            frustum_culled,
        };
        let mut reads = 0;
        assert_eq!(
            resolve_culling_frustum(query(Any), || {
                reads += 1;
                None
            })
            .unwrap(),
            None
        );
        assert_eq!(
            reads, 0,
            "inactive frustum filtering must not read the camera"
        );
        let missing = resolve_culling_frustum(query(RequiredFalse), || None).unwrap_err();
        assert!(missing.message.contains("no camera"));
        let invalid = resolve_culling_frustum(query(RequiredFalse), || {
            Some(Err(crate::camera::FrustumError::Degenerate { plane: 2 }))
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
        let bytes = serde_json::to_vec(&graph).unwrap();
        let (id, _) = registry.compile(&bytes).unwrap();
        let active = "existing_graph";
        let pending: Option<&str> = None;
        assert_eq!(
            resolve_switch_request(&registry, false, 1, id.slot, id.generation).unwrap(),
            ResolvedSwitchRequest::Compiled(id)
        );
        assert_eq!(active, "existing_graph");
        assert_eq!(pending, None);
        assert!(registry.contains(id));

        let pending_error = resolve_switch_request(&registry, true, 1, id.slot, id.generation)
            .expect_err("an existing pending request must win");
        assert_eq!(pending_error.code, "GRAPH_SWITCH_PENDING");
        assert_eq!(active, "existing_graph");
        assert_eq!(pending, None);

        let invalid_replacement =
            br#"{"schemaVersion":2,"graphId":"switch","revision":2,"nodes":[],"unexpected":true}"#;
        assert_eq!(
            registry.compile(invalid_replacement).unwrap_err().code,
            "GRAPH_JSON_INVALID"
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
    target: SwitchTarget,
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
    pub depth_texture: wgpu::Texture,
    pub depth_view: wgpu::TextureView,
}

pub struct Renderer<T: scene::Scene> {
    canvas: web_sys::OffscreenCanvas,
    events_chan: Receiver<WindowEvent>,
    context: RendererContext,
    resources: PipelineLibrary,
    scene: T,
    render_data: RenderData,
    snapshot: crate::shared_snapshot::SharedSnapshot,
    snapshot_init_sent: bool,
    scene_frame: scene_frame::SceneFrameCache,
    gpu_scene: gpu_scene::GpuSceneCache,
    materials: material::MaterialResources,
    pub(crate) command_ring: Option<&'static CommandRing>,
    pending_replies: Vec<JsValue>,
    gpu_error: std::sync::Arc<std::sync::atomic::AtomicBool>,
    framing_radius: f32,
    graph_registry: crate::render_graph::Registry,
    active_compiled: Option<ActiveCompiledGraph>,
    pending_switch: Option<PendingSwitch>,
    pending_resize: Option<ActiveCompiledGraph>,
    in_flight: Option<InFlightPreparation>,
    next_preparation_token: u64,
    preparation_completions: Rc<RefCell<Vec<PreparationCompletion>>>,
    halted: bool,
    profiler: profiler::Profiler,
}

impl<T: Scene + 'static> Renderer<T> {
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
                    self.pending_switch.as_ref().and_then(|pending| {
                        if let SwitchTarget::Compiled(active) = &pending.target {
                            Some(active.id())
                        } else {
                            None
                        }
                    }),
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
                    words[4],
                )
                .and_then(|target| match target {
                    ResolvedSwitchRequest::Immediate => {
                        self.pending_switch = Some(PendingSwitch {
                            request,
                            target: SwitchTarget::Immediate,
                        });
                        Ok(())
                    }
                    ResolvedSwitchRequest::Compiled(id) => {
                        let graph = self.graph_registry.get(id)?.clone();
                        self.begin_compiled_preparation(
                            id,
                            graph,
                            PreparationPurpose::Switch { request },
                        )
                    }
                });
                if let Err(error) = outcome {
                    self.reply(request, Err(error.into()));
                }
                continue;
            }
            let outcome: Result<JsValue, &'static str> = (|| match opcode {
                1 => {
                    if words[3] > 1 {
                        return Err("INVALID_FRAMING");
                    }
                    let bytes = crate::take_payload(words[2]).ok_or("PAYLOAD_MISSING")?;
                    let imported =
                        crate::gltf::decode_gltf_owned(bytes).map_err(|_| "GLB_INVALID")?;
                    let pipelines = Self::ensure_gltf_pipelines(&mut self.resources, &self.context);
                    // Build a complete GPU candidate first. Neither the live scene nor
                    // its material epoch changes if image decode/resource creation fails.
                    let prepared_materials = self
                        .materials
                        .prepare(&self.context.device, &self.context.queue, &imported)
                        .map_err(|_| "MATERIAL_INVALID")?;
                    let installed = install_imported(&mut self.render_data, &imported, pipelines)
                        .map_err(|_| "INSTALL_FAILED")?;
                    // RenderData replacement and material publication are adjacent in
                    // this synchronous command, preventing a frame with mixed assets.
                    self.materials
                        .install(prepared_materials, self.render_data.revision());
                    if let Some(ModelBounds { min, max }) = installed.bounds {
                        let center = ultraviolet::Vec3::new(
                            (min[0] + max[0]) * 0.5,
                            (min[1] + max[1]) * 0.5,
                            (min[2] + max[2]) * 0.5,
                        );
                        let extent = ultraviolet::Vec3::new(
                            max[0] - min[0],
                            max[1] - min[1],
                            max[2] - min[2],
                        );
                        let radius = (0.5
                            * (extent.x * extent.x + extent.y * extent.y + extent.z * extent.z)
                                .sqrt())
                        .max(1.0);
                        self.framing_radius = radius;
                        log::info!(
                            "framing imported scene: center={center:?}, extent={extent:?}, radius={radius}"
                        );
                        self.scene.set_camera_depth_range(
                            (radius * 0.001).max(0.1),
                            (radius * 6.0).max(1.1),
                        );
                        if words[3] == 1 {
                            self.scene.set_camera_look_at(
                                center + ultraviolet::Vec3::new(0.0, radius * 0.05, 0.0),
                                center + ultraviolet::Vec3::new(radius, 0.0, 0.0),
                            );
                        } else {
                            self.scene.set_camera_look_at(
                                center
                                    + ultraviolet::Vec3::new(
                                        radius * 1.8,
                                        radius * 1.4,
                                        radius * 1.8,
                                    ),
                                center,
                            );
                        }
                    }
                    let result = js_sys::Object::new();
                    let meshes = js_sys::Array::new();
                    for h in installed.meshes {
                        meshes.push(&js_sys::Array::of2(
                            &h.slot().into(),
                            &h.generation().into(),
                        ));
                    }
                    js_sys::Reflect::set(&result, &"meshes".into(), &meshes).unwrap();
                    Ok(result.into())
                }
                2 => {
                    self.render_data
                        .set_mesh_flags(
                            MeshHandle::from_parts(words[2], words[3]),
                            RenderFlags::from_bits_retain(words[4]),
                        )
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                3 => {
                    let mesh = MeshHandle::from_parts(words[2], words[3]);
                    let mut m = [[0.; 4]; 4];
                    for i in 0..16 {
                        m[i / 4][i % 4] = f32::from_bits(words[4 + i]);
                    }
                    let h = self
                        .render_data
                        .create_instance(mesh, m, RenderFlags::from_bits_retain(words[20]))
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(js_sys::Array::of2(&h.slot().into(), &h.generation().into()).into())
                }
                4 => {
                    self.render_data
                        .set_instance_flags(
                            InstanceHandle::from_parts(words[2], words[3]),
                            RenderFlags::from_bits_retain(words[4]),
                        )
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                5 => {
                    let h = InstanceHandle::from_parts(words[2], words[3]);
                    let mut m = [[0.; 4]; 4];
                    for i in 0..16 {
                        m[i / 4][i % 4] = f32::from_bits(words[4 + i]);
                    }
                    self.render_data
                        .set_instance_transform(h, m)
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
                }
                6 => {
                    self.render_data
                        .destroy_instance(InstanceHandle::from_parts(words[2], words[3]))
                        .map_err(|e| render_data_error_code(&e))?;
                    Ok(JsValue::UNDEFINED)
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

    fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let size = wgpu::Extent3d {
            width: config.width.max(1),
            height: config.height.max(1),
            depth_or_array_layers: 1,
        };

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("depth texture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        (texture, view)
    }

    fn recreate_depth_texture(&mut self) {
        let (texture, view) =
            Self::create_depth_texture(&self.context.device, &self.context.surface_config);
        self.context.depth_texture = texture;
        self.context.depth_view = view;
    }

    fn ensure_gltf_pipelines(
        resources: &mut PipelineLibrary,
        context: &RendererContext,
    ) -> [crate::render_data::PipelineKey; 2] {
        let layout = gpu_scene::vertex_layouts();
        let culled = resources.get_or_create_pipeline(
            &context.device,
            "gltf_standard",
            &layout,
            include_str!("../gltf.wgsl"),
            context.initial_surface_config.format,
        );
        let double_sided = resources.get_or_create_pipeline(
            &context.device,
            "gltf_standard_double_sided",
            &layout,
            include_str!("../gltf.wgsl"),
            context.initial_surface_config.format,
        );
        [culled, double_sided]
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
        let resolved_pipelines = graph
            .executions
            .iter()
            .enumerate()
            .map(|(index, execution)| {
                let NormalizedParameters::Pipeline { pipeline, .. } = &execution.parameters else {
                    return Ok(None);
                };
                self.resources
                    .find_pipeline(pipeline)
                    .map(Some)
                    .ok_or_else(|| {
                        GraphError::at(
                            "GRAPH_EXECUTION_UNSUPPORTED",
                            format!("pipeline '{pipeline}' is not registered"),
                            format!("executions[{index}].parameters.pipeline"),
                        )
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
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
        let shader = self
            .context
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(" fullscreen"),
                source: wgpu::ShaderSource::Wgsl(include_str!("fullscreen_copy.wgsl").into()),
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
            let contract = crate::render_graph::contract(&execution.executor.key)
                .ok_or_else(|| fail("executor contract missing"))?;
            match execution.executor.key.as_str() {
                "frustum_cull" => executions.push(PreparedExecution::FrustumCull),
                "mesh_query" => executions.push(PreparedExecution::MeshQuery),
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
                        let ExecutionKind::Render {
                            color_attachments, ..
                        } = &execution.kind
                        else {
                            return Err(fail("fullscreen execution is not render"));
                        };
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
                    let entry = resolve_fullscreen_entry(&execution.executor.key)
                        .ok_or_else(|| fail("fullscreen executor mismatch"))?;
                    let pipeline = self.context.device.create_render_pipeline(
                        &wgpu::RenderPipelineDescriptor {
                            label: Some(" post pipeline"),
                            layout: Some(&pipeline_layout),
                            vertex: wgpu::VertexState {
                                module: &shader,
                                entry_point: Some("vs_main"),
                                buffers: &[],
                                compilation_options: Default::default(),
                            },
                            primitive: Default::default(),
                            depth_stencil: None,
                            multisample: Default::default(),
                            fragment: Some(wgpu::FragmentState {
                                module: &shader,
                                entry_point: Some(entry),
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
                        execution: index,
                        frame_out,
                        bind_group,
                        pipeline,
                        _uniform: uniform,
                    });
                }
                "pipeline_registry" => executions.push(PreparedExecution::PipelineRegistry),
                "pipeline" => {
                    let ExecutionKind::Render {
                        color_attachments,
                        depth_stencil,
                    } = &execution.kind
                    else {
                        return Err(fail("pipeline is not render"));
                    };
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
                    let NormalizedParameters::Pipeline {
                        pipeline: _,
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
                        )
                        .map_err(|e| GraphError::new("GRAPH_RUNTIME_PLAN_INVALID", e))?;
                    executions.push(PreparedExecution::Pipeline {
                        execution: index,
                        base,
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
        resolve_culling_frustum(runtime.allocations.query, || self.scene.frustum_planes())?;
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
                        target: SwitchTarget::Compiled(candidate),
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
        events_chan: Receiver<WindowEvent>,
        profile: bool,
    ) -> Self {
        let id = wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU,
            ..Default::default()
        };

        let instance = wgpu::Instance::new(&id);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))
            .unwrap();
        let mut adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .unwrap();

        let optional_features = profiler::Profiler::requested_features(profile, adapter.features());
        let descriptor = wgpu::DeviceDescriptor {
            required_features: optional_features,
            required_limits: wgpu::Limits::default(),
            label: None,
            memory_hints: wgpu::MemoryHints::default(),
            trace: wgpu::Trace::default(),
        };

        let (device, queue) = match adapter.request_device(&descriptor).await {
            Ok(result) => result,
            Err(error) if !optional_features.is_empty() => {
                log::warn!("timestamp-enabled device request failed, retrying baseline: {error}");
                adapter = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        compatible_surface: Some(&surface),
                        force_fallback_adapter: false,
                        ..Default::default()
                    })
                    .await
                    .expect("surface-compatible adapter required for baseline device");
                let baseline = wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: None,
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::default(),
                };
                adapter.request_device(&baseline).await.unwrap()
            }
            Err(error) => panic!("baseline WebGPU device request failed: {error}"),
        };
        info!("Adapter info: {:?}", adapter.get_info());
        info!("Adapter features: {:?}", adapter.features());
        info!("Adapter limits: {:?}", adapter.limits());
        let profiler = profiler::Profiler::new(profile, &device, &queue).await;
        let gpu_error = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let error_flag = gpu_error.clone();
        device.on_uncaptured_error(Box::new(move |error| {
            error_flag.store(true, std::sync::atomic::Ordering::Relaxed);
            log::error!("Uncaptured GPU error: {error}");
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

        let (depth_texture, depth_view) = Self::create_depth_texture(&device, &surface_config);

        let mut resources = PipelineLibrary::new();
        let context = RendererContext {
            adapter,
            surface,
            device,
            queue,
            initial_surface_config: surface_config.clone(),
            surface_config,
            depth_texture,
            depth_view,
        };

        let mut render_data =
            RenderData::new(RenderDataConfig::default()).expect("valid render data config");
        let scene = T::setup(&context, &mut resources, &mut render_data);
        let materials = material::MaterialResources::new(&context.device, &context.queue);
        resources.set_material_bind_group_layout(&materials.layout);
        Self::ensure_gltf_pipelines(&mut resources, &context);

        Self {
            canvas,
            events_chan,
            context,
            scene,
            resources,
            render_data,
            snapshot: crate::shared_snapshot::SharedSnapshot::new(),
            snapshot_init_sent: false,
            scene_frame: Default::default(),
            gpu_scene: Default::default(),
            materials,
            command_ring: None,
            pending_replies: Vec::new(),
            gpu_error,
            framing_radius: 0.0,
            graph_registry: Default::default(),
            active_compiled: None,
            pending_switch: None,
            pending_resize: None,
            in_flight: None,
            next_preparation_token: 0,
            preparation_completions: Default::default(),
            halted: false,
            profiler,
        }
    }

    fn render(&mut self, _time: f32) {
        if self.halted {
            return;
        }
        self.drain_preparation_completions();
        if self
            .gpu_error
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            self.halted = true;
            self.post_fatal("GPU_VALIDATION_FAILED", "uncaptured WebGPU error");
            return;
        }
        if !self.drain_commands() {
            return;
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
            let _ = js_sys::Reflect::set(&message, &"controlVersion".into(), &1.into());
            let _ = js_sys::Reflect::set(&message, &"schemaVersion".into(), &1.into());
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
        let pending = self.pending_switch.as_ref().map(|p| match &p.target {
            SwitchTarget::Immediate => UploadGraph::Immediate,
            SwitchTarget::Compiled(graph) => classify_upload_graph(graph),
        });
        let target_source = select_frame_target_source(
            self.pending_switch.is_some(),
            self.pending_resize.is_some(),
            self.active_compiled.is_some(),
        );
        let selected = match target_source {
            FrameTargetSource::PendingSwitch => pending,
            FrameTargetSource::PendingResize => {
                self.pending_resize.as_ref().map(classify_upload_graph)
            }
            FrameTargetSource::Active => self.active_compiled.as_ref().map(classify_upload_graph),
            FrameTargetSource::Immediate => Some(UploadGraph::Immediate),
        };
        let query = upload_query_for_render(selected, None);
        // Resolve again immediately before every active frame. Do this before scene
        // upload so an invalid camera cannot mutate GPU state or produce a frame.
        let planes = match update_validate_write_scene(&mut self.scene, &self.context.queue, query)
        {
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
                    FrameTargetSource::Active | FrameTargetSource::Immediate => {
                        self.post_fatal("GRAPH_EXECUTION_FAILED", &error.message);
                    }
                }
                return;
            }
        };
        let upload = if let Some(query) = query {
            self.gpu_scene.upload_with_query(
                &self.context.device,
                &self.context.queue,
                frame_plan,
                query,
            )
        } else {
            self.gpu_scene
                .upload(&self.context.device, &self.context.queue, frame_plan)
        };
        if let Err(error) = upload {
            log::error!("GPU scene upload failed: {error}");
            self.post_fatal("GPU_UPLOAD_FAILED", &error);
            return;
        }
        if let Some(query) = query {
            self.gpu_scene
                .write_culling_params(&self.context.queue, planes, query);
        }

        // Candidate publication is transactional: configure its complete contract at
        // the last possible point before acquisition, but retain the known-good
        // configuration and active graph identity until presentation succeeds.
        let candidate_config = match target_source {
            FrameTargetSource::PendingSwitch => match &self.pending_switch.as_ref().unwrap().target
            {
                SwitchTarget::Compiled(active) => Self::surface_config(&active.runtime.surface),
                SwitchTarget::Immediate => {
                    let mut config = self.context.initial_surface_config.clone();
                    config.width = self.context.surface_config.width;
                    config.height = self.context.surface_config.height;
                    config
                }
            },
            FrameTargetSource::PendingResize => {
                Self::surface_config(&self.pending_resize.as_ref().unwrap().runtime.surface)
            }
            FrameTargetSource::Active | FrameTargetSource::Immediate => {
                self.context.surface_config.clone()
            }
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
            FrameTargetSource::PendingSwitch => match &self.pending_switch.as_ref().unwrap().target
            {
                SwitchTarget::Compiled(active) => Some(active),
                SwitchTarget::Immediate => None,
            },
            FrameTargetSource::PendingResize => self.pending_resize.as_ref(),
            FrameTargetSource::Active => self.active_compiled.as_ref(),
            FrameTargetSource::Immediate => None,
        };
        let mut profile_frame = self.profiler.begin(|| match rendering_compiled {
            None => "immediate".to_owned(),
            Some(active) => format!(
                "graph:{}:{}:{}:{}",
                active.graph.graph_id, active.graph.revision, active.id.slot, active.id.generation
            ),
        });
        let encode_result = if let Some(active) = rendering_compiled {
            executors::encode_compiled(
                &mut encoder,
                &texture_view,
                active,
                &self.scene,
                &self.gpu_scene,
                &self.resources,
                &self.materials,
                profile_frame.as_mut(),
            )
        } else {
            executors::encode_immediate(
                &mut encoder,
                &texture_view,
                &self.context.depth_view,
                &self.scene,
                &self.gpu_scene,
                &self.resources,
                &self.materials,
                profile_frame.as_mut(),
            );
            Ok(())
        };
        if let Err(error) = encode_result {
            if let Some(frame) = profile_frame.take() {
                self.profiler.cancel(frame);
            }
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
        let profile_map = profile_frame.and_then(|frame| self.profiler.finish(&mut encoder, frame));
        self.context.queue.submit(std::iter::once(encoder.finish()));
        if let Some(request) = profile_map {
            self.profiler.map(request);
        }
        surface_texture.present();
        if let Some(pending) = self.pending_switch.take() {
            // A successful user switch supersedes any resize recreation of the
            // previously active graph. Retain that resize candidate only as a
            // fallback while the switch is being prepared or attempted.
            self.pending_resize = None;
            let result = match pending.target {
                SwitchTarget::Immediate => {
                    self.active_compiled = None;
                    js_sys::JSON::parse(r#"{"mode":"immediate"}"#).unwrap_or(JsValue::NULL)
                }
                SwitchTarget::Compiled(active) => {
                    let summary = serde_json::json!({
                        "mode":"compiled",
                        "compiledId":[active.id().slot, active.id().generation],
                        "graphId":active.graph_id(),
                        "revision":active.revision(),
                        "schemaVersion":active.schema_version()
                    });
                    self.active_compiled = Some(active);
                    js_sys::JSON::parse(&summary.to_string()).unwrap_or(JsValue::NULL)
                }
            };
            self.reply(pending.request, Ok(result));
        } else if let Some(candidate) = self.pending_resize.take() {
            self.active_compiled = Some(candidate);
        }
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
            ("framingRadius", self.framing_radius.into()),
            (
                "renderMode",
                if active.is_some() {
                    "compiled".into()
                } else {
                    "immediate".into()
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
            (
                "gpuError",
                self.gpu_error
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .into(),
            ),
        ] {
            let _ = js_sys::Reflect::set(&telemetry, &key.into(), &value);
        }
        let _ = global.post_message(&telemetry);
        if let Some(snapshot) = self.profiler.snapshot_json(js_sys::Date::now()) {
            let _ = global.post_message(&snapshot);
        }
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

    pub async fn read_pixel_from_texture(&self, x: u32, y: u32) -> Vec4 {
        let width = self.context.depth_texture.width();
        let height = self.context.depth_texture.height();

        if width == 0 || height == 0 {
            log::warn!("Depth texture has zero extent ({} x {})", width, height);
            return Vec4::zero();
        }

        // Validate coordinates
        if x >= width || y >= height {
            log::warn!(
                "Pixel coordinates ({}, {}) out of bounds for texture size {}x{}",
                x,
                y,
                width,
                height
            );
            return Vec4::zero();
        }

        let pixel_size = std::mem::size_of::<f32>() as u32;
        let unpadded_row_bytes = width * pixel_size;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row_bytes = if unpadded_row_bytes % align == 0 {
            unpadded_row_bytes
        } else {
            (unpadded_row_bytes / align + 1) * align
        };
        let buffer_size = padded_row_bytes as u64 * height as u64;
        let buffer = self.context.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("depth pixel read buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        // Copy just the single pixel
        let mut encoder =
            self.context
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("copy depth pixel to buffer"),
                });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.context.depth_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: 0 },
                aspect: wgpu::TextureAspect::DepthOnly,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.context.queue.submit(std::iter::once(encoder.finish()));

        // Map the buffer and read the pixel
        let slice = buffer.slice(..);
        let (tx, rx) = oneshot::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).unwrap();
        });

        // Poll the device to process the mapping

        rx.await.unwrap().unwrap();
        let depth_value = {
            let data = slice.get_mapped_range();
            let row_pitch = padded_row_bytes as usize;
            let byte_offset = y as usize * row_pitch + x as usize * pixel_size as usize;
            let mut depth_bytes = [0u8; 4];
            depth_bytes.copy_from_slice(&data[byte_offset..byte_offset + 4]);
            f32::from_le_bytes(depth_bytes)
        };
        buffer.unmap();

        Vec4::new(depth_value, 0.0, 0.0, 0.0)
    }

    pub async fn handle_event(renderer: Rc<RefCell<Self>>, event: WindowEvent) {
        match event {
            WindowEvent::PointerMove(msg) => {
                renderer.borrow_mut().mouse_move(msg);
            }
            WindowEvent::Resize(msg) => {
                renderer.borrow_mut().resize(msg);
            }
            WindowEvent::PointerClick(msg) => {
                {
                    log::info!("click start");

                    let mut r = renderer.borrow_mut();
                    let x = (msg.offset_x * msg.scale_factor) as f32;
                    let y = (msg.offset_y * msg.scale_factor) as f32;
                    r.scene.handle_mouse_click(x, y);
                    log::info!("clicked");
                }

                // Read pixel from depth texture at click coordinates
                // let renderer_clone = renderer.clone();
                // let x_coord = msg.offset_x as u32;
                // let y_coord = msg.offset_y as u32;
                // let pixel_value = renderer_clone
                //     .borrow()
                //     .read_pixel_from_texture(x_coord, y_coord)
                //     .await;
                // log::info!(
                //     "Depth pixel at ({}, {}): {:?}",
                //     x_coord,
                //     y_coord,
                //     pixel_value
                // );
            }
            WindowEvent::PointerWheel(msg) => {
                let mut r = renderer.borrow_mut();
                r.scene.handle_zoom(msg.delta_y_pixels);
            }
            WindowEvent::Keyboard(_) => {}
        }
    }

    fn drain_events(renderer: &Rc<RefCell<Self>>) -> Result<(), DrainEventError> {
        loop {
            let event = renderer.try_borrow_mut()?.events_chan.try_recv()?;

            let renderer_clone = renderer.clone();
            spawn_local(async move {
                Self::handle_event(renderer_clone, event).await;
            });
        }
    }

    pub fn run_render_loop(renderer: Rc<RefCell<Renderer<T>>>) {
        let render_frame: Closure<dyn FnMut(f32)> = Closure::new(move |time: f32| {
            {
                if let Err(e) = Self::drain_events(&renderer) {
                    match e {
                        DrainEventError::ChannelEmpty => {
                            // Normal condition, no error needed
                        }
                        DrainEventError::ChannelDisconnected => {
                            log::warn!("Event channel disconnected; stopping event polling");
                        }
                        DrainEventError::BorrowError(_) => {
                            log::error!("Failed to borrow renderer: {}", e);
                        }
                    }
                }
            }

            {
                if let Ok(mut r) = renderer.try_borrow_mut() {
                    r.render(time);
                }
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
                    self.post_fatal(
                        "SURFACE_FRAME_FAILED",
                        "immediate surface format is unsupported",
                    );
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
            // Resize always establishes the exact immediate fallback first. Compiled
            // configuration is transactional and is applied only by the commit frame.
            self.configure_surface(self.context.initial_surface_config.clone());
            self.recreate_depth_texture();
            self.scene.resize(
                new_width as f64,
                new_height as f64,
                msg.scale_factor,
                &self.context.queue,
            );
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

    pub fn mouse_move(&mut self, msg: MouseMessage) {
        let delta_x = msg.movement_x as f32;
        let delta_y = msg.movement_y as f32;
        match camera_drag(msg.buttons) {
            Some(CameraDrag::Orbit) => self.scene.handle_orbit(delta_x, delta_y),
            Some(CameraDrag::Pan) => {
                self.scene
                    .handle_pan(delta_x, delta_y, msg.viewport_height as f32);
            }
            None => {}
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
