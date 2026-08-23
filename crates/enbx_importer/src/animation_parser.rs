//! ENBX animation parser — extracts `<AnimationSaveInfo>` and
//! `<AnimationOrders>` from a `Slide_*.xml` string and converts them
//! to `ElementAnimation` + `SlideAnimationSequence`.
//!
//! Field mapping is based on dotPeek output of Seewo's C# `AnimationSaveInfo` class.
//! All missing / malformed fields default silently.

use drafftink_core::animation::{
    AnimationCategory, AnimationTrigger, Direction, Easing, EffectType,
    ElementAnimation, SlideAnimationSequence, SLIDE_BACKGROUND_ID,
};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

use super::converter::{xml_str, xml_val};

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse a single slide's animation data.
///
/// `slide_xml` is the full content of `Slides/Slide_N.xml`.
/// `package_root` is the absolute path to the directory containing the
///   extracted ENBX files (used to resolve relative audio paths).
/// `page_size` is the logical page width in pixels (for percentage→pixel conversion).
pub fn parse_slide_animations(
    slide_xml: &str,
    package_root: &std::path::Path,
    page_size: [f32; 2],
) -> (HashMap<Uuid, ElementAnimation>, Option<SlideAnimationSequence>) {
    let anim_map = parse_animations(slide_xml, package_root, page_size);
    let sequence = parse_animation_orders(slide_xml, &anim_map);
    (anim_map, sequence)
}

// ---------------------------------------------------------------------------
// <Animations> → HashMap<Uuid, ElementAnimation>
// ---------------------------------------------------------------------------

fn parse_animations(
    xml: &str,
    package_root: &std::path::Path,
    page_size: [f32; 2],
) -> HashMap<Uuid, ElementAnimation> {
    let mut map = HashMap::new();

    // Locate <Animations>…</Animations>
    let start = match xml.find("<Animations>") {
        Some(p) => p + 12,
        None => return map,
    };
    let end = match xml[start..].find("</Animations>") {
        Some(p) => start + p,
        None => return map,
    };
    let block = &xml[start..end];

    let mut rest = block;
    while let Some(pos) = rest.find("<AnimationSaveInfo>") {
        rest = &rest[pos..];
        let close = find_close_depth(rest, "AnimationSaveInfo");
        let node = &rest[..close];

        if let Some(anim) = parse_one_animation(node, package_root, page_size) {
            map.insert(anim.id, anim);
        }
        if close >= rest.len() {
            break;
        }
        rest = &rest[close..];
    }

    map
}

fn parse_one_animation(
    xml: &str,
    package_root: &std::path::Path,
    page_size: [f32; 2],
) -> Option<ElementAnimation> {
    // --- Id ---------------------------------------------------------------
    let id: Uuid = xml_str(xml, "Id")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(Uuid::new_v4);

    // --- Target element ---------------------------------------------------
    let target_element_id: Uuid = xml_str(xml, "TargetElementId")
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(Uuid::new_v4);

    // --- Category ---------------------------------------------------------
    let category = match xml_str(xml, "AnimationCategory")
        .as_deref()
        .unwrap_or("Enter")
    {
        "Exit" => AnimationCategory::Exit,
        "Emphasis" => AnimationCategory::Emphasis,
        _ => AnimationCategory::Enter,
    };

    // --- Trigger ----------------------------------------------------------
    let trigger = match xml_str(xml, "AnimationTrigger")
        .as_deref()
        .unwrap_or("Before")
    {
        "Click" => AnimationTrigger::Click,
        "After" => AnimationTrigger::After,
        _ => AnimationTrigger::Before,
    };

    // --- Trigger source (for Click) ---------------------------------------
    let trigger_source_id: Option<Uuid> = xml_str(xml, "TriggerSource")
        .and_then(|s| s.parse().ok());

    // --- Effect type ------------------------------------------------------
    let effect = parse_effect_type(
        xml_str(xml, "EffectType").as_deref().unwrap_or("FadeIn"),
    );

    // --- Easing -----------------------------------------------------------
    let easing = match xml_str(xml, "Easing").as_deref().unwrap_or("Linear") {
        "EaseInOut" => Easing::EaseInOut,
        "Bounce" => Easing::Bounce,
        _ => Easing::Linear,
    };

    // --- Timing -----------------------------------------------------------
    let begin_time = xml_val(xml, "BeginTime")
        .map(|v| Duration::from_millis(v as u64))
        .unwrap_or_default();
    let duration = xml_val(xml, "Duration")
        .map(|v| Duration::from_millis(v as u64))
        .unwrap_or(Duration::from_millis(500));

    // --- Orientation -------------------------------------------------------
    let orientation = xml_str(xml, "Orientation").and_then(|s| match s.as_str() {
        "Top" => Some(Direction::Top),
        "Bottom" => Some(Direction::Bottom),
        "Left" => Some(Direction::Left),
        "Right" => Some(Direction::Right),
        "TopLeft" => Some(Direction::TopLeft),
        "TopRight" => Some(Direction::TopRight),
        "BottomLeft" => Some(Direction::BottomLeft),
        "BottomRight" => Some(Direction::BottomRight),
        _ => None,
    });

    // --- Magnitude --------------------------------------------------------
    let magnitude = xml_val(xml, "Magnitude").unwrap_or(1.0);

    // --- Repeat -----------------------------------------------------------
    let repeat = xml_val(xml, "Repeat").unwrap_or(0.0) as u32;

    // --- Audio ------------------------------------------------------------
    let audio_path = xml_str(xml, "AudioPath").or_else(|| xml_str(xml, "Sound"));
    let audio_path = audio_path.map(|rel| {
        if std::path::Path::new(&rel).is_absolute() {
            rel
        } else {
            package_root.join(&rel).to_string_lossy().into_owned()
        }
    });

    // --- Position targets -------------------------------------------------
    let to_x = xml_val(xml, "ToX");
    let to_y = xml_val(xml, "ToY");
    let by_x = xml_val(xml, "ByX");
    let by_y = xml_val(xml, "ByY");

    // --- Distance ---------------------------------------------------------
    let distance = xml_val(xml, "Distance").or_else(|| {
        // Percentage form: <DistancePercent>25</DistancePercent>
        xml_val(xml, "DistancePercent")
            .map(|pct| page_size[0].max(page_size[1]) * pct / 100.0)
    });

    Some(ElementAnimation {
        id,
        category,
        trigger,
        effect,
        easing,
        target_element_id,
        trigger_source_id,
        begin_time,
        duration,
        orientation,
        magnitude,
        repeat,
        audio_path,
        to_x,
        to_y,
        by_x,
        by_y,
        distance,
    })
}

// ---------------------------------------------------------------------------
// Effect name → EffectType (38 named + Unsupported fallback)
// ---------------------------------------------------------------------------

fn parse_effect_type(name: &str) -> EffectType {
    match name {
        // Enter
        "FadeIn" => EffectType::FadeIn,
        "TranslateInTop" => EffectType::TranslateInTop,
        "TranslateInBottom" => EffectType::TranslateInBottom,
        "TranslateInLeft" => EffectType::TranslateInLeft,
        "TranslateInRight" => EffectType::TranslateInRight,
        "ScaleIn" => EffectType::ScaleIn,
        "WipeInLeft" => EffectType::WipeInLeft,
        "WipeInRight" => EffectType::WipeInRight,
        "WipeInTop" => EffectType::WipeInTop,
        "WipeInBottom" => EffectType::WipeInBottom,
        "FlyInTopLeft" => EffectType::FlyInTopLeft,
        "FlyInTopRight" => EffectType::FlyInTopRight,
        "FlyInBottomLeft" => EffectType::FlyInBottomLeft,
        "FlyInBottomRight" => EffectType::FlyInBottomRight,
        "SplitInHorizontal" => EffectType::SplitInHorizontal,
        "SplitInVertical" => EffectType::SplitInVertical,
        // Exit
        "FadeOut" => EffectType::FadeOut,
        "TranslateOutTop" => EffectType::TranslateOutTop,
        "TranslateOutBottom" => EffectType::TranslateOutBottom,
        "TranslateOutLeft" => EffectType::TranslateOutLeft,
        "TranslateOutRight" => EffectType::TranslateOutRight,
        "ScaleOut" => EffectType::ScaleOut,
        "WipeOutLeft" => EffectType::WipeOutLeft,
        "WipeOutRight" => EffectType::WipeOutRight,
        "WipeOutTop" => EffectType::WipeOutTop,
        "WipeOutBottom" => EffectType::WipeOutBottom,
        "FlyOutTopLeft" => EffectType::FlyOutTopLeft,
        "FlyOutTopRight" => EffectType::FlyOutTopRight,
        "FlyOutBottomLeft" => EffectType::FlyOutBottomLeft,
        "FlyOutBottomRight" => EffectType::FlyOutBottomRight,
        "SplitOutHorizontal" => EffectType::SplitOutHorizontal,
        "SplitOutVertical" => EffectType::SplitOutVertical,
        // Emphasis
        "Transparency" => EffectType::Transparency,
        "Zoom" => EffectType::Zoom,
        "Heartbeat" => EffectType::Heartbeat,
        "Shake" => EffectType::Shake,
        "Wave" => EffectType::Wave,
        "Spin" => EffectType::Spin,
        "Pulse" => EffectType::Pulse,
        "Teeter" => EffectType::Teeter,
        "ColorBlend" => EffectType::ColorBlend,
        "GrowShrink" => EffectType::GrowShrink,
        "Darken" => EffectType::Darken,
        "Lighten" => EffectType::Lighten,
        other => EffectType::Unsupported(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// <AnimationOrders> → SlideAnimationSequence
// ---------------------------------------------------------------------------

fn parse_animation_orders(
    xml: &str,
    anim_map: &HashMap<Uuid, ElementAnimation>,
) -> Option<SlideAnimationSequence> {
    let start = xml.find("<AnimationOrders>")? + 17;
    let end = start + xml[start..].find("</AnimationOrders>")?;
    let block = &xml[start..end];

    // Collect all animation IDs in order
    let ordered_ids: Vec<Uuid> = block
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("<AnimationId>") && trimmed.ends_with("</AnimationId>") {
                let inner = trimmed
                    .strip_prefix("<AnimationId>")
                    .and_then(|s| s.strip_suffix("</AnimationId>"))
                    .unwrap_or("");
                inner.parse::<Uuid>().ok()
            } else {
                None
            }
        })
        .collect();

    if ordered_ids.is_empty() {
        return None;
    }

    let mut before_queue = Vec::new();
    let mut after_queue = Vec::new();
    let mut click_map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

    for &id in &ordered_ids {
        if let Some(anim) = anim_map.get(&id) {
            match anim.trigger {
                AnimationTrigger::Before => before_queue.push(id),
                AnimationTrigger::After => after_queue.push(id),
                AnimationTrigger::Click => {
                    let source = anim
                        .trigger_source_id
                        .unwrap_or(SLIDE_BACKGROUND_ID);
                    click_map.entry(source).or_default().push(id);
                }
            }
        }
    }

    // If we found no sequences at all, treat as "no animation"
    if before_queue.is_empty() && after_queue.is_empty() && click_map.is_empty() {
        return None;
    }

    Some(SlideAnimationSequence {
        before_queue,
        after_queue,
        click_map,
    })
}

// ---------------------------------------------------------------------------
// Depth‑tracking close‑tag finder (same logic as converter.rs)
// ---------------------------------------------------------------------------

fn find_close_depth(xml: &str, tag: &str) -> usize {
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}>");
    let mut depth: i32 = 0;
    let bytes = xml.as_bytes();
    let mut pos = 0usize;

    while pos < bytes.len() {
        if bytes[pos..].starts_with(open_pat.as_bytes()) {
            let after = pos + open_pat.len();
            if after < bytes.len() && matches!(bytes[after], b'>' | b' ' | b'\n' | b'\r' | b'\t' | b'/') {
                depth += 1;
                if after < bytes.len() && bytes[after] == b'/' && after + 1 < bytes.len() && bytes[after + 1] == b'>' {
                    depth -= 1;
                    if depth == 0 {
                        return pos + open_pat.len() + 2;
                    }
                    pos = after + 2;
                    continue;
                }
                if let Some(e) = xml[pos..].find('>') {
                    pos += e + 1;
                } else {
                    pos += 1;
                }
                continue;
            }
        }
        if bytes[pos..].starts_with(close_pat.as_bytes()) {
            depth -= 1;
            if depth <= 0 {
                return pos + close_pat.len();
            }
            pos += close_pat.len();
            continue;
        }
        pos += 1;
    }
    xml.len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn sample_xml() -> &'static str {
        r#"<Slide>
  <Width>1280</Width>
  <Height>720</Height>
  <Animations>
    <AnimationSaveInfo>
      <Id>11111111-1111-1111-1111-111111111111</Id>
      <AnimationCategory>Enter</AnimationCategory>
      <AnimationTrigger>Before</AnimationTrigger>
      <EffectType>FadeIn</EffectType>
      <TargetElementId>a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0</TargetElementId>
      <BeginTime>100</BeginTime>
      <Duration>500</Duration>
      <Easing>EaseInOut</Easing>
    </AnimationSaveInfo>
    <AnimationSaveInfo>
      <Id>22222222-2222-2222-2222-222222222222</Id>
      <AnimationCategory>Enter</AnimationCategory>
      <AnimationTrigger>After</AnimationTrigger>
      <EffectType>TranslateInTop</EffectType>
      <TargetElementId>b0b0b0b0-b0b0-b0b0-b0b0-b0b0b0b0b0b0</TargetElementId>
      <Duration>800</Duration>
      <Orientation>Top</Orientation>
      <Distance>300</Distance>
      <ToX>500</ToX>
      <ToY>300</ToY>
    </AnimationSaveInfo>
    <AnimationSaveInfo>
      <Id>33333333-3333-3333-3333-333333333333</Id>
      <AnimationCategory>Emphasis</AnimationCategory>
      <AnimationTrigger>Click</AnimationTrigger>
      <EffectType>Zoom</EffectType>
      <TargetElementId>c0c0c0c0-c0c0-c0c0-c0c0-c0c0c0c0c0c0</TargetElementId>
      <Duration>300</Duration>
      <Repeat>2</Repeat>
    </AnimationSaveInfo>
  </Animations>
  <AnimationOrders>
    <AnimationId>11111111-1111-1111-1111-111111111111</AnimationId>
    <AnimationId>22222222-2222-2222-2222-222222222222</AnimationId>
    <AnimationId>33333333-3333-3333-3333-333333333333</AnimationId>
  </AnimationOrders>
</Slide>"#
    }

    #[test]
    fn parse_basic_animations() {
        let root = Path::new("C:/fake/package");
        let (map, seq) = parse_slide_animations(sample_xml(), root, [1280.0, 720.0]);

        assert_eq!(map.len(), 3);
        let seq = seq.expect("sequence should exist");
        assert_eq!(seq.before_queue.len(), 1);
        assert_eq!(seq.after_queue.len(), 1);
        assert!(!seq.click_map.is_empty());

        let before_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let before_anim = map.get(&before_id).unwrap();
        assert_eq!(before_anim.category, AnimationCategory::Enter);
        assert_eq!(before_anim.trigger, AnimationTrigger::Before);
        assert_eq!(before_anim.effect, EffectType::FadeIn);
        assert_eq!(before_anim.easing, Easing::EaseInOut);
        assert_eq!(before_anim.begin_time, Duration::from_millis(100));
        assert_eq!(before_anim.duration, Duration::from_millis(500));

        let after_id = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let after_anim = map.get(&after_id).unwrap();
        assert_eq!(after_anim.to_x, Some(500.0));
        assert_eq!(after_anim.to_y, Some(300.0));
        assert_eq!(after_anim.orientation, Some(Direction::Top));
        assert_eq!(after_anim.distance, Some(300.0));

        let click_id = Uuid::parse_str("33333333-3333-3333-3333-333333333333").unwrap();
        let click_anim = map.get(&click_id).unwrap();
        assert_eq!(click_anim.repeat, 2);
        assert_eq!(click_anim.effect, EffectType::Zoom);
    }

    #[test]
    fn parse_unsupported_effect_falls_back() {
        let xml = r#"<Slide>
<Animations>
<AnimationSaveInfo>
  <Id>99999999-9999-9999-9999-999999999999</Id>
  <EffectType>CrazySpiral</EffectType>
  <TargetElementId>a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0</TargetElementId>
  <Duration>500</Duration>
</AnimationSaveInfo>
</Animations>
</Slide>"#;
        let (map, _) = parse_slide_animations(xml, Path::new("."), [1280.0, 720.0]);
        let anim = map.values().next().unwrap();
        assert!(anim.effect.is_unsupported());
    }

    #[test]
    fn parse_percentage_distance() {
        let xml = r#"<Slide>
<Animations>
<AnimationSaveInfo>
  <Id>aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa</Id>
  <EffectType>TranslateInLeft</EffectType>
  <TargetElementId>a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0</TargetElementId>
  <Duration>500</Duration>
  <DistancePercent>25</DistancePercent>
</AnimationSaveInfo>
</Animations>
</Slide>"#;
        let (map, _) = parse_slide_animations(xml, Path::new("."), [1280.0, 720.0]);
        let anim = map.values().next().unwrap();
        // page_size max = 1280, 25% = 320
        assert!((anim.distance.unwrap() - 320.0).abs() < 1.0);
    }

    #[test]
    fn no_animation_orders_returns_none() {
        let xml = r#"<Slide>
<Animations>
<AnimationSaveInfo>
  <Id>aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa</Id>
  <EffectType>FadeIn</EffectType>
  <TargetElementId>a0a0a0a0-a0a0-a0a0-a0a0-a0a0a0a0a0a0</TargetElementId>
  <Duration>500</Duration>
</AnimationSaveInfo>
</Animations>
</Slide>"#;
        let (_, seq) = parse_slide_animations(xml, Path::new("."), [1280.0, 720.0]);
        assert!(seq.is_none());
    }

    #[test]
    fn empty_slide_returns_empty() {
        let xml = "<Slide></Slide>";
        let (map, seq) = parse_slide_animations(xml, Path::new("."), [1280.0, 720.0]);
        assert!(map.is_empty());
        assert!(seq.is_none());
    }
}
