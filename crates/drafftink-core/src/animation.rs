//! Animation data model for the SeewoClass courseware editor.
//!
//! Defines all enums, configuration structs, easing functions, and the
//! slide-level animation sequence.  This crate is `#[no_std]`-compatible
//! (apart from `std::time::Duration`) and has zero external dependencies
//! beyond `serde` and `uuid`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Special identifiers
// ---------------------------------------------------------------------------

/// Background element ID used for Click triggers that target the slide
/// background rather than a specific element.  Matches the nil GUID that
/// Seewo uses in its `TriggerSource` field.
pub const SLIDE_BACKGROUND_ID: Uuid = Uuid::from_bytes([0; 16]);

// ---------------------------------------------------------------------------
// Categories & triggers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationCategory {
    Enter,
    Exit,
    Emphasis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnimationTrigger {
    Click,
    Before,
    After,
}

// ---------------------------------------------------------------------------
// Easing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum Easing {
    #[default]
    Linear,
    EaseInOut,
    Bounce,
}

/// Apply an easing function to a normalised progress value `t` ∈ [0, 1].
pub fn apply_easing(t: f32, easing: Easing) -> f32 {
    match easing {
        Easing::Linear => t,
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t), // smoothstep
        Easing::Bounce => bounce(t),
    }
}

fn bounce(t: f32) -> f32 {
    // Four parabolic segments simulating a damped bounce.
    const C: f32 = 7.5625;
    const D: f32 = 2.75;
    if t < 1.0 / D {
        C * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        C * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        C * t * t + 0.9375
    } else {
        let t = t - 2.625 / D;
        C * t * t + 0.984375
    }
}

// ---------------------------------------------------------------------------
// Effect type — 38 named variants + Unsupported fallback
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectType {
    // ── Enter (16) ────────────────────────────────────────────────────────
    FadeIn,
    TranslateInTop,
    TranslateInBottom,
    TranslateInLeft,
    TranslateInRight,
    ScaleIn,
    WipeInLeft,
    WipeInRight,
    WipeInTop,
    WipeInBottom,
    FlyInTopLeft,
    FlyInTopRight,
    FlyInBottomLeft,
    FlyInBottomRight,
    SplitInHorizontal,
    SplitInVertical,

    // ── Exit (16) ─────────────────────────────────────────────────────────
    FadeOut,
    TranslateOutTop,
    TranslateOutBottom,
    TranslateOutLeft,
    TranslateOutRight,
    ScaleOut,
    WipeOutLeft,
    WipeOutRight,
    WipeOutTop,
    WipeOutBottom,
    FlyOutTopLeft,
    FlyOutTopRight,
    FlyOutBottomLeft,
    FlyOutBottomRight,
    SplitOutHorizontal,
    SplitOutVertical,

    // ── Emphasis (12) ─────────────────────────────────────────────────────
    Transparency,
    Zoom,
    Heartbeat,
    Shake,
    Wave,
    Spin,
    Pulse,
    Teeter,
    ColorBlend,
    GrowShrink,
    Darken,
    Lighten,

    // ── Fallback ──────────────────────────────────────────────────────────
    /// An effect name that we do not yet implement.  Playback will
    /// instantly snap to the final state and log an info message.
    Unsupported(String),
}

impl EffectType {
    /// Returns `true` when the effect is not yet implemented.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, EffectType::Unsupported(_))
    }
}

// ---------------------------------------------------------------------------
// Direction
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Top,
    Bottom,
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

// ---------------------------------------------------------------------------
// ElementAnimation — per-element animation configuration
// ---------------------------------------------------------------------------

/// Deserialised representation of a single `<AnimationSaveInfo>` node.
///
/// Every field that may be absent in the XML has `#[serde(default)]`.
/// Durations are stored as `std::time::Duration` (ms precision); the
/// parser converts from Seewo's integer-millisecond convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementAnimation {
    pub id: Uuid,

    pub category: AnimationCategory,

    pub trigger: AnimationTrigger,

    pub effect: EffectType,

    #[serde(default)]
    pub easing: Easing,

    /// ID of the canvas element this animation acts upon.
    pub target_element_id: Uuid,

    /// For `Click` triggers, the element that must be clicked to fire
    /// this animation (often `SLIDE_BACKGROUND_ID`).
    #[serde(default)]
    pub trigger_source_id: Option<Uuid>,

    /// Delay before playback starts.
    /// Zero means the default per-trigger delay (Before → 300 ms, After → 600 ms).
    #[serde(default)]
    pub begin_time: Duration,

    /// Total playback duration (excluding `begin_time`).
    pub duration: Duration,

    /// Direction hint for translate / wipe effects.
    #[serde(default)]
    pub orientation: Option<Direction>,

    /// Generic magnitude multiplier (1.0 = normal).
    #[serde(default = "default_magnitude")]
    pub magnitude: f32,

    /// Number of repetitions (0 = play once, >0 = loop N times).
    #[serde(default)]
    pub repeat: u32,

    /// Path to an audio file played alongside the animation.
    /// Relative to the ENBX package root; the parser resolves it to an
    /// absolute path before storing.
    #[serde(default)]
    pub audio_path: Option<String>,

    // ── Position targets (for translate / fly effects) ─────────────────────

    /// Absolute target X position.
    #[serde(default)]
    pub to_x: Option<f32>,

    /// Absolute target Y position.
    #[serde(default)]
    pub to_y: Option<f32>,

    /// Relative X offset from the current position.
    #[serde(default)]
    pub by_x: Option<f32>,

    /// Relative Y offset from the current position.
    #[serde(default)]
    pub by_y: Option<f32>,

    // ── Translate distance ─────────────────────────────────────────────────

    /// Distance for translate animations.
    /// If absent, defaults to 25 % of the logical page width.
    #[serde(default)]
    pub distance: Option<f32>,
}

fn default_magnitude() -> f32 {
    1.0
}

impl ElementAnimation {
    /// Resolve the effective delay for this animation, taking into
    /// account the per-trigger hard-coded defaults from Seewo's C#
    /// source (300 ms for Before, 600 ms for After).
    pub fn effective_delay(&self) -> Duration {
        if self.begin_time > Duration::ZERO {
            return self.begin_time;
        }
        match self.trigger {
            AnimationTrigger::Before => Duration::from_millis(300),
            AnimationTrigger::After => Duration::from_millis(600),
            _ => Duration::ZERO,
        }
    }

    /// Compute the target position delta `[dx, dy]` relative to the
    /// element's current position at animation start.
    ///
    /// Priority: `(to_x, to_y)` > `(by_x, by_y)` > `(0, 0)`.
    pub fn compute_delta(&self, base_pos: [f32; 2]) -> [f32; 2] {
        if let (Some(tx), Some(ty)) = (self.to_x, self.to_y) {
            [tx - base_pos[0], ty - base_pos[1]]
        } else if let (Some(bx), Some(by)) = (self.by_x, self.by_y) {
            [bx, by]
        } else {
            [0.0, 0.0]
        }
    }

    /// Resolve the animation's translate offset in pixels, given the
    /// logical page width (for percentage-based defaults).
    pub fn resolve_distance(&self, page_width: f32) -> f32 {
        self.distance.unwrap_or(page_width * 0.25)
    }
}

// ---------------------------------------------------------------------------
// SlideAnimationSequence — global play order for one slide
// ---------------------------------------------------------------------------

/// Maps the slide-level `<AnimationOrders>` to three queues, mirroring
/// Seewo's `SlideAnimationPlayer` + `SlideConditionAnimationPlayer`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlideAnimationSequence {
    /// Animations that fire automatically 300 ms after the slide opens.
    #[serde(default)]
    pub before_queue: Vec<Uuid>,

    /// Animations that fire automatically 600 ms after `before_queue`
    /// finishes.
    #[serde(default)]
    pub after_queue: Vec<Uuid>,

    /// Animations awaiting a Click trigger.  Key = trigger_source_id
    /// (often `SLIDE_BACKGROUND_ID`).
    #[serde(default)]
    pub click_map: HashMap<Uuid, Vec<Uuid>>,
}

impl SlideAnimationSequence {
    /// Returns `true` when there are no animations on this slide.
    pub fn is_empty(&self) -> bool {
        self.before_queue.is_empty()
            && self.after_queue.is_empty()
            && self.click_map.is_empty()
    }

    /// Collects all `target_element_id`s from the `before_queue` and
    /// `after_queue`.  Used during `init_page` to pre-hide elements
    /// before their entrance animation plays.
    pub fn all_target_ids(
        &self,
        anim_map: &HashMap<Uuid, ElementAnimation>,
    ) -> Vec<Uuid> {
        let mut ids = Vec::new();
        for q in [&self.before_queue, &self.after_queue] {
            for id in q {
                if let Some(a) = anim_map.get(id) {
                    ids.push(a.target_element_id);
                }
            }
        }
        ids
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_linear_endpoints() {
        assert!((apply_easing(0.0, Easing::Linear) - 0.0).abs() < 0.001);
        assert!((apply_easing(1.0, Easing::Linear) - 1.0).abs() < 0.001);
    }

    #[test]
    fn easing_smoothstep_endpoints() {
        assert!((apply_easing(0.0, Easing::EaseInOut) - 0.0).abs() < 0.001);
        assert!((apply_easing(1.0, Easing::EaseInOut) - 1.0).abs() < 0.001);
    }

    #[test]
    fn easing_smoothstep_symmetry() {
        let a = apply_easing(0.3, Easing::EaseInOut);
        let b = 1.0 - apply_easing(0.7, Easing::EaseInOut);
        assert!((a - b).abs() < 0.001);
    }

    #[test]
    fn effective_delay_before_default() {
        let anim = ElementAnimation {
            id: Uuid::new_v4(),
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::Before,
            effect: EffectType::FadeIn,
            target_element_id: Uuid::new_v4(),
            easing: Easing::default(),
            begin_time: Duration::ZERO,
            duration: Duration::from_millis(500),
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
        };
        assert_eq!(anim.effective_delay(), Duration::from_millis(300));
    }

    #[test]
    fn effective_delay_after_default() {
        let mut anim = ElementAnimation {
            id: Uuid::new_v4(),
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::After,
            effect: EffectType::FadeIn,
            target_element_id: Uuid::new_v4(),
            easing: Easing::default(),
            begin_time: Duration::ZERO,
            duration: Duration::from_millis(500),
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
        };
        anim.trigger = AnimationTrigger::After;
        assert_eq!(anim.effective_delay(), Duration::from_millis(600));
    }

    #[test]
    fn effective_delay_explicit_overrides_default() {
        let anim = ElementAnimation {
            id: Uuid::new_v4(),
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::Before,
            effect: EffectType::FadeIn,
            target_element_id: Uuid::new_v4(),
            easing: Easing::default(),
            begin_time: Duration::from_millis(1200),
            duration: Duration::from_millis(500),
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
        };
        assert_eq!(anim.effective_delay(), Duration::from_millis(1200));
    }

    #[test]
    fn compute_delta_to_xy() {
        let anim = ElementAnimation {
            id: Uuid::new_v4(),
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::Before,
            effect: EffectType::FadeIn,
            target_element_id: Uuid::new_v4(),
            easing: Easing::default(),
            begin_time: Duration::ZERO,
            duration: Duration::from_millis(500),
            orientation: None,
            magnitude: 1.0,
            repeat: 0,
            audio_path: None,
            trigger_source_id: None,
            to_x: Some(300.0),
            to_y: Some(200.0),
            by_x: None,
            by_y: None,
            distance: None,
        };
        assert_eq!(anim.compute_delta([100.0, 150.0]), [200.0, 50.0]);
    }

    #[test]
    fn compute_delta_by_xy() {
        let anim = ElementAnimation {
            id: Uuid::new_v4(),
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::Before,
            effect: EffectType::FadeIn,
            target_element_id: Uuid::new_v4(),
            easing: Easing::default(),
            begin_time: Duration::ZERO,
            duration: Duration::from_millis(500),
            orientation: None,
            magnitude: 1.0,
            repeat: 0,
            audio_path: None,
            trigger_source_id: None,
            to_x: None,
            to_y: None,
            by_x: Some(50.0),
            by_y: Some(-30.0),
            distance: None,
        };
        assert_eq!(anim.compute_delta([100.0, 150.0]), [50.0, -30.0]);
    }

    #[test]
    fn compute_delta_none() {
        let anim = ElementAnimation {
            id: Uuid::new_v4(),
            category: AnimationCategory::Enter,
            trigger: AnimationTrigger::Before,
            effect: EffectType::FadeIn,
            target_element_id: Uuid::new_v4(),
            easing: Easing::default(),
            begin_time: Duration::ZERO,
            duration: Duration::from_millis(500),
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
        };
        assert_eq!(anim.compute_delta([100.0, 150.0]), [0.0, 0.0]);
    }

    #[test]
    fn sequence_is_empty() {
        let seq = SlideAnimationSequence::default();
        assert!(seq.is_empty());
    }

    #[test]
    fn sequence_all_target_ids() {
        let target1 = Uuid::new_v4();
        let target2 = Uuid::new_v4();
        let mut map = HashMap::new();
        let aid1 = Uuid::new_v4();
        let aid2 = Uuid::new_v4();
        map.insert(
            aid1,
            ElementAnimation {
                id: aid1,
                target_element_id: target1,
                category: AnimationCategory::Enter,
                trigger: AnimationTrigger::Before,
                effect: EffectType::FadeIn,
                easing: Easing::default(),
                begin_time: Duration::ZERO,
                duration: Duration::from_millis(500),
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
        map.insert(
            aid2,
            ElementAnimation {
                id: aid2,
                target_element_id: target2,
                category: AnimationCategory::Enter,
                trigger: AnimationTrigger::After,
                effect: EffectType::FadeIn,
                easing: Easing::default(),
                begin_time: Duration::ZERO,
                duration: Duration::from_millis(500),
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
        let seq = SlideAnimationSequence {
            before_queue: vec![aid1],
            after_queue: vec![aid2],
            click_map: HashMap::new(),
        };
        let ids = seq.all_target_ids(&map);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&target1));
        assert!(ids.contains(&target2));
    }
}
