use ratatui::{buffer::Buffer, layout::Rect, style::Style};

use super::Palette;

pub(super) fn list_scroll_metrics(
    row_heights: &[u16],
    gaps_after: &[u16],
    body_height: u16,
    requested_start: usize,
) -> crate::pane::ScrollMetrics {
    if row_heights.is_empty() || body_height == 0 {
        return crate::pane::ScrollMetrics {
            offset_from_bottom: 0,
            max_offset_from_bottom: 0,
            viewport_rows: 0,
        };
    }

    let mut used = 0u16;
    let mut max_start = row_heights.len();
    for index in (0..row_heights.len()).rev() {
        let height = row_heights[index].max(1).min(body_height);
        let gap = gaps_after.get(index).copied().unwrap_or(0);
        if used.saturating_add(height).saturating_add(gap) > body_height {
            break;
        }
        used = used.saturating_add(height).saturating_add(gap);
        max_start = index;
    }
    max_start = max_start.min(row_heights.len().saturating_sub(1));
    let start = requested_start.min(max_start);

    let mut viewport_rows = 0usize;
    let mut used = 0u16;
    for (index, row_height) in row_heights.iter().enumerate().skip(start) {
        let height = (*row_height).max(1).min(body_height);
        if used.saturating_add(height) > body_height {
            break;
        }
        used = used.saturating_add(height);
        viewport_rows += 1;
        let gap = gaps_after.get(index).copied().unwrap_or(0);
        if used.saturating_add(gap) > body_height {
            break;
        }
        used = used.saturating_add(gap);
    }

    crate::pane::ScrollMetrics {
        offset_from_bottom: max_start.saturating_sub(start),
        max_offset_from_bottom: max_start,
        viewport_rows,
    }
}

pub(super) fn list_scroll_start_to_reveal(
    row_heights: &[u16],
    gaps_after: &[u16],
    body_height: u16,
    requested_start: usize,
    target: usize,
) -> usize {
    let mut metrics = list_scroll_metrics(row_heights, gaps_after, body_height, requested_start);
    let mut start = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    if target < start {
        return target;
    }
    while target >= start.saturating_add(metrics.viewport_rows)
        && start < metrics.max_offset_from_bottom
    {
        start = start.saturating_add(1);
        metrics = list_scroll_metrics(row_heights, gaps_after, body_height, start);
    }
    start
}

/// Width left for list rows: the scrollbar (if shown) takes the last column, `padding` blank
/// columns sit before it (or before the right edge). Rows always keep at least one column.
pub(super) fn list_content_width(width: u16, show_scrollbar: bool, padding: u16) -> u16 {
    width
        .saturating_sub(u16::from(show_scrollbar))
        .saturating_sub(padding)
        .max(1)
}

/// Splits the list width into the row text width and the row padding behind it (the text
/// always keeps at least one column).
pub(super) fn list_row_width(width: u16, row_padding: u16) -> (u16, u16) {
    let text = width.saturating_sub(row_padding).max(1).min(width);
    (text, width - text)
}

/// Continues each line's background (taken from the last text column) over the `padding`
/// columns to the right of `text`, so a row highlight spans the padding as well.
pub(super) fn extend_row_backgrounds(buffer: &mut Buffer, text: Rect, padding: u16) {
    if padding == 0 || text.width == 0 {
        return;
    }
    let last = text.right() - 1;
    for y in text.y..text.bottom() {
        let Some(bg) = buffer.cell((last, y)).map(|cell| cell.bg) else {
            continue;
        };
        for x in text.right()..text.right().saturating_add(padding) {
            if let Some(cell) = buffer.cell_mut((x, y)) {
                cell.set_symbol(" ").set_bg(bg);
            }
        }
    }
}

pub(super) fn render_list_scrollbar(
    buffer: &mut Buffer,
    track: Rect,
    metrics: crate::pane::ScrollMetrics,
    palette: &Palette,
) {
    let Some(thumb) = crate::ui::scrollbar_thumb(metrics, track) else {
        return;
    };
    for row in track.y..track.bottom() {
        if let Some(cell) = buffer.cell_mut((track.x, row)) {
            cell.set_symbol("▕")
                .set_style(Style::default().fg(palette.surface_dim));
        }
    }
    for row in thumb.top..thumb.top.saturating_add(thumb.len) {
        if let Some(cell) = buffer.cell_mut((track.x, row)) {
            cell.set_symbol("▕")
                .set_style(Style::default().fg(palette.overlay0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_row_width_keeps_one_text_column() {
        assert_eq!(list_row_width(30, 0), (30, 0));
        assert_eq!(list_row_width(30, 2), (28, 2));
        assert_eq!(list_row_width(2, 5), (1, 1));
        assert_eq!(list_row_width(0, 1), (0, 0));
    }

    #[test]
    fn extend_row_backgrounds_carries_the_row_highlight() {
        use ratatui::style::Color;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 2));
        buffer.set_string(0, 0, "abcdef", Style::default().bg(Color::Red));
        extend_row_backgrounds(&mut buffer, Rect::new(0, 0, 4, 2), 2);
        assert_eq!(buffer[(4, 0)].symbol(), " ");
        assert_eq!(buffer[(5, 0)].bg, Color::Red);
        assert_eq!(buffer[(5, 1)].bg, Color::Reset);
    }

    #[test]
    fn list_content_width_reserves_scrollbar_and_padding() {
        assert_eq!(list_content_width(30, false, 0), 30);
        assert_eq!(list_content_width(30, false, 2), 28);
        assert_eq!(list_content_width(30, true, 0), 29);
        assert_eq!(list_content_width(30, true, 1), 28);
        assert_eq!(list_content_width(3, true, 5), 1);
    }

    #[test]
    fn list_metrics_preserve_variable_rows_and_caller_owned_gap_policy() {
        let top = list_scroll_metrics(&[1, 3, 2], &[1, 1, 0], 5, 0);
        assert_eq!(top.max_offset_from_bottom, 2);
        assert_eq!(top.offset_from_bottom, 2);
        assert_eq!(top.viewport_rows, 2);

        let bottom = list_scroll_metrics(&[1, 3, 2], &[1, 1, 0], 5, usize::MAX);
        assert_eq!(bottom.offset_from_bottom, 0);
        assert_eq!(bottom.viewport_rows, 1);

        let parent_child = list_scroll_metrics(&[2, 2, 2], &[0, 1, 0], 5, 0);
        assert_eq!(parent_child.max_offset_from_bottom, 1);
        assert_eq!(parent_child.viewport_rows, 2);
    }
}
