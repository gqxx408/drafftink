//! Animation playback engine — single Timeline scheduler replacing Seewo's
//! `SlideAnimationPlayer` + `SlideConditionAnimationPlayer`.
//!
//! State machine:  WaitingBefore → PlayingBefore → WaitingAfter → PlayingAfter → WaitingClick → Done
//!
//! Key invariants (from C# alignment):
//! 1. Before animations get a 300 ms hard-coded delay (unless begin_time is set).
//! 2. After  animations get a 600 ms hard-coded delay.
//! 3. Elements targeted by Before/After are hidden (opacity = 0) until their animation starts.
//! 4. Base values are captured at the instant `start_one` is called (FromCurrentState).
//! 5. ActiveAnimation stores delta from base → target; update is one mul+add per attribute.
//! 6. Audio sinks are per-animation and cleaned up by `retain`; no global sink.

use drafftink_core::animation::{
    apply_easing, AnimationCategory, Direction, EffectType, ElementAnimation,
    SlideAnimationSequence, SLIDE_BACKGROUND_ID,
};
use drafftink_core::model::Element;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

pub struct AnimationPlayer {
    /// All animation configurations for the current slide (id → config).
    anim_map: HashMap<Uuid, ElementAnimation>,

    /// Play-order queues + click mapping for the current slide.
    sequence: SlideAnimationSequence,

    /// Animations currently being played.
    active: Vec<ActiveAnimation>,

    /// State machine.
    state: PlayerState,

    /// Logical page size [w, h] (from Board.xml), used for translate offsets.
    page_size: [f32; 2],

    /// Time the current state was entered.
    state_start: Instant,

    // ── Audio ──────────────────────────────────────────────────────────────
    /// Audio output stream (held for lifetime).
    _stream: Option<rodio::OutputStream>,
    stream_handle: Option<rodio::OutputStreamHandle>,
    /// Pending sinks; retained every frame — a sink is dropped automatically
    /// when it becomes empty (Rust Drop = stop).
    pending_sinks: Vec<rodio::Sink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlayerState {
    Idle,
    WaitingBefore,
    PlayingBefore,
    WaitingAfter,
    PlayingAfter,
    WaitingClick,
    Done,
}

struct ActiveAnimation {
    #[allow(dead_code)] // used in debug_ui (debug_assertions only)
    anim_id: Uuid,
    target_element_id: Uuid,
    start_time: Instant,

    base_opacity: f32,
    base_pos: [f32; 2],
    base_size: [f32; 2],

    delta_opacity: f32,
    delta_pos: [f32; 2],
    delta_size: [f32; 2],

    easing: drafftink_core::animation::Easing,
    duration: Duration,
    repeat: u32,
    done: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

impl AnimationPlayer {
    pub fn new() -> Self {
        let (stream, handle) = rodio::OutputStream::try_default()
            .map(|(s, h)| (Some(s), Some(h)))
            .unwrap_or((None, None));

        Self {
            anim_map: HashMap::new(),
            sequence: SlideAnimationSequence::default(),
            active: Vec::new(),
            state: PlayerState::Idle,
            page_size: [1920.0, 1080.0],
            state_start: Instant::now(),
            _stream: stream,
            stream_handle: handle,
            pending_sinks: Vec::new(),
        }
    }

    /// Initialise the player for a new slide.  Stops all running animations
    /// and pre-hides target elements (HiddenOnStart).
    pub fn init_page(
        &mut self,
        sequence: SlideAnimationSequence,
        anim_map: HashMap<Uuid, ElementAnimation>,
        page_size: [f32; 2],
        elements: &mut [Element],
    ) {
        self.shutdown();
        self.page_size = page_size;

        if sequence.is_empty() {
            self.state = PlayerState::Done;
            return;
        }

        self.anim_map = anim_map;
        self.sequence = sequence;

        // ── HiddenOnStart ──────────────────────────────────────────────────
        for target_id in self.sequence.all_target_ids(&self.anim_map) {
            if let Some(elem) = elements.iter_mut().find(|e| e.id() == target_id) {
                elem.base_mut().opacity = 0.0;
            }
        }

        // ── Enter state ────────────────────────────────────────────────────
        self.transition(if self.sequence.before_queue.is_empty() {
            PlayerState::WaitingAfter
        } else {
            PlayerState::WaitingBefore
        });
    }

    /// Handle a click anywhere on the canvas.
    pub fn on_canvas_click(&mut self, world_pos: [f32; 2], elements: &mut [Element]) {
        if self.state != PlayerState::WaitingClick {
            return;
        }

        // Check hit-tested element first
        let mut triggered = false;
        for elem in elements.iter() {
            if elem.base().hit_test(world_pos) {
                let id = elem.id();
                if let Some(anim_ids) = self.sequence.click_map.get(&id).cloned() {
                    self.start_animations(&anim_ids, elements);
                    triggered = true;
                    break;
                }
            }
        }

        // Global background click
        if !triggered {
            if let Some(anim_ids) = self.sequence.click_map.get(&SLIDE_BACKGROUND_ID).cloned() {
                self.start_animations(&anim_ids, elements);
            }
        }
    }

    /// Advance all active animations by one frame.  Call once per render frame.
    pub fn update(&mut self, now: Instant, elements: &mut [Element]) {
        self.pending_sinks.retain(|s| !s.empty());

        match self.state {
            PlayerState::Idle | PlayerState::Done => (),

            PlayerState::WaitingBefore => {
                if now.duration_since(self.state_start).as_millis() >= 300 {
                    self.transition(PlayerState::PlayingBefore);
                    self.start_animations(&self.sequence.before_queue.clone(), elements);
                }
            }

            PlayerState::PlayingBefore => {
                self.tick_active(now, elements);
                if self.active.is_empty() {
                    if self.sequence.after_queue.is_empty() {
                        self.transition(if self.sequence.click_map.is_empty() {
                            PlayerState::Done
                        } else {
                            PlayerState::WaitingClick
                        });
                    } else {
                        self.transition(PlayerState::WaitingAfter);
                    }
                }
            }

            PlayerState::WaitingAfter => {
                if now.duration_since(self.state_start).as_millis() >= 600 {
                    self.transition(PlayerState::PlayingAfter);
                    self.start_animations(&self.sequence.after_queue.clone(), elements);
                }
            }

            PlayerState::PlayingAfter => {
                self.tick_active(now, elements);
                if self.active.is_empty() {
                    self.transition(if self.sequence.click_map.is_empty() {
                        PlayerState::Done
                    } else {
                        PlayerState::WaitingClick
                    });
                }
            }

            PlayerState::WaitingClick => {
                // nothing — waiting for on_canvas_click
            }
        }
    }

    /// Returns `true` when the player still has work to do.
    pub fn is_active(&self) -> bool {
        !matches!(self.state, PlayerState::Idle | PlayerState::Done) || !self.active.is_empty()
    }

    /// Stop everything, release audio.  Called on slide change / plugin unload.
    pub fn shutdown(&mut self) {
        self.active.clear();
        self.pending_sinks.clear();
        self.anim_map.clear();
        self.sequence = SlideAnimationSequence::default();
        self.state = PlayerState::Idle;
    }

    // ------------------------------------------------------------------
    // Debug UI (debug_assertions only)
    // ------------------------------------------------------------------

    #[cfg(debug_assertions)]
    pub fn debug_ui(&self, ctx: &egui::Context) {
        egui::Window::new("Animation Debug")
            .anchor(egui::Align2::RIGHT_TOP, [0.0, 0.0])
            .default_open(true)
            .show(ctx, |ui| {
                let elapsed = Instant::now()
                    .duration_since(self.state_start)
                    .as_secs_f32();
                ui.label(format!("State: {:?} ({elapsed:.2}s)", self.state));
                ui.label(format!("Active: {}", self.active.len()));
                for a in &self.active {
                    let id_str = a.anim_id.to_string();
                    let short = &id_str[..id_str.len().min(4)];
                    let e = Instant::now().duration_since(a.start_time).as_secs_f32();
                    let pct = (e / a.duration.as_secs_f32() * 100.0).min(100.0);
                    ui.label(format!("  {short} — {pct:3.0}%"));
                }
            });
    }
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

impl AnimationPlayer {
    fn transition(&mut self, new_state: PlayerState) {
        if self.state != new_state {
            log::info!(
                "Player: {:?} → {:?} (elapsed={:.0}ms)",
                self.state,
                new_state,
                Instant::now().duration_since(self.state_start).as_millis()
            );
        }
        self.state = new_state;
        self.state_start = Instant::now();
    }

    fn start_animations(&mut self, anim_ids: &[Uuid], elements: &mut [Element]) {
        for &id in anim_ids {
            self.start_one(id, elements);
        }
    }

    fn start_one(&mut self, anim_id: Uuid, elements: &mut [Element]) {
        let anim = match self.anim_map.get(&anim_id) {
            Some(a) => a.clone(),
            None => return,
        };

        // ── Fast path: Unsupported / zero-duration ──────────────────────────
        if anim.effect.is_unsupported() || anim.duration.is_zero() {
            log::info!(
                "Unsupported animation effect: {:?}, falling back to instant cut.",
                anim.effect
            );
            if let Some(elem) = elements
                .iter_mut()
                .find(|e| e.id() == anim.target_element_id)
            {
                let base = elem.base_mut();
                base.opacity = 1.0;
                // Apply position delta if any
                let d = anim.compute_delta(base.position);
                base.position[0] += d[0];
                base.position[1] += d[1];
            }
            return;
        }

        // ── Find element & capture base values (FromCurrentState) ──────────
        let (base_opacity, base_pos, base_size) = match elements
            .iter_mut()
            .find(|e| e.id() == anim.target_element_id)
        {
            Some(e) => {
                let b = e.base_mut();
                (b.opacity, b.position, b.size)
            }
            None => return,
        };

        // ── Compute deltas from effect type ──────────────────────────────────
        let (delta_opacity, delta_pos, delta_size) =
            compute_effect_deltas(&anim, base_opacity, base_pos, base_size, self.page_size[0]);

        self.active.push(ActiveAnimation {
            anim_id,
            target_element_id: anim.target_element_id,
            start_time: Instant::now(),
            base_opacity,
            base_pos,
            base_size,
            delta_opacity,
            delta_pos,
            delta_size,
            easing: anim.easing,
            duration: anim.duration,
            repeat: anim.repeat,
            done: false,
        });

        // ── Audio ──────────────────────────────────────────────────────────
        if let Some(ref path) = anim.audio_path {
            if let Some(handle) = &self.stream_handle {
                if let Ok(file) = std::fs::File::open(path) {
                    let reader = std::io::BufReader::new(file);
                    if let Ok(decoder) = rodio::Decoder::new(reader) {
                        if let Ok(sink) = rodio::Sink::try_new(handle) {
                            sink.append(decoder);
                            self.pending_sinks.push(sink);
                        }
                    }
                }
            }
        }
    }

    fn tick_active(&mut self, now: Instant, elements: &mut [Element]) {
        for active in &mut self.active {
            let elapsed = now.duration_since(active.start_time);
            let raw_t = elapsed.as_secs_f32() / active.duration.as_secs_f32();
            let progress = if active.repeat > 0 {
                raw_t % 1.0
            } else {
                raw_t.clamp(0.0, 1.0)
            };
            let eased = apply_easing(progress, active.easing);

            if let Some(elem) = elements
                .iter_mut()
                .find(|e| e.id() == active.target_element_id)
            {
                let b = elem.base_mut();
                b.opacity = (active.base_opacity + active.delta_opacity * eased).clamp(0.0, 1.0);
                b.position[0] = active.base_pos[0] + active.delta_pos[0] * eased;
                b.position[1] = active.base_pos[1] + active.delta_pos[1] * eased;
                b.size[0] = active.base_size[0] + active.delta_size[0] * eased;
                b.size[1] = active.base_size[1] + active.delta_size[1] * eased;
            }

            active.done = raw_t >= 1.0 && active.repeat == 0;
        }
        self.active.retain(|a| !a.done);
    }
}

impl Default for AnimationPlayer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Effect deltas — maps EffectType → (delta_opacity, delta_pos, delta_size)
// ---------------------------------------------------------------------------

/// Compute the three delta tuples that drive one animation from its current
/// state to the target state defined by the `ElementAnimation` config.
///
/// Each attribute updates as `base + delta * eased` inside `tick_active`.
fn compute_effect_deltas(
    anim: &ElementAnimation,
    base_opacity: f32,
    base_pos: [f32; 2],
    base_size: [f32; 2],
    page_width: f32,
) -> (f32, [f32; 2], [f32; 2]) {
    let mag = anim.magnitude;
    let dist = anim.resolve_distance(page_width);

    let (delta_opacity, delta_pos, delta_size) = match anim.effect {
        // ── Fade ────────────────────────────────────────────────────────
        EffectType::FadeIn => (1.0 - base_opacity, [0.0, 0.0], [0.0, 0.0]),
        EffectType::FadeOut => (-base_opacity, [0.0, 0.0], [0.0, 0.0]),

        // ── Translate In ────────────────────────────────────────────────
        EffectType::TranslateInTop => (
            1.0 - base_opacity,
            pos_delta(Direction::Top, dist, true),
            [0.0, 0.0],
        ),
        EffectType::TranslateInBottom => (
            1.0 - base_opacity,
            pos_delta(Direction::Bottom, dist, true),
            [0.0, 0.0],
        ),
        EffectType::TranslateInLeft => (
            1.0 - base_opacity,
            pos_delta(Direction::Left, dist, true),
            [0.0, 0.0],
        ),
        EffectType::TranslateInRight => (
            1.0 - base_opacity,
            pos_delta(Direction::Right, dist, true),
            [0.0, 0.0],
        ),

        // ── Translate Out ───────────────────────────────────────────────
        EffectType::TranslateOutTop => (
            -base_opacity,
            pos_delta(Direction::Top, dist, false),
            [0.0, 0.0],
        ),
        EffectType::TranslateOutBottom => (
            -base_opacity,
            pos_delta(Direction::Bottom, dist, false),
            [0.0, 0.0],
        ),
        EffectType::TranslateOutLeft => (
            -base_opacity,
            pos_delta(Direction::Left, dist, false),
            [0.0, 0.0],
        ),
        EffectType::TranslateOutRight => (
            -base_opacity,
            pos_delta(Direction::Right, dist, false),
            [0.0, 0.0],
        ),

        // ── Scale ───────────────────────────────────────────────────────
        EffectType::ScaleIn => {
            let dw = base_size[0] * mag;
            let dh = base_size[1] * mag;
            (1.0 - base_opacity, [0.0, 0.0], [dw, dh])
        }
        EffectType::ScaleOut => (
            0.0,
            anim.compute_delta(base_pos),
            [-base_size[0], -base_size[1]],
        ),

        // ── Zoom (emphasis — grows then shrinks back) ───────────────────
        EffectType::Zoom => {
            let dw = base_size[0] * 0.3 * mag;
            let dh = base_size[1] * 0.3 * mag;
            (0.0, [0.0, 0.0], [dw, dh])
        }

        // ── Transparency (emphasis — opacity pulse) ─────────────────────
        EffectType::Transparency => (0.7 * mag, [0.0, 0.0], [0.0, 0.0]),

        // ── Default (use category-based opacity, explicit ToX/ToY) ──────
        _ => {
            let op = match anim.category {
                AnimationCategory::Enter => 1.0 - base_opacity,
                AnimationCategory::Exit => -base_opacity,
                AnimationCategory::Emphasis => 0.0,
            };
            (op, anim.compute_delta(base_pos), [0.0, 0.0])
        }
    };

    (delta_opacity, delta_pos, delta_size)
}

/// Position delta for translate effects.
/// `inward`: true = move toward rest position; false = move away.
fn pos_delta(dir: Direction, dist: f32, inward: bool) -> [f32; 2] {
    let (dx, dy) = match dir {
        Direction::Top => (0.0, -dist),
        Direction::Bottom => (0.0, dist),
        Direction::Left => (-dist, 0.0),
        Direction::Right => (dist, 0.0),
        Direction::TopLeft => (-dist, -dist),
        Direction::TopRight => (dist, -dist),
        Direction::BottomLeft => (-dist, dist),
        Direction::BottomRight => (dist, dist),
    };
    if inward {
        // Element starts at base_pos + (dx, dy) during tick_active,
        // and moves to base_pos.  So delta = -(dx, dy) since we
        // compute base + delta * eased.
        [-dx, -dy]
    } else {
        // Outward: element starts at base_pos and moves to base_pos + (dx, dy).
        [dx, dy]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use drafftink_core::animation::{AnimationTrigger, Easing, EffectType, ElementAnimation};

    fn make_text_element(id: Uuid, opacity: f32, pos: [f32; 2], size: [f32; 2]) -> Element {
        use drafftink_core::model::{BaseElement, TextElement};
        Element::Text(TextElement {
            base: BaseElement {
                id,
                position: pos,
                size,
                opacity,
                ..Default::default()
            },
            text: String::new(),
            font_size: 24.0,
            font_family: String::new(),
        })
    }

    fn make_fade_in_anim(id: Uuid, target: Uuid) -> ElementAnimation {
        ElementAnimation {
            id,
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::Before,
            effect: EffectType::FadeIn,
            easing: Easing::Linear,
            target_element_id: target,
            duration: Duration::from_millis(500),
            begin_time: Duration::ZERO,
            orientation: None,
            magnitude: 1.0,
            repeat: 0,
            audio_path: None,
            trigger_source_id: None,
            to_x: None,
            to_y: None,
            by_x: None,
            by_y: None,
            distance: None,
        }
    }

    #[test]
    fn fadein_sets_opacity_to_one() {
        let target_id = Uuid::new_v4();
        let anim_id = Uuid::new_v4();
        let mut player = AnimationPlayer::new();
        let mut elements = vec![make_text_element(target_id, 0.0, [0.0, 0.0], [100.0, 50.0])];

        let mut map = HashMap::new();
        map.insert(anim_id, make_fade_in_anim(anim_id, target_id));
        player.anim_map = map;

        player.start_one(anim_id, &mut elements);
        assert_eq!(player.active.len(), 1);
        assert_eq!(player.active[0].base_opacity, 0.0);
        assert!((player.active[0].delta_opacity - 1.0).abs() < 0.001);

        // Fast forward past end
        let start = Instant::now();
        let future = start + Duration::from_millis(600);
        player.tick_active(future, &mut elements);
        assert!(player.active.is_empty());
        assert!((elements[0].base().opacity - 1.0).abs() < 0.001);
    }

    #[test]
    fn unsupported_effect_instant_cut() {
        let target_id = Uuid::new_v4();
        let anim_id = Uuid::new_v4();
        let mut player = AnimationPlayer::new();
        let mut elements = vec![make_text_element(
            target_id,
            0.5,
            [100.0, 200.0],
            [50.0, 50.0],
        )];

        let mut map = HashMap::new();
        map.insert(
            anim_id,
            ElementAnimation {
                id: anim_id,
                effect: EffectType::Unsupported("CrazySpin".into()),
                target_element_id: target_id,
                duration: Duration::from_millis(500),
                category: AnimationCategory::Enter,
                trigger: AnimationTrigger::Before,
                easing: Easing::Linear,
                begin_time: Duration::ZERO,
                orientation: None,
                magnitude: 1.0,
                repeat: 0,
                audio_path: None,
                trigger_source_id: None,
                to_x: None,
                to_y: None,
                by_x: None,
                by_y: None,
                distance: None,
            },
        );
        player.anim_map = map;

        player.start_one(anim_id, &mut elements);
        // Should be instantly cut — not in active queue
        assert!(player.active.is_empty());
        assert!((elements[0].base().opacity - 1.0).abs() < 0.001);
    }

    #[test]
    fn state_transition_before_to_after() {
        let mut player = AnimationPlayer::new();
        let target_id = Uuid::new_v4();
        let before_id = Uuid::new_v4();
        let after_id = Uuid::new_v4();
        let mut elements = vec![make_text_element(target_id, 1.0, [0.0, 0.0], [100.0, 50.0])];

        let mut map = HashMap::new();
        map.insert(before_id, make_fade_in_anim(before_id, target_id));
        map.insert(after_id, {
            let mut a = make_fade_in_anim(after_id, target_id);
            a.trigger = AnimationTrigger::After;
            a
        });

        player.init_page(
            SlideAnimationSequence {
                before_queue: vec![before_id],
                after_queue: vec![after_id],
                ..Default::default()
            },
            map,
            [1280.0, 720.0],
            &mut elements,
        );

        assert_eq!(player.state, PlayerState::WaitingBefore);
        // Element should be hidden by HiddenOnStart
        assert!((elements[0].base().opacity).abs() < 0.001);
    }

    #[test]
    fn hidden_on_start_resets_opacity() {
        let mut player = AnimationPlayer::new();
        let target_id = Uuid::new_v4();
        let anim_id = Uuid::new_v4();
        let mut elements = vec![make_text_element(target_id, 0.8, [0.0, 0.0], [100.0, 50.0])];

        let mut map = HashMap::new();
        map.insert(anim_id, make_fade_in_anim(anim_id, target_id));
        player.init_page(
            SlideAnimationSequence {
                before_queue: vec![anim_id],
                ..Default::default()
            },
            map,
            [1280.0, 720.0],
            &mut elements,
        );

        assert!(player.is_active());
        assert_eq!(player.state, PlayerState::WaitingBefore);
        // HiddenOnStart should override existing opacity
        assert!((elements[0].base().opacity).abs() < 0.001);
    }

    #[test]
    fn shutdown_clears_all() {
        let target_id = Uuid::new_v4();
        let anim_id = Uuid::new_v4();
        let mut player = AnimationPlayer::new();
        let mut elements = vec![make_text_element(target_id, 1.0, [0.0, 0.0], [100.0, 50.0])];

        let mut map = HashMap::new();
        map.insert(anim_id, make_fade_in_anim(anim_id, target_id));
        player.init_page(
            SlideAnimationSequence {
                before_queue: vec![anim_id],
                ..Default::default()
            },
            map,
            [1280.0, 720.0],
            &mut elements,
        );

        player.shutdown();
        assert!(!player.is_active());
        assert_eq!(player.state, PlayerState::Idle);
        assert!(player.active.is_empty());
        assert!(player.anim_map.is_empty());
    }

    // ── Effect delta tests ───────────────────────────────────────────────

    #[test]
    fn translate_in_top_delta() {
        let anim = make_fade_in_anim(Uuid::new_v4(), Uuid::new_v4());
        let anim = ElementAnimation {
            effect: EffectType::TranslateInTop,
            orientation: Some(Direction::Top),
            distance: Some(300.0),
            ..anim
        };
        let (do_, dp, _ds) = compute_effect_deltas(&anim, 0.0, [100.0, 200.0], [50.0, 50.0], 1280.0);
        assert!((do_ - 1.0).abs() < 0.001); // fade in
        assert!((dp[0] - 0.0).abs() < 0.001);
        assert!((dp[1] - 300.0).abs() < 0.001); // -(-300) = 300
    }

    #[test]
    fn scale_in_delta() {
        let anim = ElementAnimation {
            effect: EffectType::ScaleIn,
            magnitude: 1.0,
            ..make_fade_in_anim(Uuid::new_v4(), Uuid::new_v4())
        };
        let (do_, _dp, ds) = compute_effect_deltas(&anim, 0.0, [0.0, 0.0], [100.0, 50.0], 1280.0);
        assert!((do_ - 1.0).abs() < 0.001);
        assert!((ds[0] - 100.0).abs() < 0.001);
        assert!((ds[1] - 50.0).abs() < 0.001);
    }

    #[test]
    fn zoom_delta() {
        let anim = ElementAnimation {
            effect: EffectType::Zoom,
            magnitude: 1.0,
            ..make_fade_in_anim(Uuid::new_v4(), Uuid::new_v4())
        };
        let (do_, _dp, ds) = compute_effect_deltas(&anim, 1.0, [0.0, 0.0], [200.0, 100.0], 1280.0);
        assert!((do_ - 0.0).abs() < 0.001); // no opacity change
        assert!((ds[0] - 60.0).abs() < 0.001); // 200 * 0.3
    }

    #[test]
    fn transparency_delta() {
        let anim = ElementAnimation {
            effect: EffectType::Transparency,
            magnitude: 1.0,
            ..make_fade_in_anim(Uuid::new_v4(), Uuid::new_v4())
        };
        let (do_, _dp, _ds) = compute_effect_deltas(&anim, 0.3, [0.0, 0.0], [50.0, 50.0], 1280.0);
        assert!((do_ - 0.7).abs() < 0.001); // 0.7 pulse
    }

    #[test]
    fn default_distance_from_page_width() {
        let anim = ElementAnimation {
            effect: EffectType::TranslateInLeft,
            distance: None,
            ..make_fade_in_anim(Uuid::new_v4(), Uuid::new_v4())
        };
        let (_do, dp, _ds) = compute_effect_deltas(&anim, 0.0, [0.0, 0.0], [50.0, 50.0], 1280.0);
        // default: 1280 * 0.25 = 320
        assert!((dp[0] - 320.0).abs() < 0.001);
    }
}
