use std::collections::HashMap;

/// Smart highlighter-alpha calculator based on page background and content density.
pub struct SmartAlpha {
    recommendations: HashMap<[u8; 3], u8>,
    bg_luminance: f32,
    content_density: f32,
    last_analyzed_page: usize,
}

impl Default for SmartAlpha {
    fn default() -> Self {
        Self {
            recommendations: HashMap::new(),
            bg_luminance: 240.0,
            content_density: 0.3,
            last_analyzed_page: usize::MAX,
        }
    }
}

impl SmartAlpha {
    /// Analyse the current page and recompute recommendations.
    /// Returns true if any value changed from the previous analysis.
    pub fn analyze_page(
        &mut self,
        page_index: usize,
        bg_color: &[u8; 3],
        element_count: usize,
        canvas_width: f32,
        canvas_height: f32,
    ) -> bool {
        if self.last_analyzed_page == page_index {
            return false; // already analysed
        }
        self.last_analyzed_page = page_index;

        let old_map = self.recommendations.clone();

        // Rec. 709 luminance
        let r = bg_color[0] as f32;
        let g = bg_color[1] as f32;
        let b = bg_color[2] as f32;
        self.bg_luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;

        // Content density – elements per 10 000 px²
        let area = canvas_width * canvas_height;
        self.content_density = ((element_count as f32) / (area / 10000.0))
            .min(1.0)
            .max(0.0);

        self.recompute_all();

        // Emit report
        eprintln!(
            "[smart_alpha] page={} luminance={:.0} density={:.0}%",
            page_index,
            self.bg_luminance,
            self.content_density * 100.0,
        );
        for (rgb, alpha) in &self.recommendations {
            eprintln!("  rgb={:?} → α={}", rgb, alpha);
        }

        old_map != self.recommendations
    }

    fn recompute_all(&mut self) {
        self.recommendations.clear();

        let colors: [([u8; 3], &str); 5] = [
            ([255, 230, 0], "yellow"),
            ([50, 200, 80], "green"),
            ([255, 80, 160], "pink"),
            ([30, 120, 255], "blue"),
            ([255, 140, 0], "orange"),
        ];

        for (rgb, _) in &colors {
            self.recommendations.insert(*rgb, self.compute_alpha(rgb));
        }
    }

    /// Core formula: base + luminance correction + density penalty + colour boost.
    /// Result is clamped to [24, 232].
    fn compute_alpha(&self, rgb: &[u8; 3]) -> u8 {
        let base: f32 = 140.0;

        // ── Luminance adjustment ──
        let lum_factor = (self.bg_luminance - 128.0) / 128.0; // -1 .. +1
        let bg_adj = lum_factor * 40.0;

        // ── Density penalty ──
        let density_adj = -self.content_density * 50.0;

        // ── Colour visibility boost ──
        let boost = self.colour_visibility_boost(rgb);

        let raw = base + bg_adj + density_adj + boost;
        raw.max(24.0).min(232.0) as u8
    }

    /// How visible a colour is on a white-ish background.
    /// Darker/more-saturated colours need LESS extra alpha.
    /// Returns [-20, +20].
    fn colour_visibility_boost(&self, rgb: &[u8; 3]) -> f32 {
        let max_contrast = (255.0_f32.powi(2) * 3.0).sqrt(); // ≈ 441.67

        let contrast = ((255.0 - rgb[0] as f32).powi(2)
            + (255.0 - rgb[1] as f32).powi(2)
            + (255.0 - rgb[2] as f32).powi(2))
        .sqrt();

        let visibility = contrast / max_contrast; // 0 = white, 1 = black
        (1.0 - visibility) * 40.0 - 20.0
    }

    pub fn recommendation_for(&self, rgb: &[u8; 3]) -> Option<u8> {
        self.recommendations.get(rgb).copied()
    }

    #[allow(dead_code)]
    pub fn all_recommendations(&self) -> &HashMap<[u8; 3], u8> {
        &self.recommendations
    }

    #[allow(dead_code)]
    pub fn apply_to(&self, color: &mut [u8; 4], alphas: &mut HashMap<[u8; 3], u8>) -> bool {
        let rgb: [u8; 3] = [color[0], color[1], color[2]];
        if let Some(&a) = self.recommendations.get(&rgb) {
            color[3] = a;
            alphas.insert(rgb, a);
            true
        } else {
            false
        }
    }

    /// Push all recommendations into the alpha map.
    #[allow(dead_code)]
    pub fn apply_all(&self, alphas: &mut HashMap<[u8; 3], u8>) {
        for (rgb, a) in &self.recommendations {
            alphas.insert(*rgb, *a);
        }
    }

    #[allow(dead_code)]
    pub fn summary(&self) -> String {
        format!(
            "亮度:{:.0} 密度:{:.0}%",
            self.bg_luminance,
            self.content_density * 100.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_bg_high_alpha() {
        let mut s = SmartAlpha::default();
        s.analyze_page(0, &[255, 255, 255], 10, 1920.0, 1080.0);
        // White background → all recommendations should be above 150
        for (_, a) in s.all_recommendations() {
            assert!(*a > 140, "white bg alpha should be high, got {}", a);
        }
    }

    #[test]
    fn dark_bg_low_alpha() {
        let mut s = SmartAlpha::default();
        s.analyze_page(0, &[40, 40, 50], 5, 1920.0, 1080.0);
        // Dark background → alpha should be lower
        for (_, a) in s.all_recommendations() {
            assert!(*a <= 130, "dark bg alpha should be moderate, got {}", a);
        }
    }

    #[test]
    fn dense_content_lower_alpha() {
        let mut s = SmartAlpha::default();
        s.analyze_page(0, &[255, 255, 255], 10, 1920.0, 1080.0);
        let sparse = s.recommendation_for(&[255, 230, 0]).unwrap();

        let mut s2 = SmartAlpha::default();
        s2.analyze_page(1, &[255, 255, 255], 500, 1920.0, 1080.0);
        let dense = s2.recommendation_for(&[255, 230, 0]).unwrap();

        assert!(
            dense < sparse,
            "dense page alpha ({}) should be lower than sparse ({})",
            dense,
            sparse
        );
    }

    #[test]
    fn clamp_bounds() {
        let mut s = SmartAlpha::default();
        // Extremely bright bg, very sparse → push alpha high
        s.analyze_page(0, &[255, 255, 255], 0, 1920.0, 1080.0);
        for (_, a) in s.all_recommendations() {
            assert!(*a <= 232);
        }

        // Extremely dark bg, very dense → push alpha low
        let mut d = SmartAlpha::default();
        d.analyze_page(0, &[0, 0, 0], 9999, 1920.0, 1080.0);
        for (_, a) in d.all_recommendations() {
            assert!(*a >= 24);
        }
    }

    #[test]
    fn yellow_gets_highest_boost() {
        let mut s = SmartAlpha::default();
        s.analyze_page(0, &[255, 255, 255], 10, 1920.0, 1080.0);
        let yellow = s.recommendation_for(&[255, 230, 0]).unwrap();
        let blue = s.recommendation_for(&[30, 120, 255]).unwrap();
        // Yellow on white is hardest to see → should get higher alpha
        assert!(
            yellow > blue,
            "yellow alpha ({}) should exceed blue ({}) on white bg",
            yellow,
            blue
        );
    }

    #[test]
    fn apply_to_modifies_color() {
        let mut s = SmartAlpha::default();
        s.analyze_page(0, &[255, 255, 255], 10, 1920.0, 1080.0);
        let mut color = [255, 230, 0, 140];
        let mut alphas = HashMap::new();
        assert!(s.apply_to(&mut color, &mut alphas));
        let rec = s.recommendation_for(&[255, 230, 0]).unwrap();
        assert_eq!(color[3], rec);
    }

    #[test]
    fn same_page_no_recompute() {
        let mut s = SmartAlpha::default();
        assert!(s.analyze_page(0, &[255, 255, 255], 10, 1920.0, 1080.0));
        assert!(!s.analyze_page(0, &[100, 100, 100], 999, 1920.0, 1080.0));
    }
}
