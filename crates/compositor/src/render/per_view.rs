//! One cache per view a thing is drawn on.
//!
//! On the hardware every screen is drawn on its own, through its own view of
//! the global space: the same window, bar or capsule is a different element
//! on each screen it shows on (other place, other clipping). A single cache
//! would see its element change at every screen drawn and damage it again
//! each time; one cache per view keeps each screen's element still.

use coquille_model::Rect;

/// More views than any setup has: the three screens, the nested window,
/// a screenshot of one screen. The oldest is dropped past it.
const MAX_VIEWS: usize = 5;

#[derive(Debug)]
pub struct PerView<C> {
    entries: Vec<(Rect, C)>,
}

impl<C> Default for PerView<C> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<C: Default> PerView<C> {
    /// The cache of the view `view`, made the first time.
    pub fn get(&mut self, view: Rect) -> &mut C {
        let index = match self.entries.iter().position(|(seen, _)| *seen == view) {
            Some(index) => index,
            None => {
                if self.entries.len() == MAX_VIEWS {
                    self.entries.remove(0);
                }
                self.entries.push((view, C::default()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[index].1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_view_keeps_its_own_cache() {
        let (left, center) = (Rect::new(0, 0, 1920, 1080), Rect::new(1920, 0, 3440, 1440));
        let mut caches = PerView::<u32>::default();
        *caches.get(left) = 1;
        *caches.get(center) = 2;
        assert_eq!(*caches.get(left), 1);
        assert_eq!(*caches.get(center), 2);
    }

    #[test]
    fn the_oldest_view_goes_first() {
        let mut caches = PerView::<u32>::default();
        for index in 0..=MAX_VIEWS as i32 {
            *caches.get(Rect::new(index * 100, 0, 100, 100)) = index as u32 + 1;
        }
        assert_eq!(*caches.get(Rect::new(0, 0, 100, 100)), 0, "made again");
        assert_eq!(*caches.get(Rect::new(500, 0, 100, 100)), 6);
    }
}
