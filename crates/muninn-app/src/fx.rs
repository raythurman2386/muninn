//! Graph FX compositor: paints Rune `El::effect` slots as rune-fx layers.
//!
//! Marks are minted by `muninn-ui` graph view (`fx_marks`). Classic and Off
//! presets emit no slots, so this compositor is a no-op then.
//!
//! Electric is a **Muninn-composed** treatment (edge arcs + selection /
//! neighbour borders), not a Rune packaged paired preset.

use muninn_ui::{GraphEffects, State, fx_marks};
use rune::compose::{Compositor, Frame};
use rune::render::{EffectPhase, EffectSite, KIND_EFFECT};
use rune_fx::{
    ElectricParams, ElectricStyle, Fx, FxFrame, FxLayer, FxOptions, FxRenderer, FxSlot, Place,
    Shape, rgb,
};

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
        let reduced = f.rt.state.reduced_motion;
        let effects = f.rt.state.graph_effects;
        let accent = f.rt.state.fx_accent.get();
        let muted = f.rt.state.fx_muted.get();
        let accent_rgb = rgb(pack_hex(accent));
        let muted_rgb = rgb(pack_hex(muted));

        let marks = &f.rt.marks;
        let mut at: Vec<u32> = Vec::new();
        let mut layers: Vec<FxLayer> = Vec::new();
        let mut placements: Vec<FxSlot> = Vec::new();
        let mut any_anim = false;

        if effects == GraphEffects::Electric {
            for (n, i) in f.rt.instances.iter().enumerate() {
                if i.params[1] != KIND_EFFECT {
                    continue;
                }
                let mark = marks.get(n).copied().unwrap_or(0);
                let k = i.params[3];
                if k <= 0.0 {
                    continue;
                }
                let [x, y, w, h] = i.rect.map(|v| v / k);
                let r = i.radii.iter().fold(0.0f32, |a, v| a.max(v.abs())) / k;
                let shape = Shape::rounded(x, y, w.max(1.0), h.max(1.0), r.max(8.0));
                let data = i.uv;
                let kind = mark & 0xffff_0000;
                let layer = if kind == fx_marks::FX_NODE_SEL {
                    any_anim |= !reduced;
                    Fx::electric(ElectricParams {
                        style: ElectricStyle::Border,
                        color: accent_rgb,
                        core: accent_rgb,
                        amplitude: if reduced { 1.2 } else { 3.2 },
                        glow: 5.5,
                        forks: if reduced { 0.0 } else { 0.35 },
                        flicker: if reduced { 0.0 } else { 0.2 },
                        speed: if reduced { 0.0 } else { 2.2 },
                        strands: 1,
                        intensity: 1.05,
                        ..ElectricParams::default()
                    })
                    .on(shape)
                    .over()
                } else if kind == fx_marks::FX_NODE_NEAR {
                    any_anim |= !reduced;
                    Fx::electric(ElectricParams {
                        style: ElectricStyle::Charged,
                        color: muted_rgb,
                        core: accent_rgb,
                        amplitude: if reduced { 0.6 } else { 1.0 },
                        glow: 3.2,
                        forks: if reduced { 0.0 } else { 0.15 },
                        flicker: 0.0,
                        speed: if reduced { 0.0 } else { 1.4 },
                        strands: 1,
                        intensity: 0.75,
                        ..ElectricParams::charged()
                    })
                    .on(shape)
                } else if kind == fx_marks::FX_EDGE_HOT || kind == fx_marks::FX_EDGE_COLD {
                    any_anim |= !reduced;
                    let hot = kind == fx_marks::FX_EDGE_HOT;
                    let a = [x + data[0] * w, y + data[1] * h];
                    let b = [x + data[2] * w, y + data[3] * h];
                    let color = if hot { accent_rgb } else { muted_rgb };
                    Fx::electric(ElectricParams {
                        style: ElectricStyle::Arc { a, b },
                        color,
                        core: accent_rgb,
                        amplitude: if reduced {
                            0.8
                        } else if hot {
                            4.5
                        } else {
                            2.8
                        },
                        glow: if hot { 5.0 } else { 3.5 },
                        forks: if reduced {
                            0.0
                        } else if hot {
                            0.45
                        } else {
                            0.25
                        },
                        flicker: if reduced { 0.0 } else { 0.25 },
                        strands: 1,
                        intensity: if hot { 1.0 } else { 0.7 },
                        speed: if reduced { 0.0 } else { 2.4 },
                        ..ElectricParams::default()
                    })
                    .on(shape)
                } else {
                    continue;
                };
                let mut layer = layer.at(t).seed((mark & 0xffff) + 1);
                layer.reduced_motion = reduced;
                if kind == fx_marks::FX_NODE_SEL || kind == fx_marks::FX_NODE_NEAR {
                    layer.place = Place::Over;
                }
                at.push(n as u32);
                layers.push(layer);
                placements.push(FxSlot::default().scale(k).clip(i.clip, i.fade));
            }
        }

        self.animating = any_anim && effects == GraphEffects::Electric && !reduced;

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
        fx.add_format(device, wgpu::TextureFormat::Rgba16Float);
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

fn pack_hex(rgb: [f32; 3]) -> u32 {
    let enc = |c: f32| -> u32 {
        let x = c.clamp(0.0, 1.0);
        (x * 255.0).round() as u32
    };
    (enc(rgb[0]) << 16) | (enc(rgb[1]) << 8) | enc(rgb[2])
}
