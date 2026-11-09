//! A kept renderer draws a scene far faster than `render_frame`, which creates a GPU device,
//! pipeline and target per call. Relative, so it holds on any adapter (lavapipe in CI too) and
//! under load: each round times one of each on the same scene, interleaved, warm-up untimed.
//! Its own test file, so its own process. Measured on the Intel HD 630: about 4 ms vs 32 ms.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stderr)]

use std::time::{Duration, Instant};

use tb_core::{BarSize, BarTheme, Chord, SpacingRules, presets};
use tb_render::{BarScene, Renderer, build_scene, render_frame};

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

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort();
    times[times.len() / 2]
}

#[test]
fn a_kept_renderer_draws_a_scene_more_than_3_times_faster_than_render_frame() {
    let scenes = [scene(Chord::Default), scene(Chord::Fn)];
    let mut renderer = Renderer::new().expect("a Vulkan adapter (lavapipe is fine)");
    for s in &scenes {
        renderer.render(s);
        render_frame(s.clone());
    }

    let (mut kept, mut fresh) = (Vec::new(), Vec::new());
    for i in 0..ROUNDS {
        let s = &scenes[(i + 1) % 2];
        let start = Instant::now();
        let a = renderer.render(s);
        kept.push(start.elapsed());
        let start = Instant::now();
        let b = render_frame(s.clone());
        fresh.push(start.elapsed());
        assert!(a == b, "round {i}: kept renderer frame differs from render_frame");
    }
    let (kept, fresh) = (median(kept), median(fresh));
    eprintln!(
        "median of {ROUNDS}: kept renderer {:.2} ms, render_frame {:.2} ms, ratio {:.1}",
        kept.as_secs_f64() * 1e3,
        fresh.as_secs_f64() * 1e3,
        fresh.as_secs_f64() / kept.as_secs_f64(),
    );
    assert!(kept * 3 < fresh, "kept renderer {kept:?} is not 3x faster than render_frame {fresh:?}");
}
