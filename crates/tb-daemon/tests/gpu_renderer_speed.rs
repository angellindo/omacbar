//! touchbard's GPU renderer keeps one GPU device for its life: it draws a layer switch far faster
//! than `render_frame`, which creates a device, pipeline and target per call. Relative, so it
//! holds on any adapter (lavapipe in CI too) and under load: each round times one of each on the
//! same scene, interleaved, warm-up untimed. Its own test file, so its own process.
//! Measured on the Intel HD 630: about 4 ms vs 32 ms; a device per frame gives a ratio near 1.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr)]

use std::time::{Duration, Instant};

use tb_core::{BarSize, BarTheme, Chord, SpacingRules, presets};
use tb_daemon::{GpuRenderer, SceneRenderer};
use tb_render::{BarScene, build_scene, render_frame};

const ROUNDS: usize = 11;

fn scene(chord: Chord) -> BarScene {
    let layout = presets::default_layout();
    build_scene(
        layout.layer(chord).unwrap(),
        &BarTheme::omarchy_fallback(),
        BarSize::MACBOOK_PRO,
        &SpacingRules::STANDARD,
        None,
    )
    .unwrap()
}

fn start() -> Result<GpuRenderer, tb_render::RenderError> {
    GpuRenderer::new()
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

#[test]
fn the_gpu_renderer_draws_a_layer_switch_far_faster_than_a_new_device_does() {
    let scenes = [scene(Chord::Default), scene(Chord::Fn)];
    let mut gpu = start().expect("a Vulkan adapter (lavapipe is fine)");
    for s in &scenes {
        gpu.render(s);
        render_frame(s.clone());
    }

    let (mut kept, mut fresh) = (Vec::new(), Vec::new());
    for i in 0..ROUNDS {
        let s = &scenes[(i + 1) % 2];
        let t = Instant::now();
        let a = gpu.render(s);
        kept.push(t.elapsed());
        let t = Instant::now();
        let b = render_frame(s.clone());
        fresh.push(t.elapsed());
        assert!(a == b, "round {i}: the GPU renderer's frame differs from render_frame");
    }
    let (kept, fresh) = (median(kept), median(fresh));
    eprintln!(
        "median of {ROUNDS}: GPU renderer {:.2} ms, render_frame {:.2} ms, ratio {:.1}",
        kept.as_secs_f64() * 1e3,
        fresh.as_secs_f64() * 1e3,
        fresh.as_secs_f64() / kept.as_secs_f64(),
    );
    assert!(kept * 3 < fresh, "GPU renderer {kept:?} is not 3x faster than render_frame {fresh:?}");
}
