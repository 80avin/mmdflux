//! SVG text measurement utilities.

/// Mermaid default font family for SVG output.
pub const DEFAULT_FONT_FAMILY: &str = "\"trebuchet ms\", verdana, arial, sans-serif";
/// Mermaid default font size (px).
pub const DEFAULT_FONT_SIZE: f64 = 16.0;

#[derive(Debug, Clone)]
pub struct SvgTextMetrics {
    pub font_size: f64,
    pub line_height: f64,
    pub padding_x: f64,
    pub padding_y: f64,
}

impl SvgTextMetrics {
    pub fn new(font_size: f64) -> Self {
        Self {
            font_size,
            line_height: font_size * 1.2,
            padding_x: font_size * 0.4,
            padding_y: font_size * 0.3,
        }
    }

    pub fn measure_text(&self, text: &str) -> (f64, f64) {
        let lines: Vec<&str> = text.split('\n').collect();
        let line_count = lines.len().max(1) as f64;
        let max_width = lines
            .iter()
            .map(|line| self.measure_line_width(line))
            .fold(0.0, f64::max);
        let width = max_width + self.padding_x * 2.0;
        let height = self.line_height * line_count + self.padding_y * 2.0;
        (width, height)
    }

    pub fn node_dimensions(&self, label: &str) -> (f64, f64) {
        self.measure_text(label)
    }

    pub fn edge_label_dimensions(&self, label: &str) -> (f64, f64) {
        self.measure_text(label)
    }

    fn measure_line_width(&self, text: &str) -> f64 {
        text.chars()
            .map(|c| self.char_width_ratio(c) * self.font_size)
            .sum::<f64>()
    }

    fn char_width_ratio(&self, c: char) -> f64 {
        match c {
            'i' | 'l' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' => 0.3,
            'f' | 'j' | 't' | 'r' => 0.35,
            'm' | 'w' | 'M' | 'W' => 0.75,
            'A'..='Z' => 0.65,
            _ => 0.55,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_text_uses_proportional_heuristic() {
        let metrics = SvgTextMetrics::new(16.0);
        let (w, h) = metrics.measure_text("ABC");

        assert!(w > 16.0);
        assert!(h > 16.0);
    }
}
