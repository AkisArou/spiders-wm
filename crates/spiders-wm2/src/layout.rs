use smithay::utils::{Logical, Point, Rectangle, Size};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayoutSlot {
    pub location: Point<i32, Logical>,
    pub size: Size<i32, Logical>,
}

pub fn plan_tiled_slot(
    output_geometry: Rectangle<i32, Logical>,
    count: usize,
    index: usize,
) -> Option<RelayoutSlot> {
    if count == 0 || index >= count {
        return None;
    }

    let output_width = output_geometry.size.w.max(1);
    let output_height = output_geometry.size.h.max(1);

    if count == 1 {
        return Some(RelayoutSlot {
            location: output_geometry.loc,
            size: Size::from((output_width, output_height)),
        });
    }

    let master_width = ((output_width * 3) / 5).max(1);
    let stack_width = (output_width - master_width).max(1);

    if index == 0 {
        return Some(RelayoutSlot {
            location: output_geometry.loc,
            size: Size::from((master_width, output_height)),
        });
    }

    let stack_count = (count - 1) as i32;
    let stack_index = (index - 1) as i32;
    let base_height = (output_height / stack_count).max(1);
    let remainder = output_height.rem_euclid(stack_count);
    let height = (base_height + i32::from(stack_index < remainder)).max(1);
    let y = output_geometry.loc.y + stack_index * base_height + remainder.min(stack_index);

    Some(RelayoutSlot {
        location: Point::from((output_geometry.loc.x + master_width, y)),
        size: Size::from((stack_width, height)),
    })
}

pub fn plan_tiled_slots(
    output_geometry: Rectangle<i32, Logical>,
    count: usize,
) -> Vec<RelayoutSlot> {
    (0..count)
        .filter_map(|index| plan_tiled_slot(output_geometry, count, index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_window_uses_entire_output() {
        let output = Rectangle::new((10, 20).into(), (100, 50).into());
        let slots = plan_tiled_slots(output, 1);

        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].location, Point::from((10, 20)));
        assert_eq!(slots[0].size, Size::from((100, 50)));
    }

    #[test]
    fn master_stack_uses_master_column_for_first_window() {
        let output = Rectangle::new((10, 20).into(), (100, 50).into());
        let slots = plan_tiled_slots(output, 2);

        assert_eq!(slots.len(), 2);
        assert_eq!(slots[0].location, Point::from((10, 20)));
        assert_eq!(slots[0].size, Size::from((60, 50)));
        assert_eq!(slots[1].location, Point::from((70, 20)));
        assert_eq!(slots[1].size, Size::from((40, 50)));
    }

    #[test]
    fn stack_windows_split_secondary_column_by_height() {
        let output = Rectangle::new((10, 20).into(), (100, 101).into());
        let slots = plan_tiled_slots(output, 4);

        assert_eq!(slots.len(), 4);
        assert_eq!(slots[0].location, Point::from((10, 20)));
        assert_eq!(slots[0].size, Size::from((60, 101)));
        assert_eq!(slots[1].location, Point::from((70, 20)));
        assert_eq!(slots[1].size, Size::from((40, 34)));
        assert_eq!(slots[2].location, Point::from((70, 54)));
        assert_eq!(slots[2].size, Size::from((40, 34)));
        assert_eq!(slots[3].location, Point::from((70, 88)));
        assert_eq!(slots[3].size, Size::from((40, 33)));
    }

    #[test]
    fn tiled_slot_rejects_out_of_bounds_index() {
        let output = Rectangle::new((0, 0).into(), (10, 10).into());
        assert!(plan_tiled_slot(output, 0, 0).is_none());
        assert!(plan_tiled_slot(output, 2, 2).is_none());
    }
}