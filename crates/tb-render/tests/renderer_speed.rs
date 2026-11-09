//! How fast a kept renderer draws a layer switch: render only, warm-up untimed, median of 21.
//! Its own test file, so its own process: no other GPU test runs beside it.
//!
//! The 20 ms bound holds on a hardware GPU (measured 4.1 ms median on the Intel HD 630 of the
//! MacBookPro14,3, debug build). On a CPU adapter (lavapipe in CI) the median is printed and not
//! bounded: there the `renderer_speedup` test, relative to `render_frame`, is the guard.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr)]

use std::time::{Duration, Instant};

use rune::render::Gpu;
use tb_core::{BarSize, BarTheme, Chord, SpacingRules, presets};
use tb_render::{BarScene, Renderer, build_scene};

const BOUND: Duration = Duration::from_millis(20);
const TIMED: usize = 21;

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

/// The kind of adapter Rune picks headless here, e.g. `IntegratedGpu` or `Cpu` (lavapipe).
fn adapter_kind() -> String {
    let gpu = Gpu::headless();
    format!("{:?}", gpu.adapter.get_info().device_type)
}

#[test]
fn a_kept_renderer_draws_a_layer_switch_in_under_20_ms_on_a_hardware_gpu() {
    let kind = adapter_kind();
    let (default, fn_layer) = (scene(Chord::Default), scene(Chord::Fn));
    let mut renderer = Renderer::new().expect("a Vulkan adapter (lavapipe is fine)");
    renderer.render(&default);
    renderer.render(&fn_layer);

    let mut times: Vec<Duration> = (0..TIMED)
        .map(|i| {
            let s = if i % 2 == 0 { &default } else { &fn_layer };
            let start = Instant::now();
            let frame = renderer.render(s);
            let took = start.elapsed();
            assert_eq!((frame.width, frame.height), (2170, 60));
            took
        })
        .collect();
    times.sort();
    let median = times[TIMED / 2];
    eprintln!(
        "kept renderer, layer switch ({kind} adapter): median {:.2} ms, min {:.2} ms, max {:.2} ms over {TIMED}",
        median.as_secs_f64() * 1e3,
        times[0].as_secs_f64() * 1e3,
        times[TIMED - 1].as_secs_f64() * 1e3,
    );
    if kind == "Cpu" {
        eprintln!("CPU adapter: the 20 ms bound applies to a hardware GPU only");
        return;
    }
    assert!(median < BOUND, "median {median:?} is not under {BOUND:?} ({kind} adapter)");
}
