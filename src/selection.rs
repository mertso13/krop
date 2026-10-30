use crate::bgra_image::{BYTES_PER_PIXEL, BgraImage};
use windows::Win32::Foundation::{POINT, RECT};

pub const MINIMUM_SELECTION_SIZE: i32 = 5;

pub fn normalized_selection_rect(anchor: POINT, cursor: POINT, screen_width: i32, screen_height: i32) -> RECT {
    RECT {
        left: anchor.x.min(cursor.x).clamp(0, screen_width),
        top: anchor.y.min(cursor.y).clamp(0, screen_height),
        right: anchor.x.max(cursor.x).clamp(0, screen_width),
        bottom: anchor.y.max(cursor.y).clamp(0, screen_height),
    }
}

pub fn is_selection_large_enough(selection: &RECT) -> bool {
    selection.right - selection.left >= MINIMUM_SELECTION_SIZE
        && selection.bottom - selection.top >= MINIMUM_SELECTION_SIZE
}

pub fn crop_bgra_region(source_pixels: &[u8], source_width: u32, source_height: u32, region: &RECT) -> Option<BgraImage> {
    let region_is_inside_source = region.left >= 0
        && region.top >= 0
        && region.left < region.right
        && region.top < region.bottom
        && region.right <= source_width as i32
        && region.bottom <= source_height as i32
        && source_pixels.len() == source_width as usize * source_height as usize * BYTES_PER_PIXEL;
    if !region_is_inside_source {
        return None;
    }
    let source_stride = source_width as usize * BYTES_PER_PIXEL;
    let row_start_offset = region.left as usize * BYTES_PER_PIXEL;
    let row_end_offset = region.right as usize * BYTES_PER_PIXEL;
    let cropped_pixels = source_pixels
        .chunks_exact(source_stride)
        .skip(region.top as usize)
        .take((region.bottom - region.top) as usize)
        .flat_map(|source_row| &source_row[row_start_offset..row_end_offset])
        .copied()
        .collect();
    Some(BgraImage::new(
        (region.right - region.left) as u32,
        (region.bottom - region.top) as u32,
        cropped_pixels,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT { left, top, right, bottom }
    }

    #[test]
    fn selection_is_normalized_for_every_drag_direction() {
        let expected_selection = rect(10, 20, 50, 60);
        let corner_pairs = [
            (POINT { x: 10, y: 20 }, POINT { x: 50, y: 60 }),
            (POINT { x: 50, y: 60 }, POINT { x: 10, y: 20 }),
            (POINT { x: 50, y: 20 }, POINT { x: 10, y: 60 }),
            (POINT { x: 10, y: 60 }, POINT { x: 50, y: 20 }),
        ];
        for (anchor, cursor) in corner_pairs {
            assert_eq!(normalized_selection_rect(anchor, cursor, 100, 100), expected_selection);
        }
    }

    #[test]
    fn selection_is_clamped_to_screen_bounds() {
        let selection = normalized_selection_rect(POINT { x: -30, y: 80 }, POINT { x: 40, y: 250 }, 100, 200);
        assert_eq!(selection, rect(0, 80, 40, 200));
    }

    #[test]
    fn selections_smaller_than_five_pixels_are_rejected() {
        assert!(!is_selection_large_enough(&rect(10, 10, 10, 10)));
        assert!(!is_selection_large_enough(&rect(10, 10, 14, 30)));
        assert!(!is_selection_large_enough(&rect(10, 10, 30, 14)));
        assert!(is_selection_large_enough(&rect(10, 10, 15, 15)));
    }

    #[test]
    fn crop_copies_only_the_selected_pixels() {
        let source_pixels: Vec<u8> = (0..4 * 3).flat_map(|pixel_index| [pixel_index, 0, 0, 255]).collect();

        let cropped_image = crop_bgra_region(&source_pixels, 4, 3, &rect(1, 1, 3, 3)).unwrap();

        assert_eq!((cropped_image.width(), cropped_image.height()), (2, 2));
        assert_eq!(
            cropped_image.top_down_pixels(),
            &[5, 0, 0, 255, 6, 0, 0, 255, 9, 0, 0, 255, 10, 0, 0, 255]
        );
    }

    #[test]
    fn crop_rejects_regions_outside_source_or_empty() {
        let source_pixels = vec![0; 4 * 3 * BYTES_PER_PIXEL];
        assert!(crop_bgra_region(&source_pixels, 4, 3, &rect(2, 0, 5, 3)).is_none());
        assert!(crop_bgra_region(&source_pixels, 4, 3, &rect(-1, 0, 2, 3)).is_none());
        assert!(crop_bgra_region(&source_pixels, 4, 3, &rect(2, 2, 2, 3)).is_none());
    }
}
