//! The app's effect compositor: paints `El::effect` slots with `rune-fx` layers.
//!
//! Marks are minted by the graph view (`muninn_ui::fx_marks`). The compositor
//! reads each slot's mark and payload straight off its instance record, so the
//! UI never hands it anything but the four floats `El::effect_data` carries.
//!
//! Two things are painted here:
//!
//! - **Electric** (Settings › Graph): arcs on edges and the selection's border,
//!   a calmer charged border on direct neighbours. All three are
//!   `rune_fx::ElectricStyle`, so the preset reads as one treatment. Classic
//!   and Off emit no slots at all and this compositor only draws the UI.
//!   Settings › Graph › Effect intensity scales their amplitude, glow, forks
//!   and strand count, so the same preset can be subtle or intense.
//! - **The delete burn**: a short, low `FireStyle::Embers` fire over the node a
//!   deleted note left behind (see `muninn_ui::BurnMark`). Every preset except
//!   Off plays it, the intensity scales its heat and ember count, and reduced
//!   motion skips it.
//!
//! A Glow preset is intentionally absent: `rune-fx` glow only draws rounded-rect
//! blooms, so it could paint the node border but not an arbitrary diagonal edge
//! segment.

use muninn_ui::{BurnMark, GraphEffects, State, fx_marks};
use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune_fx::{
    ElectricParams, ElectricStyle, FireParams, FireStyle, Fx, FxFrame, FxLayer, FxOptions,
    FxRenderer, FxSlot, Rgb, Shape,
};
use wgpu::TextureFormat;

/// Scale one `rune-fx` knob by the Settings › Graph intensity, keeping the
/// result inside the range the shader documents for it (so Subtle and Intense
/// never push a knob out of bounds).
fn scaled(base: f32, scale: f32, lo: f32, hi: f32) -> f32 {
    (base * scale).clamp(lo, hi)
}

/// Scale a strand count by the intensity. `rune-fx` draws between one and
/// three strands, so Subtle collapses a pair to one and Intense saturates.
fn scaled_strands(base: u32, scale: f32) -> u32 {
    (base as f32 * scale).round().clamp(1.0, 3.0) as u32
}

#[derive(Default)]
pub struct GraphFx {
    fx: Option<FxRenderer>,
    animating: bool,
}

impl Compositor<State> for GraphFx {
    fn animating(&self) -> bool {
        self.animating
    }

    fn render(&mut self, f: &mut Frame<'_, State>) {
        let (device, queue, size) = (f.device, f.queue, f.size);
        let t = f.rt.t as f32;
        let reduced = f.rt.state.reduced_motion();
        let effects = f.rt.state.graph_effects();
        // Settings › Graph › Effect intensity, as a multiplier on the shape and
        // glow below. Balanced is 1.0, so the defaults are exactly the picture
        // every build before the setting drew.
        let boost = f.rt.state.effect_intensity().scale();
        // `view` publishes the resolved theme colours every frame; the
        // compositor runs outside its borrow. `Rgb` is linear, like the theme.
        let accent = Rgb(f.rt.state.fx_accent.get());
        let muted = Rgb(f.rt.state.fx_muted.get());

        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut placements: Vec<FxSlot> = Vec::new();
        let mut any_anim = false;

        for (n, i) in f.rt.instances.iter().enumerate() {
            if i.params[1] != KIND_EFFECT {
                continue;
            }
            // `push_effect` packs the slot's tag into `border` as two exact
            // 16-bit floats (see `rune::render::Instance`).
            let mark = (i.border[0] as u32) | ((i.border[1] as u32) << 16);
            let kind = mark & 0xffff_0000;
            let seed = (mark & 0xffff) + 1;
            let k = i.params[3];
            if k <= 0.0 {
                continue;
            }
            // Slots carry device pixels; effect shapes are in points, so undo
            // the slot's scale. `i.uv` is `effect_data`.
            let [x, y, w, h] = i.rect.map(|v| v / k);
            let data = i.uv;

            let layer = match kind {
                fx_marks::FX_NODE_SEL | fx_marks::FX_NODE_NEAR
                    if effects == GraphEffects::Electric =>
                {
                    let radius = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / k;
                    let shape = Shape::rounded(x, y, w.max(1.0), h.max(1.0), radius.max(8.0));
                    let hot = kind == fx_marks::FX_NODE_SEL;
                    any_anim |= !reduced;
                    if hot {
                        Fx::electric(ElectricParams {
                            style: ElectricStyle::Border,
                            color: accent,
                            core: accent,
                            amplitude: scaled(if reduced { 0.7 } else { 4.0 }, boost, 0.0, 24.0),
                            glow: scaled(5.0, boost, 1.0, 20.0),
                            forks: scaled(if reduced { 0.0 } else { 0.55 }, boost, 0.0, 1.0),
                            flicker: scaled(if reduced { 0.0 } else { 0.45 }, boost, 0.0, 1.0),
                            speed: if reduced { 0.0 } else { 3.5 },
                            strands: scaled_strands(2, boost),
                            gap: 3.0,
                            fork_len: scaled(22.0, boost, 2.0, 60.0),
                            intensity: scaled(1.0, boost, 0.0, 3.0),
                            ..ElectricParams::default()
                        })
                    } else {
                        Fx::electric(ElectricParams {
                            style: ElectricStyle::Charged,
                            color: muted,
                            core: accent,
                            amplitude: scaled(if reduced { 0.5 } else { 1.1 }, boost, 0.0, 24.0),
                            glow: scaled(4.0, boost, 1.0, 20.0),
                            forks: scaled(if reduced { 0.0 } else { 0.25 }, boost, 0.0, 1.0),
                            flicker: scaled(if reduced { 0.0 } else { 0.2 }, boost, 0.0, 1.0),
                            speed: if reduced { 0.0 } else { 2.5 },
                            strands: scaled_strands(1, boost),
                            gap: 0.5,
                            fork_len: scaled(14.0, boost, 2.0, 60.0),
                            intensity: scaled(0.9, boost, 0.0, 3.0),
                            ..ElectricParams::charged()
                        })
                    }
                    .on(shape)
                    .at(t)
                    .over()
                }
                fx_marks::FX_EDGE_HOT | fx_marks::FX_EDGE_COLD
                    if effects == GraphEffects::Electric =>
                {
                    let hot = kind == fx_marks::FX_EDGE_HOT;
                    // `effect_data` holds both endpoints as fractions of the
                    // slot rect; the arc itself is drawn in point space.
                    let a = [x + data[0] * w, y + data[1] * h];
                    let b = [x + data[2] * w, y + data[3] * h];
                    any_anim |= !reduced;
                    Fx::electric(ElectricParams {
                        style: ElectricStyle::Arc { a, b },
                        color: if hot { accent } else { muted },
                        core: accent,
                        amplitude: scaled(
                            if reduced {
                                0.5
                            } else if hot {
                                9.0
                            } else {
                                6.0
                            },
                            boost,
                            0.0,
                            24.0,
                        ),
                        glow: scaled(if hot { 6.0 } else { 4.0 }, boost, 1.0, 20.0),
                        forks: scaled(
                            if reduced {
                                0.0
                            } else if hot {
                                0.6
                            } else {
                                0.4
                            },
                            boost,
                            0.0,
                            1.0,
                        ),
                        flicker: scaled(if reduced { 0.0 } else { 0.5 }, boost, 0.0, 1.0),
                        strands: scaled_strands(3, boost),
                        intensity: scaled(if hot { 1.0 } else { 0.7 }, boost, 0.0, 3.0),
                        speed: if reduced { 0.0 } else { 4.0 },
                        ..ElectricParams::default()
                    })
                    .on(Shape::rounded(x, y, w.max(1.0), h.max(1.0), 1.0))
                    .at(t)
                }
                fx_marks::FX_BURN => {
                    // Subtle on purpose: few embers, short life, low intensity.
                    // `data[0]` is the burn's progress; cool and fade with it.
                    // The layer clock starts at the deletion so embers are born
                    // with it, not whenever the app happened to open.
                    let p = data[0].clamp(0.0, 1.0);
                    let heat = 1.0 - p;
                    let shape = Shape::rounded(x, y, w.max(1.0), h.max(1.0), 9.0);
                    any_anim |= !reduced;
                    Fx::fire(FireParams {
                        style: FireStyle::Embers,
                        intensity: scaled(0.8 * heat, boost, 0.0, 3.0),
                        embers: (5.0 * heat * boost).round() as u32,
                        ember_rise: 26.0,
                        ember_size: 0.45,
                        ember_life: 0.7,
                        height: 16.0,
                        scale: 6.0,
                        base_glow: scaled(0.22 * heat, boost, 0.0, 1.0),
                        occlusion: 0.0,
                        ..FireParams::heat()
                    })
                    .on(shape)
                    .at(p * BurnMark::LIFE as f32)
                    .opacity(heat)
                    .over()
                }
                _ => continue,
            };

            let mut layer = layer.seed(seed);
            layer.reduced_motion = reduced;
            at.push(n as u32);
            layers.push(layer);
            placements.push(FxSlot::default().scale(k).clip(i.clip, i.fade));
        }

        self.animating = !reduced && any_anim;

        let clear = f.rt.clear_color();
        f.ui.prepare(device, queue, &mut f.rt.text.atlas, &f.rt.instances, size);

        if layers.is_empty() {
            f.ui.draw_with_backdrop(f.encoder, f.target, clear, &mut |_, _| {});
            return;
        }

        let fx = self
            .fx
            .get_or_insert_with(|| FxRenderer::new(device, f.format, FxOptions { dither: true }));
        fx.set_srgb_blend(f.srgb_blend());
        fx.add_format(device, TextureFormat::Rgba16Float);
        fx.prepare_with_slots(
            device,
            queue,
            &FxFrame {
                viewport: [size.0, size.1],
                scale: 1.0,
                layers: &layers,
                backdrop: None,
                sources: &[],
            },
            &placements,
        );

        f.ui.draw_with_backdrop(
            f.encoder,
            f.target,
            clear,
            &mut |phase, s: &EffectSite<'_>| {
                let Some(k) = at.iter().position(|n| *n == s.index) else {
                    return;
                };
                match phase {
                    EffectPhase::Backdrop(_) => {}
                    EffectPhase::Draw(pass) => fx.draw_layer_in(pass, s.format, s.origin, k),
                }
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_ui::GraphEffectIntensity;

    #[test]
    fn balanced_leaves_every_knob_exactly_as_it_was() {
        let boost = GraphEffectIntensity::Balanced.scale();
        assert_eq!(scaled(9.0, boost, 0.0, 24.0), 9.0);
        assert_eq!(scaled(0.55, boost, 0.0, 1.0), 0.55);
        assert_eq!(scaled_strands(3, boost), 3);
    }

    #[test]
    fn intensity_scales_shape_and_glow_inside_the_shader_ranges() {
        let subtle = GraphEffectIntensity::Subtle.scale();
        let intense = GraphEffectIntensity::Intense.scale();
        assert!(subtle < 1.0 && intense > 1.0);
        // Subtle quiets the arc: shorter throws, less glow, a fork or two,
        // and a strand count collapsed to one where the base was one.
        assert!(scaled(9.0, subtle, 0.0, 24.0) < 9.0);
        assert!(scaled(6.0, subtle, 1.0, 20.0) < 6.0);
        assert!(scaled(0.5, subtle, 0.0, 1.0) < 0.5);
        assert_eq!(scaled_strands(3, subtle), 2);
        assert_eq!(scaled_strands(1, subtle), 1);
        // Intense grows them, and never past the documented knob ranges.
        assert!(scaled(9.0, intense, 0.0, 24.0) > 9.0);
        assert_eq!(scaled(24.0, intense, 0.0, 24.0), 24.0);
        assert_eq!(scaled(20.0, intense, 1.0, 20.0), 20.0);
        assert_eq!(scaled(1.0, intense, 0.0, 1.0), 1.0);
        assert_eq!(scaled_strands(3, intense), 3);
        assert_eq!(scaled_strands(1, intense), 2);
        // The reduced-motion static arc still follows the choice.
        assert!(scaled(0.7, subtle, 0.0, 24.0) < 0.7);
        assert!(scaled(0.7, intense, 0.0, 24.0) > 0.7);
    }
}
