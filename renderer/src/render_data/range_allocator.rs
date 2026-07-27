use std::ops::Range;

use super::RenderDataError;

#[derive(Default, Debug)]
pub(super) struct RangeAllocator {
    free: Vec<Range<u32>>,
    pub(super) high_water: u32,
}

impl RangeAllocator {
    pub fn allocate(&mut self, count: u32) -> Result<Range<u32>, RenderDataError> {
        if count == 0 {
            return Err(RenderDataError::EmptyRange);
        }
        if let Some(index) = self
            .free
            .iter()
            .position(|range| range.end - range.start >= count)
        {
            let start = self.free[index].start;
            let end = start
                .checked_add(count)
                .ok_or(RenderDataError::RangeOverflow)?;
            self.free[index].start = end;
            if self.free[index].is_empty() {
                self.free.remove(index);
            }
            return Ok(start..end);
        }
        let end = self
            .high_water
            .checked_add(count)
            .ok_or(RenderDataError::RangeOverflow)?;
        let range = self.high_water..end;
        self.high_water = end;
        Ok(range)
    }

    pub fn free(&mut self, range: Range<u32>) -> Result<u32, RenderDataError> {
        if range.start >= range.end {
            return Err(RenderDataError::EmptyRange);
        }
        if range.end > self.high_water {
            return Err(RenderDataError::RangeOutOfBounds);
        }
        let index = self
            .free
            .partition_point(|candidate| candidate.start < range.start);
        if index > 0 && self.free[index - 1].end > range.start
            || index < self.free.len() && self.free[index].start < range.end
        {
            return Err(RenderDataError::RangeOverlap);
        }

        let joins_left = index > 0 && self.free[index - 1].end == range.start;
        let joins_right = index < self.free.len() && self.free[index].start == range.end;
        match (joins_left, joins_right) {
            (true, true) => {
                let right_end = self.free.remove(index).end;
                self.free[index - 1].end = right_end;
            }
            (true, false) => self.free[index - 1].end = range.end,
            (false, true) => self.free[index].start = range.start,
            (false, false) => self.free.insert(index, range),
        }
        while self
            .free
            .last()
            .is_some_and(|range| range.end == self.high_water)
        {
            self.high_water = self.free.pop().unwrap().start;
        }
        Ok(self.high_water)
    }

    pub fn high_water(&self) -> u32 {
        self.high_water
    }

    pub fn clear(&mut self) {
        self.free.clear();
        self.high_water = 0;
    }
}
