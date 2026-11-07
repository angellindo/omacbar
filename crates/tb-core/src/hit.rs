//! Touch hit-testing along the bar's x axis.

use crate::layout::Placed;
use crate::model::ButtonId;
use crate::spacing::SpacingRules;

/// The button under a touch at `x`. A touch in a gap goes to the nearest button when it is within
/// the touch slop (half a gap); further away (edges, group gaps) it hits nothing.
pub fn hit<'a>(placed: &'a [Placed], x: u16, rules: &SpacingRules) -> Option<&'a ButtonId> {
    placed
        .iter()
        .map(|p| (p.rect.distance_x(x), p))
        .filter(|(d, _)| *d <= rules.touch_slop())
        .min_by_key(|(d, _)| *d)
        .map(|(_, p)| &p.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    fn at(id: &str, x: u16, width: u16) -> Placed {
        Placed { id: ButtonId::new(id), rect: Rect { x, y: 8, width, height: 44 }, group: 0 }
    }

    #[test]
    fn a_touch_inside_a_button_hits_it() {
        let placed = [at("a", 8, 100), at("b", 116, 100)];
        assert_eq!(hit(&placed, 50, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("a"));
        assert_eq!(hit(&placed, 116, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("b"));
    }

    #[test]
    fn a_touch_in_a_gap_goes_to_the_nearest_button() {
        let placed = [at("a", 8, 100), at("b", 116, 100)];
        assert_eq!(hit(&placed, 109, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("a"));
        assert_eq!(hit(&placed, 114, &SpacingRules::STANDARD).map(ButtonId::as_str), Some("b"));
    }

    #[test]
    fn a_touch_far_from_any_button_hits_nothing() {
        let placed = [at("a", 8, 100), at("b", 140, 100)];
        assert_eq!(hit(&placed, 124, &SpacingRules::STANDARD), None);
        assert_eq!(hit(&placed, 0, &SpacingRules::STANDARD), None);
    }
}
