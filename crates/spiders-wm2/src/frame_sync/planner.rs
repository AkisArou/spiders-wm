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

    let count = count as i32;
    let index = index as i32;
    let base_width = (output_geometry.size.w / count).max(1);
    let remainder = output_geometry.size.w.rem_euclid(count);
    let width = (base_width + i32::from(index < remainder)).max(1);
    let x = output_geometry.loc.x + index * base_width + remainder.min(index);

    Some(RelayoutSlot {
        location: Point::from((x, output_geometry.loc.y)),
        size: Size::from((width, output_geometry.size.h.max(1))),
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
    fn tiled_slots_distribute_remainder_from_the_left() {
        let output = Rectangle::new((10, 20).into(), (100, 50).into());
        let slots = plan_tiled_slots(output, 3);

        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].location, Point::from((10, 20)));
        assert_eq!(slots[0].size, Size::from((34, 50)));
        assert_eq!(slots[1].location, Point::from((44, 20)));
        assert_eq!(slots[1].size, Size::from((33, 50)));
        assert_eq!(slots[2].location, Point::from((77, 20)));
        assert_eq!(slots[2].size, Size::from((33, 50)));
    }

    #[test]
    fn tiled_slot_rejects_out_of_bounds_index() {
        let output = Rectangle::new((0, 0).into(), (10, 10).into());
        assert!(plan_tiled_slot(output, 0, 0).is_none());
        assert!(plan_tiled_slot(output, 2, 2).is_none());
    }
}