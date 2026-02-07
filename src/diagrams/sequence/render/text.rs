//! Sequence diagram text renderer.
//!
//! Renders a `SequenceLayout` onto a shared `Canvas` using box-drawing
//! characters from `CharSet`. Supports both Unicode and ASCII output.

use crate::diagrams::sequence::layout::{
    ParticipantLayout, RowLayout, SELF_MSG_WIDTH, SequenceLayout,
};
use crate::diagrams::sequence::model::MessageStyle;
use crate::render::canvas::Canvas;
use crate::render::chars::CharSet;

/// Render a sequence layout to a string.
pub fn render(layout: &SequenceLayout, charset: &CharSet) -> String {
    if layout.participants.is_empty() {
        return String::new();
    }

    let mut canvas = Canvas::new(layout.width, layout.height);

    // Draw participant headers
    for p in &layout.participants {
        draw_participant_header(&mut canvas, p, charset);
    }

    // Draw lifelines (vertical lines under each participant)
    let lifeline_start = 3; // after header
    let lifeline_end = layout.height;
    for p in &layout.participants {
        for y in lifeline_start..lifeline_end {
            canvas.set(p.center_x, y, charset.vertical);
        }
    }

    // Draw event rows
    for row in &layout.rows {
        match row {
            RowLayout::Message {
                y,
                from_idx,
                to_idx,
                style,
                text,
                number,
            } => {
                let from_x = layout.participants[*from_idx].center_x;
                let to_x = layout.participants[*to_idx].center_x;

                if from_idx == to_idx {
                    draw_self_message(&mut canvas, from_x, *y, text, number, style, charset);
                } else {
                    draw_message(&mut canvas, from_x, to_x, *y, text, number, style, charset);
                }
            }
            RowLayout::Note { y, over_idx, text } => {
                let center_x = layout.participants[*over_idx].center_x;
                draw_note(&mut canvas, center_x, *y, text, charset);
            }
        }
    }

    canvas.to_string()
}

/// Draw a participant header box.
fn draw_participant_header(canvas: &mut Canvas, p: &ParticipantLayout, cs: &CharSet) {
    let x = p.box_x;
    let w = p.box_width;

    // Top border: ┌───┐
    canvas.set(x, 0, cs.corner_tl);
    for i in 1..w - 1 {
        canvas.set(x + i, 0, cs.horizontal);
    }
    canvas.set(x + w - 1, 0, cs.corner_tr);

    // Label row: │ Label │
    canvas.set(x, 1, cs.vertical);
    canvas.set(x + 1, 1, ' ');
    canvas.write_str(x + 2, 1, &p.label);
    canvas.set(x + 2 + p.label.len(), 1, ' ');
    canvas.set(x + w - 1, 1, cs.vertical);

    // Bottom border with tee: └──┬──┘
    canvas.set(x, 2, cs.corner_bl);
    for i in 1..w - 1 {
        canvas.set(x + i, 2, cs.horizontal);
    }
    canvas.set(x + w - 1, 2, cs.corner_br);

    // Tee junction at center (lifeline goes down from here)
    canvas.set(p.center_x, 2, cs.tee_down);
}

/// Draw a message arrow between two lifelines.
#[allow(clippy::too_many_arguments)]
fn draw_message(
    canvas: &mut Canvas,
    from_x: usize,
    to_x: usize,
    y: usize,
    text: &str,
    number: &Option<usize>,
    style: &MessageStyle,
    cs: &CharSet,
) {
    let left_to_right = to_x > from_x;
    let (start_x, end_x) = if left_to_right {
        (from_x + 1, to_x)
    } else {
        (to_x + 1, from_x)
    };

    // Draw the arrow line first
    let (line_char, arrow_char) = match style {
        MessageStyle::Solid => (cs.horizontal, if left_to_right { '>' } else { '<' }),
        MessageStyle::Dashed => (cs.dotted_horizontal, if left_to_right { '>' } else { '<' }),
    };

    for x in start_x..end_x {
        canvas.set(x, y, line_char);
    }

    // Place the arrowhead
    if left_to_right {
        canvas.set(end_x - 1, y, arrow_char);
    } else {
        canvas.set(start_x, y, arrow_char);
    }

    // Draw the label on top of the arrow line (overwrites line chars)
    let label = format_label(text, number);
    if !label.is_empty() {
        let label_x = start_x + 1;
        canvas.write_str(label_x, y, &label);
    }
}

/// Draw a self-message (loop back to same participant).
fn draw_self_message(
    canvas: &mut Canvas,
    center_x: usize,
    y: usize,
    text: &str,
    number: &Option<usize>,
    _style: &MessageStyle,
    cs: &CharSet,
) {
    // Self-message layout:
    // Row y:   ├──┐  label
    // Row y+1: │  │
    // Row y+2: ◄──┘

    let arm_end = center_x + SELF_MSG_WIDTH;

    // Row 0: outgoing arm ├──┐
    canvas.set(center_x, y, cs.tee_right);
    for x in (center_x + 1)..arm_end {
        canvas.set(x, y, cs.horizontal);
    }
    canvas.set(arm_end, y, cs.corner_tr);

    // Label after the arm
    let label = format_label(text, number);
    if !label.is_empty() {
        canvas.write_str(arm_end + 2, y, &label);
    }

    // Row 1: vertical │  │
    canvas.set(arm_end, y + 1, cs.vertical);

    // Row 2: return arm ◄──┘
    canvas.set(center_x, y + 2, '<');
    for x in (center_x + 1)..arm_end {
        canvas.set(x, y + 2, cs.horizontal);
    }
    canvas.set(arm_end, y + 2, cs.corner_br);
}

/// Draw a note box over a participant.
fn draw_note(canvas: &mut Canvas, center_x: usize, y: usize, text: &str, cs: &CharSet) {
    let box_width = text.len() + 4; // borders + padding
    let box_x = center_x.saturating_sub(box_width / 2);

    // Top border
    canvas.set(box_x, y, cs.corner_tl);
    for i in 1..box_width - 1 {
        canvas.set(box_x + i, y, cs.horizontal);
    }
    canvas.set(box_x + box_width - 1, y, cs.corner_tr);

    // Text row
    canvas.set(box_x, y + 1, cs.vertical);
    canvas.set(box_x + 1, y + 1, ' ');
    canvas.write_str(box_x + 2, y + 1, text);
    canvas.set(box_x + 2 + text.len(), y + 1, ' ');
    canvas.set(box_x + box_width - 1, y + 1, cs.vertical);

    // Bottom border
    canvas.set(box_x, y + 2, cs.corner_bl);
    for i in 1..box_width - 1 {
        canvas.set(box_x + i, y + 2, cs.horizontal);
    }
    canvas.set(box_x + box_width - 1, y + 2, cs.corner_br);
}

/// Format a message label with optional autonumber prefix.
fn format_label(text: &str, number: &Option<usize>) -> String {
    match number {
        Some(n) => {
            if text.is_empty() {
                format!("{n}.")
            } else {
                format!("{n}. {text}")
            }
        }
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::sequence::parser::parse_sequence;
    use crate::diagrams::sequence::{compiler, layout};

    fn render_input(input: &str) -> String {
        let stmts = parse_sequence(input).unwrap();
        let model = compiler::compile(&stmts).unwrap();
        let seq_layout = layout::layout(&model);
        let cs = CharSet::unicode();
        render(&seq_layout, &cs)
    }

    fn render_ascii(input: &str) -> String {
        let stmts = parse_sequence(input).unwrap();
        let model = compiler::compile(&stmts).unwrap();
        let seq_layout = layout::layout(&model);
        let cs = CharSet::ascii();
        render(&seq_layout, &cs)
    }

    #[test]
    fn render_empty() {
        let stmts = parse_sequence("sequenceDiagram\n").unwrap();
        let model = compiler::compile(&stmts).unwrap();
        let seq_layout = layout::layout(&model);
        let cs = CharSet::unicode();
        let output = render(&seq_layout, &cs);
        assert!(output.is_empty());
    }

    #[test]
    fn render_two_participants_with_message() {
        let output = render_input("sequenceDiagram\nparticipant A\nparticipant B\nA->>B: hello");
        assert!(output.contains("┌───┐"), "should have header box");
        assert!(output.contains("│ A │"), "should show participant A");
        assert!(output.contains("│ B │"), "should show participant B");
        assert!(output.contains(">"), "should have arrowhead");
        assert!(output.contains("hello"), "should show message label");
    }

    #[test]
    fn render_dashed_message() {
        let output = render_input("sequenceDiagram\nparticipant A\nparticipant B\nA-->>B: reply");
        assert!(output.contains("reply"));
        // Dashed line uses dotted horizontal char
        assert!(
            output.contains('┄'),
            "should use dotted horizontal for dashed arrows"
        );
    }

    #[test]
    fn render_self_message() {
        let output = render_input("sequenceDiagram\nparticipant A\nA->>A: think");
        assert!(output.contains("think"));
        assert!(output.contains("┐"), "should have loop corner");
        assert!(output.contains("┘"), "should have return corner");
        assert!(output.contains("<"), "should have return arrow");
    }

    #[test]
    fn render_note() {
        let output = render_input("sequenceDiagram\nparticipant A\nNote over A: done");
        assert!(output.contains("done"));
        // Note should be in a box
        assert!(output.contains("┌"), "note should have border");
        assert!(output.contains("┘"), "note should have border");
    }

    #[test]
    fn render_autonumber() {
        let output = render_input(
            "sequenceDiagram\nautonumber\nparticipant A\nparticipant B\nA->>B: first\nB->>A: second",
        );
        assert!(output.contains("1. first"));
        assert!(output.contains("2. second"));
    }

    #[test]
    fn render_ascii_mode() {
        let output = render_ascii("sequenceDiagram\nparticipant A\nparticipant B\nA->>B: hi");
        assert!(output.contains("+"), "ASCII mode should use + for corners");
        assert!(
            output.contains("| A |"),
            "ASCII mode should use | for borders"
        );
    }

    #[test]
    fn render_deterministic() {
        let input = "sequenceDiagram\nparticipant A\nparticipant B\nA->>B: hello\nB-->>A: reply";
        let out1 = render_input(input);
        let out2 = render_input(input);
        assert_eq!(out1, out2, "rendering must be deterministic");
    }

    #[test]
    fn render_right_to_left_message() {
        let output = render_input("sequenceDiagram\nparticipant A\nparticipant B\nB->>A: back");
        assert!(output.contains("<"), "right-to-left should have < arrow");
        assert!(output.contains("back"));
    }
}
